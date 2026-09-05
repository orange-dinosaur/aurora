use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tauri::{AppHandle, Manager};
use time::OffsetDateTime;

use crate::document::{Document, refresh};
use crate::store;
use crate::tree::{self, Node, tree_from_flat};

/// A folder inside a project, together with the document it starts life with.
/// A section with no seed is created empty.
pub struct Section {
	pub folder: &'static str,
	pub seed: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
	Novel,
	Screenplay,
	ShortStories,
	StagePlay,
}

/// The one section with a hierarchy of its own. A folder anywhere else is a
/// folder and nothing more.
pub const MANUSCRIPT: &str = "Manuscript";

const NOVEL: &[Section] = &[
	Section {
		folder: "Manuscript",
		seed: Some("Scene 1.md"),
	},
	Section {
		folder: "Outline",
		seed: Some("Outline.md"),
	},
	// Characters and Locations fill up a subject at a time, so a document
	// standing in for the whole folder only gets in the way.
	Section {
		folder: "Characters",
		seed: None,
	},
	Section {
		folder: "Locations",
		seed: None,
	},
	Section {
		folder: "Notes",
		seed: Some("Notes.md"),
	},
];

impl Format {
	/// Every format, in the order the Welcome screen offers them. Keep in step
	/// with the enum.
	pub const ALL: &'static [Format] = &[
		Format::Novel,
		Format::Screenplay,
		Format::ShortStories,
		Format::StagePlay,
	];

	/// The sections a new project of this format is created with. An empty
	/// layout means the format cannot be created yet.
	pub fn layout(self) -> &'static [Section] {
		match self {
			Format::Novel => NOVEL,
			Format::Screenplay | Format::ShortStories | Format::StagePlay => &[],
		}
	}
}

/// A format as the Welcome screen needs to describe it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FormatLayout {
	pub format: Format,
	pub folders: Vec<String>,
	pub available: bool,
}

/// The folders each format creates, so the Welcome screen does not have to keep
/// its own copy of them.
#[tauri::command]
pub fn format_layouts() -> Vec<FormatLayout> {
	Format::ALL
		.iter()
		.map(|&format| {
			let folders: Vec<String> = format
				.layout()
				.iter()
				.map(|s| s.folder.to_owned())
				.collect();
			FormatLayout {
				format,
				available: !folders.is_empty(),
				folders,
			}
		})
		.collect()
}

/// Bumped when the on-disk shape changes in a way older builds cannot read.
pub const MANIFEST_VERSION: u32 = 3;

/// The contents of `aurora.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
	pub version: u32,
	pub name: String,
	pub format: Format,
	#[serde(with = "time::serde::rfc3339")]
	pub created_at: OffsetDateTime,
	/// Everything the project holds: its sections in the order it was made
	/// with, and under each of them the folders and documents the writer put
	/// there. A project keeps its own sections even if the format's definition
	/// changes later.
	pub nodes: Vec<Node>,
}

impl Manifest {
	pub fn new(name: impl Into<String>, format: Format, created_at: OffsetDateTime) -> Self {
		Self {
			version: MANIFEST_VERSION,
			name: name.into(),
			format,
			// The manifest is meant to be readable; sub-second precision is noise.
			created_at: created_at
				.replace_nanosecond(0)
				.expect("zero nanoseconds is always in range"),
			// A section is a folder like any other, so a new project is one
			// empty folder per section. The seed documents are attached by
			// `fill`, which is also what puts them on disk.
			nodes: format
				.layout()
				.iter()
				.map(|s| Node::folder(s.folder, Vec::new()))
				.collect(),
		}
	}

	/// The project's sections, in order: the folders at the top of the tree.
	pub fn folders(&self) -> Vec<String> {
		self.nodes
			.iter()
			.filter_map(|node| match node {
				Node::Folder { name, .. } => Some(name.clone()),
				Node::Document { .. } => None,
			})
			.collect()
	}

	/// Every document in the project, with the path it sits at, in the order a
	/// walk meets them. Derived from the tree rather than recorded beside it.
	pub fn documents(&self) -> Vec<Document> {
		tree::documents(&self.nodes)
	}
}

/// A manifest as it might be found on disk, of any version Aurora has written.
/// Version 2 recorded a list of section names and a flat list of documents;
/// version 3 records one tree. Which fields are there decides how it is read,
/// rather than the version number, because that number is a line in a file the
/// writer can edit.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
	version: u32,
	name: String,
	format: Format,
	#[serde(with = "time::serde::rfc3339")]
	created_at: OffsetDateTime,
	#[serde(default)]
	folders: Vec<String>,
	#[serde(default)]
	documents: Vec<Document>,
	nodes: Option<Vec<Node>>,
}

impl<'de> Deserialize<'de> for Manifest {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
		let stored = Stored::deserialize(deserializer)?;
		Ok(Self {
			version: stored.version,
			name: stored.name,
			format: stored.format,
			created_at: stored.created_at,
			nodes: stored
				.nodes
				.unwrap_or_else(|| tree_from_flat(&stored.folders, &stored.documents)),
		})
	}
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
	InvalidName(NameError),
	RelativePath,
	NoConfigDir,
	NotAProject,
	UnsupportedVersion { found: u32, supported: u32 },
	UnknownDocument,
	UnknownSection,
	UnknownFolder,
	FolderNotAllowed,
	SectionFixed,
	MoveInsideItself,
	DocumentMissing,
	DocumentExists,
	BadDocumentPath,
	OutsideProject,
	NotText,
	AlreadyExists,
	UnsupportedFormat(Format),
	Trash(trash::Error),
	Io(io::Error),
	Json(serde_json::Error),
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Error::InvalidName(e) => write!(f, "{e}"),
			Error::RelativePath => write!(f, "the project location must be an absolute path"),
			Error::NoConfigDir => {
				write!(f, "Aurora could not find its configuration folder")
			}
			Error::NotAProject => write!(f, "that folder is not an Aurora project"),
			Error::UnsupportedVersion { found, supported } => write!(
				f,
				"that project needs a newer version of Aurora (it was saved as version \
				 {found}, this Aurora reads version {supported})"
			),
			Error::UnknownDocument => write!(f, "that document is not part of this project"),
			Error::UnknownSection => write!(f, "that section is not part of this project"),
			Error::UnknownFolder => write!(f, "that folder is not part of this project"),
			Error::FolderNotAllowed => {
				write!(
					f,
					"a part belongs in the Manuscript and a chapter in the Manuscript or a \
					 part, and a chapter holds documents rather than folders"
				)
			}
			Error::SectionFixed => {
				write!(
					f,
					"a section is part of the project's shape and cannot be renamed or moved"
				)
			}
			Error::MoveInsideItself => {
				write!(f, "a folder cannot be moved inside itself")
			}
			Error::DocumentMissing => write!(f, "that document's file is no longer there"),
			Error::DocumentExists => {
				write!(f, "a document of that name is already there")
			}
			Error::BadDocumentPath => {
				write!(f, "that is not a path to a document in this project")
			}
			Error::OutsideProject => {
				write!(f, "that document is outside the project folder")
			}
			Error::NotText => write!(f, "that document is not text Aurora can read"),
			Error::AlreadyExists => write!(f, "a folder of that name is already there"),
			Error::UnsupportedFormat(format) => {
				write!(f, "{format:?} projects cannot be created yet")
			}
			Error::Trash(e) => write!(f, "the desktop's trash would not take the folder: {e}"),
			Error::Io(e) => write!(f, "{e}"),
			Error::Json(e) => write!(f, "{e}"),
		}
	}
}

impl Error {
	/// A name the frontend can switch on. The message beside it is only meant
	/// to be read, so it can be reworded without breaking anything.
	pub fn kind(&self) -> &'static str {
		match self {
			Error::InvalidName(_) => "invalidName",
			Error::RelativePath => "relativePath",
			Error::NoConfigDir => "noConfigDir",
			Error::NotAProject => "notAProject",
			Error::UnsupportedVersion { .. } => "unsupportedVersion",
			Error::UnknownDocument => "unknownDocument",
			Error::UnknownSection => "unknownSection",
			Error::UnknownFolder => "unknownFolder",
			Error::FolderNotAllowed => "folderNotAllowed",
			Error::SectionFixed => "sectionFixed",
			Error::MoveInsideItself => "moveInsideItself",
			Error::DocumentMissing => "documentMissing",
			Error::DocumentExists => "documentExists",
			Error::BadDocumentPath => "badDocumentPath",
			Error::OutsideProject => "outsideProject",
			Error::NotText => "notText",
			Error::AlreadyExists => "alreadyExists",
			Error::UnsupportedFormat(_) => "unsupportedFormat",
			Error::Trash(_) => "trash",
			Error::Io(_) => "io",
			Error::Json(_) => "json",
		}
	}
}

/// `#[tauri::command]` needs the error type to cross to the frontend, which
/// shows the message and occasionally has to act on the kind.
impl Serialize for Error {
	fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
		let mut error = serializer.serialize_struct("Error", 2)?;
		error.serialize_field("kind", self.kind())?;
		error.serialize_field("message", &self.to_string())?;
		error.end()
	}
}

impl std::error::Error for Error {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Error::InvalidName(_)
			| Error::RelativePath
			| Error::NoConfigDir
			| Error::NotAProject
			| Error::UnsupportedVersion { .. }
			| Error::UnknownDocument
			| Error::UnknownSection
			| Error::UnknownFolder
			| Error::FolderNotAllowed
			| Error::SectionFixed
			| Error::MoveInsideItself
			| Error::DocumentMissing
			| Error::DocumentExists
			| Error::BadDocumentPath
			| Error::OutsideProject
			| Error::NotText
			| Error::AlreadyExists
			| Error::UnsupportedFormat(_) => None,
			Error::Trash(e) => Some(e),
			Error::Io(e) => Some(e),
			Error::Json(e) => Some(e),
		}
	}
}

impl From<NameError> for Error {
	fn from(e: NameError) -> Self {
		Error::InvalidName(e)
	}
}

impl From<io::Error> for Error {
	fn from(e: io::Error) -> Self {
		Error::Io(e)
	}
}

impl From<serde_json::Error> for Error {
	fn from(e: serde_json::Error) -> Self {
		Error::Json(e)
	}
}

/// Why a name cannot be used as a folder on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
	Empty,
	TooLong,
	IllegalCharacter(char),
	EdgeWhitespaceOrDot,
	Reserved,
}

impl fmt::Display for NameError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			NameError::Empty => write!(f, "the name cannot be empty"),
			NameError::TooLong => {
				write!(f, "the name is too long (at most {MAX_NAME_BYTES} bytes)")
			}
			NameError::IllegalCharacter(c) => {
				write!(f, "the name cannot contain {c:?}")
			}
			NameError::EdgeWhitespaceOrDot => {
				write!(f, "the name cannot begin or end with a space or a dot")
			}
			NameError::Reserved => write!(f, "that name is reserved by the operating system"),
		}
	}
}

impl std::error::Error for NameError {}

/// The longest a single path component may be on the filesystems we target.
pub const MAX_NAME_BYTES: usize = 255;

/// Characters no mainstream filesystem accepts in a path component.
const ILLEGAL: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Windows refuses these as filenames whatever the extension.
const RESERVED: &[&str] = &[
	"CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
	"COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Checks a name the writer typed before it is used as a folder or a file.
/// Names are rejected rather than silently rewritten, so what is typed is what
/// ends up on disk.
pub fn validate_name(name: &str) -> std::result::Result<(), NameError> {
	if name.trim().is_empty() {
		return Err(NameError::Empty);
	}
	if name.len() > MAX_NAME_BYTES {
		return Err(NameError::TooLong);
	}

	let first = name.chars().next().unwrap();
	let last = name.chars().next_back().unwrap();
	if first.is_whitespace() || last.is_whitespace() || first == '.' || last == '.' {
		return Err(NameError::EdgeWhitespaceOrDot);
	}

	if let Some(c) = name.chars().find(|c| ILLEGAL.contains(c) || c.is_control()) {
		return Err(NameError::IllegalCharacter(c));
	}

	let stem = name.split('.').next().unwrap_or(name);
	if RESERVED.iter().any(|r| stem.eq_ignore_ascii_case(r)) {
		return Err(NameError::Reserved);
	}

	Ok(())
}

/// The manifest file at the root of every project.
pub const MANIFEST_FILE: &str = "aurora.json";

/// Creates `parent/<name>/` with the format's section folders, a seed document
/// in the ones that ask for it, and the manifest. Returns the new project's
/// root.
pub fn create(
	parent: &Path,
	name: &str,
	format: Format,
	created_at: OffsetDateTime,
) -> Result<PathBuf> {
	validate_name(name)?;

	let sections = format.layout();
	if sections.is_empty() {
		return Err(Error::UnsupportedFormat(format));
	}

	let root = parent.join(name);
	match fs::create_dir(&root) {
		Ok(()) => {}
		Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Err(Error::AlreadyExists),
		Err(e) => return Err(e.into()),
	}

	// The folder did not exist a moment ago, so undoing our own work is safe.
	match fill(&root, name, format, sections, created_at) {
		Ok(()) => Ok(root),
		Err(e) => {
			let _ = fs::remove_dir_all(&root);
			Err(e)
		}
	}
}

/// Pretty-prints with tabs, to match every other configuration file Aurora owns.
pub(crate) fn to_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
	let formatter = serde_json::ser::PrettyFormatter::with_indent(b"\t");
	let mut out = Vec::new();
	let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
	value.serialize(&mut serializer)?;
	out.push(b'\n');
	Ok(out)
}

/// Writes through a temporary file and a rename, so an interrupted save leaves
/// the previous version intact rather than a truncated one. The temporary file
/// replaces the extension rather than adding one, which keeps it out of `scan`.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
	let temp = path.with_extension("tmp");
	fs::write(&temp, bytes)?;
	fs::rename(&temp, path)?;
	Ok(())
}

pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
	write_atomic(path, &to_json(value)?)
}

fn fill(
	root: &Path,
	name: &str,
	format: Format,
	sections: &[Section],
	created_at: OffsetDateTime,
) -> Result<()> {
	let mut nodes = Vec::new();
	for section in sections {
		let folder = root.join(section.folder);
		fs::create_dir(&folder)?;
		let mut children = Vec::new();
		if let Some(seed) = section.seed {
			fs::write(folder.join(seed), "")?;
			children.push(Node::document(seed));
		}
		nodes.push(Node::folder(section.folder, children));
	}

	let mut manifest = Manifest::new(name, format, created_at);
	manifest.nodes = nodes;
	fs::write(root.join(MANIFEST_FILE), to_json(&manifest)?)?;
	Ok(())
}

/// What the frontend should show on launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LastProject {
	None,
	Open { name: String, root: PathBuf },
	Missing { name: String, root: PathBuf },
}

pub(crate) fn store_path(app: &AppHandle) -> Result<PathBuf> {
	app.path()
		.app_config_dir()
		.map(|dir| dir.join("store.json"))
		.map_err(|_| Error::NoConfigDir)
}

fn create_and_remember(
	store_path: &Path,
	parent: &Path,
	name: &str,
	format: Format,
	at: OffsetDateTime,
) -> Result<PathBuf> {
	// The path arrives from the frontend, so it is not trusted to be sensible.
	if !parent.is_absolute() {
		return Err(Error::RelativePath);
	}

	let root = create(parent, name, format, at)?;
	let mut store = store::load(store_path)?;
	store.remember(name, root.clone(), at);
	store::save(store_path, &store)?;
	Ok(root)
}

/// Reports what to reopen without changing anything, so asking twice gives the
/// same answer. Dropping a missing project is `forget` below.
fn resolve_last(store_path: &Path) -> Result<LastProject> {
	let store = store::load(store_path)?;
	let Some(last) = store.reopen() else {
		return Ok(LastProject::None);
	};

	let name = last.name.clone();
	let root = last.root.clone();
	if root.join(MANIFEST_FILE).is_file() {
		Ok(LastProject::Open { name, root })
	} else {
		Ok(LastProject::Missing { name, root })
	}
}

fn forget(store_path: &Path, root: &Path) -> Result<()> {
	let mut store = store::load(store_path)?;
	store.forget(root);
	store::save(store_path, &store)
}

fn close(store_path: &Path) -> Result<()> {
	let mut store = store::load(store_path)?;
	store.close();
	store::save(store_path, &store)
}

#[tauri::command]
pub fn create_project(
	app: AppHandle,
	parent: PathBuf,
	name: String,
	format: Format,
) -> Result<PathBuf> {
	create_and_remember(
		&store_path(&app)?,
		&parent,
		&name,
		format,
		OffsetDateTime::now_utc(),
	)
}

/// A project as the frontend refers to it once it is open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedProject {
	pub name: String,
	pub root: PathBuf,
}

pub fn read_manifest(root: &Path) -> Result<Manifest> {
	let bytes = fs::read(root.join(MANIFEST_FILE)).map_err(|e| match e.kind() {
		io::ErrorKind::NotFound => Error::NotAProject,
		_ => Error::Io(e),
	})?;
	let manifest: Manifest = serde_json::from_slice(&bytes)?;
	// An older Aurora cannot know what a newer one added, so refuse rather than
	// silently dropping fields and writing them away on the next save.
	if manifest.version > MANIFEST_VERSION {
		return Err(Error::UnsupportedVersion {
			found: manifest.version,
			supported: MANIFEST_VERSION,
		});
	}
	Ok(manifest)
}

/// Writes the manifest back, at the current version whatever version it was
/// read as: what goes to disk from here is always a tree.
pub(crate) fn write_manifest(root: &Path, manifest: &mut Manifest) -> Result<()> {
	manifest.version = MANIFEST_VERSION;
	write_json(&root.join(MANIFEST_FILE), manifest)
}

fn open_and_remember(store_path: &Path, root: &Path, at: OffsetDateTime) -> Result<OpenedProject> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	// The manifest holds the name, not the folder, so a renamed folder still
	// opens under the name the writer gave it.
	let manifest = refresh(root)?;
	let mut store = store::load(store_path)?;
	store.remember(manifest.name.clone(), root.to_path_buf(), at);
	store::save(store_path, &store)?;

	Ok(OpenedProject {
		name: manifest.name,
		root: root.to_path_buf(),
	})
}

/// Only projects that are still on disk, so the list never offers a button that
/// cannot work.
fn available_recents(store_path: &Path) -> Result<Vec<store::RecentProject>> {
	let store = store::load(store_path)?;
	Ok(store
		.recent
		.into_iter()
		.filter(|project| project.root.join(MANIFEST_FILE).is_file())
		.collect())
}

/// A remembered project as the welcome screen shows it: what the store holds,
/// with the size of the writing counted from the files themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentSummary {
	#[serde(flatten)]
	pub project: store::RecentProject,
	/// None when the manifest could not be read, so the screen can leave the
	/// count off rather than report a project as empty.
	pub words: Option<usize>,
	/// None for the same reason. The question asked before a project is
	/// trashed counts what is about to go, so it needs this to be honest.
	pub documents: Option<usize>,
}

/// Every word in a project. Nothing caches this, so it opens each document
/// once and keeps only the count.
/// What the welcome screen says about a project. Both numbers come out of one
/// read of the manifest, since nothing ever wants only one of them.
struct Measure {
	words: usize,
	documents: usize,
}

fn measure(root: &Path) -> Option<Measure> {
	let manifest = read_manifest(root).ok()?;
	let documents = manifest.documents();
	Some(Measure {
		words: documents
			.iter()
			.map(|document| {
				// The manifest is a file in the writer's project and could
				// have been edited by hand, so a path that climbs out of the
				// project is passed over rather than read.
				let relative = Path::new(&document.path);
				if !relative
					.components()
					.all(|part| matches!(part, Component::Normal(_)))
				{
					return 0;
				}

				fs::read_to_string(root.join(relative))
					.map(|text| text.split_whitespace().count())
					.unwrap_or(0)
			})
			.sum(),
		documents: documents.len(),
	})
}

fn summarise_recents(store_path: &Path) -> Result<Vec<RecentSummary>> {
	Ok(available_recents(store_path)?
		.into_iter()
		.map(|project| {
			let measured = measure(&project.root);
			RecentSummary {
				words: measured.as_ref().map(|m| m.words),
				documents: measured.as_ref().map(|m| m.documents),
				project,
			}
		})
		.collect())
}

#[tauri::command]
pub fn open_project(app: AppHandle, root: PathBuf) -> Result<OpenedProject> {
	open_and_remember(&store_path(&app)?, &root, OffsetDateTime::now_utc())
}

#[tauri::command]
pub fn recent_projects(app: AppHandle) -> Result<Vec<RecentSummary>> {
	summarise_recents(&store_path(&app)?)
}

#[tauri::command]
pub fn last_project(app: AppHandle) -> Result<LastProject> {
	resolve_last(&store_path(&app)?)
}

#[tauri::command]
pub fn forget_project(app: AppHandle, root: PathBuf) -> Result<()> {
	forget(&store_path(&app)?, &root)
}

/// Moves the whole project folder to the desktop's trash and takes it off the
/// recent list. Nothing here unlinks anything: the folder is recoverable from
/// the trash until the writer empties it.
#[tauri::command]
pub fn trash_project(app: AppHandle, root: PathBuf) -> Result<()> {
	let store = store_path(&app)?;
	// Only a folder Aurora can read as a project. A path that arrives from
	// anywhere else must not be able to bin a folder on the strength of being
	// asked to.
	read_manifest(&root)?;
	trash::delete(&root).map_err(Error::Trash)?;
	forget(&store, &root)
}

/// The writer is done with the open project for now. It stays in the recent
/// list, but Aurora starts on the welcome screen next time.
#[tauri::command]
pub fn close_project(app: AppHandle) -> Result<()> {
	close(&store_path(&app)?)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashSet;
	use uuid::Uuid;

	fn paths_of(manifest: &Manifest) -> Vec<String> {
		manifest
			.documents()
			.into_iter()
			.map(|document| document.path)
			.collect()
	}

	#[test]
	fn format_names_match_the_frontend() {
		assert_eq!(
			serde_json::to_string(&Format::ShortStories).unwrap(),
			"\"short-stories\""
		);
		assert_eq!(
			serde_json::from_str::<Format>("\"stage-play\"").unwrap(),
			Format::StagePlay
		);
	}

	#[test]
	fn unknown_format_is_rejected() {
		assert!(serde_json::from_str::<Format>("\"poetry\"").is_err());
	}

	#[test]
	fn novel_has_the_five_sections() {
		let folders: Vec<_> = Format::Novel.layout().iter().map(|s| s.folder).collect();
		assert_eq!(
			folders,
			["Manuscript", "Outline", "Characters", "Locations", "Notes"]
		);
	}

	#[test]
	fn a_section_that_seeds_seeds_a_markdown_file() {
		for section in Format::Novel.layout() {
			if let Some(seed) = section.seed {
				assert!(seed.ends_with(".md"), "{} seeds {}", section.folder, seed);
			}
		}
	}

	#[test]
	fn the_subject_sections_start_empty() {
		let empty: Vec<_> = Format::Novel
			.layout()
			.iter()
			.filter(|s| s.seed.is_none())
			.map(|s| s.folder)
			.collect();
		assert_eq!(empty, ["Characters", "Locations"]);
	}

	#[test]
	fn the_other_formats_have_no_layout_yet() {
		for format in [Format::Screenplay, Format::ShortStories, Format::StagePlay] {
			assert!(
				format.layout().is_empty(),
				"{format:?} should not be creatable"
			);
		}
	}

	#[test]
	fn every_format_is_offered_in_order() {
		let formats: Vec<_> = format_layouts().into_iter().map(|f| f.format).collect();
		assert_eq!(formats, Format::ALL);
	}

	#[test]
	fn the_layouts_match_the_formats() {
		let layouts = format_layouts();
		let novel = layouts.iter().find(|f| f.format == Format::Novel).unwrap();
		assert!(novel.available);
		assert_eq!(
			novel.folders,
			["Manuscript", "Outline", "Characters", "Locations", "Notes"]
		);

		for layout in layouts.iter().filter(|f| f.format != Format::Novel) {
			assert!(layout.folders.is_empty());
			assert!(!layout.available);
		}
	}

	#[test]
	fn a_layout_crosses_with_the_format_name_the_frontend_uses() {
		let layouts = format_layouts();
		let stage_play = layouts.last().unwrap();
		assert_eq!(
			serde_json::to_value(stage_play).unwrap(),
			serde_json::json!({
				"format": "stage-play",
				"folders": [],
				"available": false,
			})
		);
	}

	fn fixed_time() -> OffsetDateTime {
		OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
	}

	#[test]
	fn manifest_takes_its_folders_from_the_layout() {
		let manifest = Manifest::new("Wuthering Heights", Format::Novel, fixed_time());
		assert_eq!(manifest.version, MANIFEST_VERSION);
		assert_eq!(
			manifest.folders(),
			["Manuscript", "Outline", "Characters", "Locations", "Notes"]
		);
	}

	#[test]
	fn manifest_drops_sub_second_precision() {
		let precise = OffsetDateTime::from_unix_timestamp_nanos(1_700_000_000_297_374_168).unwrap();
		let manifest = Manifest::new("Ithaca", Format::Novel, precise);
		assert_eq!(manifest.created_at, fixed_time());
	}

	#[test]
	fn manifest_serializes_with_camel_case_keys() {
		let manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());
		let json = serde_json::to_value(&manifest).unwrap();
		assert_eq!(json["version"], MANIFEST_VERSION);
		assert_eq!(json["name"], "Ithaca");
		assert_eq!(json["format"], "novel");
		assert_eq!(json["createdAt"], "2023-11-14T22:13:20Z");
		assert!(json.get("created_at").is_none());
	}

	#[test]
	fn manifest_round_trips() {
		let manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());
		let json = serde_json::to_string(&manifest).unwrap();
		assert_eq!(serde_json::from_str::<Manifest>(&json).unwrap(), manifest);
	}

	#[test]
	fn a_manifest_missing_a_field_is_rejected() {
		let json = r#"{"version":1,"name":"Ithaca","format":"novel","folders":[]}"#;
		assert!(serde_json::from_str::<Manifest>(json).is_err());
	}

	#[test]
	fn ordinary_names_are_accepted() {
		for name in [
			"Ithaca",
			"Book One — Draft 2",
			"Les Misérables",
			"夜明け前",
			"Chapter 1 (rewrite)",
			"a",
		] {
			assert_eq!(validate_name(name), Ok(()), "{name} should be accepted");
		}
	}

	#[test]
	fn blank_names_are_rejected() {
		assert_eq!(validate_name(""), Err(NameError::Empty));
		assert_eq!(validate_name("   "), Err(NameError::Empty));
		assert_eq!(validate_name("\t\n"), Err(NameError::Empty));
	}

	#[test]
	fn names_that_break_windows_are_rejected() {
		assert_eq!(
			validate_name("Ithaca: Book One"),
			Err(NameError::IllegalCharacter(':'))
		);
		assert_eq!(
			validate_name("What Now?"),
			Err(NameError::IllegalCharacter('?'))
		);
		assert_eq!(
			validate_name("\"Quoted\""),
			Err(NameError::IllegalCharacter('"'))
		);
	}

	#[test]
	fn path_separators_and_traversal_are_rejected() {
		assert_eq!(
			validate_name("drafts/old"),
			Err(NameError::IllegalCharacter('/'))
		);
		assert_eq!(
			validate_name("drafts\\old"),
			Err(NameError::IllegalCharacter('\\'))
		);
		assert_eq!(validate_name("."), Err(NameError::EdgeWhitespaceOrDot));
		assert_eq!(validate_name(".."), Err(NameError::EdgeWhitespaceOrDot));
		assert_eq!(
			validate_name(".hidden"),
			Err(NameError::EdgeWhitespaceOrDot)
		);
	}

	#[test]
	fn edge_whitespace_and_dots_are_rejected() {
		assert_eq!(
			validate_name("Ithaca "),
			Err(NameError::EdgeWhitespaceOrDot)
		);
		assert_eq!(
			validate_name(" Ithaca"),
			Err(NameError::EdgeWhitespaceOrDot)
		);
		assert_eq!(
			validate_name("Ithaca."),
			Err(NameError::EdgeWhitespaceOrDot)
		);
	}

	#[test]
	fn control_characters_are_rejected() {
		assert_eq!(
			validate_name("Itha\u{0}ca"),
			Err(NameError::IllegalCharacter('\u{0}'))
		);
		assert_eq!(
			validate_name("Itha\nca"),
			Err(NameError::IllegalCharacter('\n'))
		);
	}

	#[test]
	fn reserved_device_names_are_rejected() {
		for name in ["CON", "con", "NUL", "Com1", "LPT9", "CON.md"] {
			assert_eq!(
				validate_name(name),
				Err(NameError::Reserved),
				"{name} should be reserved"
			);
		}
		assert_eq!(validate_name("Console"), Ok(()));
	}

	#[test]
	fn over_long_names_are_rejected() {
		assert_eq!(validate_name(&"a".repeat(MAX_NAME_BYTES)), Ok(()));
		assert_eq!(
			validate_name(&"a".repeat(MAX_NAME_BYTES + 1)),
			Err(NameError::TooLong)
		);
	}

	#[test]
	fn errors_carry_their_cause() {
		let io: Error = std::io::Error::from(std::io::ErrorKind::PermissionDenied).into();
		assert!(std::error::Error::source(&io).is_some());

		let name: Error = NameError::Reserved.into();
		assert!(std::error::Error::source(&name).is_none());
		assert_eq!(name.to_string(), NameError::Reserved.to_string());
	}

	#[test]
	fn create_lays_out_a_novel() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		assert_eq!(root, parent.path().join("Ithaca"));
		for section in Format::Novel.layout() {
			let folder = root.join(section.folder);
			assert!(folder.is_dir(), "{} is missing", section.folder);
			match section.seed {
				Some(name) => {
					let seed = folder.join(name);
					assert!(seed.is_file(), "{name} is missing");
					assert_eq!(fs::read_to_string(&seed).unwrap(), "");
				}
				None => assert_eq!(
					fs::read_dir(&folder).unwrap().count(),
					0,
					"{} should start empty",
					section.folder
				),
			}
		}

		let json = fs::read_to_string(root.join(MANIFEST_FILE)).unwrap();
		let manifest: Manifest = serde_json::from_str(&json).unwrap();
		// Every node carries a random id, so the tree is checked on its own.
		let fresh = Manifest::new("Ithaca", Format::Novel, fixed_time());
		assert_eq!(
			Manifest {
				nodes: Vec::new(),
				..manifest
			},
			Manifest {
				nodes: Vec::new(),
				..fresh
			}
		);
	}

	#[test]
	fn the_manifest_is_written_for_a_human_to_read() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let json = fs::read_to_string(root.join(MANIFEST_FILE)).unwrap();
		assert!(
			json.contains(&format!("\n\t\"version\": {MANIFEST_VERSION}")),
			"expected tab indentation"
		);
		assert!(json.contains("\"createdAt\": \"2023-11-14T22:13:20Z\""));
		assert!(json.ends_with("\n"), "expected a trailing newline");
	}

	#[test]
	fn a_new_project_records_its_seed_documents() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manifest = read_manifest(&root).unwrap();

		let documents = manifest.documents();
		let paths: Vec<_> = documents.iter().map(|d| d.path.as_str()).collect();
		assert_eq!(
			paths,
			[
				"Manuscript/Scene 1.md",
				"Outline/Outline.md",
				"Notes/Notes.md"
			]
		);

		let ids: HashSet<_> = documents.iter().map(|d| d.id).collect();
		assert_eq!(ids.len(), documents.len(), "ids must be unique");
	}

	#[test]
	fn every_recorded_document_is_on_disk() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		for document in read_manifest(&root).unwrap().documents() {
			assert!(
				root.join(&document.path).is_file(),
				"{} is recorded but missing",
				document.path
			);
		}
	}

	#[test]
	fn a_manifest_from_before_documents_existed_still_loads() {
		let dir = tempfile::tempdir().unwrap();
		fs::write(
			dir.path().join(MANIFEST_FILE),
			r#"{"version":1,"name":"Ithaca","format":"novel",
			   "createdAt":"2023-11-14T22:13:20Z","folders":["Manuscript"]}"#,
		)
		.unwrap();

		let manifest = read_manifest(dir.path()).unwrap();
		assert_eq!(manifest.version, 1);
		assert!(manifest.documents().is_empty());
	}

	/// A project written by the Aurora before this one: section names and a flat
	/// list of documents, with no tree in sight.
	fn version_two_project(dir: &Path, id: Uuid) {
		fs::write(
			dir.join(MANIFEST_FILE),
			format!(
				r#"{{"version":2,"name":"Ithaca","format":"novel",
				   "createdAt":"2023-11-14T22:13:20Z",
				   "folders":["Manuscript","Notes"],
				   "documents":[
				     {{"id":"{id}","path":"Manuscript/Scene 1.md","target":1200}},
				     {{"id":"{}","path":"Notes/Notes.md"}}
				   ]}}"#,
				Uuid::new_v4()
			),
		)
		.unwrap();
	}

	#[test]
	fn a_version_two_manifest_opens_as_a_tree() {
		let dir = tempfile::tempdir().unwrap();
		let chapter_one = Uuid::new_v4();
		version_two_project(dir.path(), chapter_one);

		let manifest = read_manifest(dir.path()).unwrap();

		assert_eq!(manifest.folders(), ["Manuscript", "Notes"]);
		let documents = manifest.documents();
		assert_eq!(
			paths_of(&manifest),
			["Manuscript/Scene 1.md", "Notes/Notes.md"]
		);
		assert_eq!(
			documents[0].id, chapter_one,
			"a document keeps the id the project already gave it"
		);
		assert_eq!(documents[0].target, Some(1200), "and what it is aiming at");
	}

	#[test]
	fn reading_an_old_manifest_does_not_rewrite_it() {
		let dir = tempfile::tempdir().unwrap();
		version_two_project(dir.path(), Uuid::new_v4());

		read_manifest(dir.path()).unwrap();

		let json = fs::read_to_string(dir.path().join(MANIFEST_FILE)).unwrap();
		assert!(
			json.contains("\"documents\"") && !json.contains("\"nodes\""),
			"the file is left as it was until something changes it"
		);
	}

	#[test]
	fn writing_an_old_manifest_back_makes_it_a_tree() {
		let dir = tempfile::tempdir().unwrap();
		version_two_project(dir.path(), Uuid::new_v4());
		let mut manifest = read_manifest(dir.path()).unwrap();

		write_manifest(dir.path(), &mut manifest).unwrap();

		let json = fs::read_to_string(dir.path().join(MANIFEST_FILE)).unwrap();
		assert!(json.contains("\"nodes\""));
		assert!(
			!json.contains("\"documents\"") && !json.contains("\"folders\""),
			"what version 2 recorded is gone: {json}"
		);
		assert_eq!(
			read_manifest(dir.path()).unwrap().version,
			MANIFEST_VERSION,
			"and it says which version it now is"
		);
	}

	#[test]
	fn create_refuses_a_bad_name_without_touching_the_disk() {
		let parent = tempfile::tempdir().unwrap();
		let err = create(
			parent.path(),
			"Ithaca: Book One",
			Format::Novel,
			fixed_time(),
		)
		.unwrap_err();
		assert!(matches!(
			err,
			Error::InvalidName(NameError::IllegalCharacter(':'))
		));
		assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
	}

	#[test]
	fn create_refuses_a_format_without_a_layout() {
		let parent = tempfile::tempdir().unwrap();
		let err = create(parent.path(), "Ithaca", Format::Screenplay, fixed_time()).unwrap_err();
		assert!(matches!(err, Error::UnsupportedFormat(Format::Screenplay)));
		assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
	}

	#[test]
	fn create_will_not_overwrite_an_existing_folder() {
		let parent = tempfile::tempdir().unwrap();
		create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap_err();
		assert!(matches!(err, Error::AlreadyExists));
		assert!(parent.path().join("Ithaca").join(MANIFEST_FILE).is_file());
	}

	#[test]
	fn create_reports_a_missing_parent() {
		let parent = tempfile::tempdir().unwrap();
		let missing = parent.path().join("nowhere");
		let err = create(&missing, "Ithaca", Format::Novel, fixed_time()).unwrap_err();
		assert!(matches!(err, Error::Io(_)));
	}

	#[test]
	fn a_blocked_section_folder_is_an_error() {
		let parent = tempfile::tempdir().unwrap();
		let root = parent.path().join("Ithaca");
		fs::create_dir(&root).unwrap();
		// A file where the first section folder needs to go.
		fs::write(root.join("Manuscript"), "").unwrap();

		assert!(
			fill(
				&root,
				"Ithaca",
				Format::Novel,
				Format::Novel.layout(),
				fixed_time()
			)
			.is_err()
		);
	}

	#[test]
	fn creating_records_the_project_in_the_store() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("config").join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		assert!(root.join(MANIFEST_FILE).is_file());
		assert!(root.join("Manuscript").join("Scene 1.md").is_file());

		let remembered = store::load(&store).unwrap();
		assert_eq!(remembered.reopen().unwrap().name, "Ithaca");
		assert_eq!(remembered.reopen().unwrap().root, root);
	}

	#[test]
	fn a_failed_creation_is_not_remembered() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		assert!(
			create_and_remember(
				&store,
				parent.path(),
				"Ithaca: Book One",
				Format::Novel,
				fixed_time()
			)
			.is_err()
		);
		assert!(store::load(&store).unwrap().reopen().is_none());
	}

	#[test]
	fn opening_adopts_a_file_added_outside_aurora() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		fs::write(root.join("Manuscript").join("Chapter 2.md"), "").unwrap();

		open_and_remember(&store, &root, fixed_time()).unwrap();

		let manifest = read_manifest(&root).unwrap();
		assert!(
			paths_of(&manifest)
				.iter()
				.any(|p| p == "Manuscript/Chapter 2.md")
		);
	}

	#[test]
	fn opening_drops_a_file_deleted_outside_aurora() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		fs::remove_file(root.join("Notes").join("Notes.md")).unwrap();

		open_and_remember(&store, &root, fixed_time()).unwrap();

		let manifest = read_manifest(&root).unwrap();
		assert!(!paths_of(&manifest).iter().any(|p| p == "Notes/Notes.md"));
	}

	#[test]
	fn opening_an_unchanged_project_leaves_the_manifest_alone() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		// Rewritten compactly, so any rewrite would restore the pretty form.
		let manifest = read_manifest(&root).unwrap();
		let compact = serde_json::to_vec(&manifest).unwrap();
		fs::write(root.join(MANIFEST_FILE), &compact).unwrap();

		open_and_remember(&store, &root, fixed_time()).unwrap();

		assert_eq!(fs::read(root.join(MANIFEST_FILE)).unwrap(), compact);
	}

	#[test]
	fn the_command_refuses_a_relative_path() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let err = create_and_remember(
			&store,
			Path::new("some/where"),
			"Ithaca",
			Format::Novel,
			fixed_time(),
		)
		.unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn a_closed_project_does_not_reopen_but_is_still_offered() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		close(&store).unwrap();

		assert_eq!(resolve_last(&store).unwrap(), LastProject::None);
		assert_eq!(available_recents(&store).unwrap().len(), 1);
	}

	#[test]
	fn nothing_remembered_means_nothing_to_reopen() {
		let dir = tempfile::tempdir().unwrap();
		let store = dir.path().join("store.json");
		assert_eq!(resolve_last(&store).unwrap(), LastProject::None);
	}

	#[test]
	fn the_last_project_is_reopened() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		assert_eq!(
			resolve_last(&store).unwrap(),
			LastProject::Open {
				name: "Ithaca".to_owned(),
				root,
			}
		);
	}

	#[test]
	fn asking_twice_gives_the_same_answer() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		fs::remove_dir_all(&root).unwrap();

		let missing = LastProject::Missing {
			name: "Ithaca".to_owned(),
			root: root.clone(),
		};
		assert_eq!(resolve_last(&store).unwrap(), missing);
		assert_eq!(resolve_last(&store).unwrap(), missing);

		forget(&store, &root).unwrap();
		assert_eq!(resolve_last(&store).unwrap(), LastProject::None);
	}

	#[test]
	fn forgetting_twice_is_harmless() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		forget(&store, &root).unwrap();
		forget(&store, &root).unwrap();
		assert_eq!(resolve_last(&store).unwrap(), LastProject::None);
	}

	#[test]
	fn a_folder_without_a_manifest_counts_as_missing() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		fs::remove_file(root.join(MANIFEST_FILE)).unwrap();

		assert!(matches!(
			resolve_last(&store).unwrap(),
			LastProject::Missing { .. }
		));
	}

	#[test]
	fn the_last_project_serializes_as_a_tagged_object() {
		assert_eq!(
			serde_json::to_value(LastProject::None).unwrap(),
			serde_json::json!({ "kind": "none" })
		);
		assert_eq!(
			serde_json::to_value(LastProject::Missing {
				name: "Ithaca".to_owned(),
				root: PathBuf::from("/writing/Ithaca"),
			})
			.unwrap(),
			serde_json::json!({
				"kind": "missing",
				"name": "Ithaca",
				"root": "/writing/Ithaca",
			})
		);
	}

	#[test]
	fn an_existing_project_can_be_reopened() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		assert_eq!(
			open_and_remember(&store, &root, fixed_time()).unwrap(),
			OpenedProject {
				name: "Ithaca".to_owned(),
				root: root.clone(),
			}
		);
	}

	#[test]
	fn opening_uses_the_name_in_the_manifest() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		let renamed = parent.path().join("moved-elsewhere");
		fs::rename(&root, &renamed).unwrap();

		let opened = open_and_remember(&store, &renamed, fixed_time()).unwrap();
		assert_eq!(opened.name, "Ithaca");
		assert_eq!(opened.root, renamed);
	}

	#[test]
	fn opening_moves_the_project_to_the_front() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let first =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		create_and_remember(
			&store,
			parent.path(),
			"Penelope",
			Format::Novel,
			fixed_time(),
		)
		.unwrap();

		open_and_remember(&store, &first, fixed_time()).unwrap();
		assert_eq!(
			store::load(&store).unwrap().reopen().unwrap().name,
			"Ithaca"
		);
	}

	#[test]
	fn an_ordinary_folder_is_not_a_project() {
		let dir = tempfile::tempdir().unwrap();
		let store = dir.path().join("store.json");
		let err = open_and_remember(&store, dir.path(), fixed_time()).unwrap_err();
		assert!(matches!(err, Error::NotAProject));
	}

	#[test]
	fn opening_refuses_a_relative_path() {
		let dir = tempfile::tempdir().unwrap();
		let store = dir.path().join("store.json");
		let err = open_and_remember(&store, Path::new("some/where"), fixed_time()).unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	/// Rewrites the manifest's version field, leaving the rest of the file alone.
	fn set_manifest_version(root: &Path, version: u32) {
		let path = root.join(MANIFEST_FILE);
		let mut value: serde_json::Value =
			serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
		value["version"] = version.into();
		fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
	}

	#[test]
	fn a_manifest_from_a_newer_aurora_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		set_manifest_version(&root, MANIFEST_VERSION + 1);

		let err = read_manifest(&root).unwrap_err();
		assert!(matches!(
			err,
			Error::UnsupportedVersion { found, supported }
				if found == MANIFEST_VERSION + 1 && supported == MANIFEST_VERSION
		));
	}

	#[test]
	fn an_error_crosses_as_a_kind_and_a_message() {
		let json = serde_json::to_value(Error::DocumentMissing).unwrap();
		assert_eq!(json["kind"], "documentMissing");
		assert_eq!(json["message"], "that document's file is no longer there");

		let json = serde_json::to_value(Error::InvalidName(NameError::Empty)).unwrap();
		assert_eq!(json["kind"], "invalidName");
		assert_eq!(json["message"], NameError::Empty.to_string());
	}

	/// The frontend switches on these, so no two variants may answer to the
	/// same name and none may be left out.
	#[test]
	fn every_error_has_its_own_kind() {
		let kinds = [
			Error::InvalidName(NameError::Empty),
			Error::RelativePath,
			Error::NoConfigDir,
			Error::NotAProject,
			Error::UnsupportedVersion {
				found: 3,
				supported: 2,
			},
			Error::UnknownDocument,
			Error::UnknownSection,
			Error::UnknownFolder,
			Error::FolderNotAllowed,
			Error::SectionFixed,
			Error::MoveInsideItself,
			Error::DocumentMissing,
			Error::DocumentExists,
			Error::BadDocumentPath,
			Error::OutsideProject,
			Error::NotText,
			Error::AlreadyExists,
			Error::UnsupportedFormat(Format::Screenplay),
			Error::Io(io::Error::from(io::ErrorKind::PermissionDenied)),
			Error::Json(serde_json::from_str::<Manifest>("{").unwrap_err()),
		]
		.map(|error| error.kind());

		let unique: std::collections::HashSet<_> = kinds.iter().collect();
		assert_eq!(unique.len(), kinds.len());
	}

	/// The message reaches the writer verbatim, so it has to read as a sentence.
	#[test]
	fn error_messages_are_single_spaced() {
		for error in [
			Error::UnsupportedVersion {
				found: 3,
				supported: 2,
			},
			Error::InvalidName(NameError::Empty),
			Error::RelativePath,
			Error::NoConfigDir,
			Error::NotAProject,
			Error::UnknownDocument,
			Error::UnknownSection,
			Error::UnknownFolder,
			Error::FolderNotAllowed,
			Error::SectionFixed,
			Error::MoveInsideItself,
			Error::DocumentMissing,
			Error::DocumentExists,
			Error::BadDocumentPath,
			Error::OutsideProject,
			Error::NotText,
			Error::AlreadyExists,
			Error::UnsupportedFormat(Format::Screenplay),
		] {
			let message = error.to_string();
			assert!(
				!message.contains('\t') && !message.contains('\n') && !message.contains("  "),
				"{message:?}"
			);
		}
	}

	#[test]
	fn a_project_we_cannot_read_is_not_remembered() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		set_manifest_version(&root, MANIFEST_VERSION + 1);
		forget(&store, &root).unwrap();

		let err = open_and_remember(&store, &root, fixed_time()).unwrap_err();
		assert!(matches!(err, Error::UnsupportedVersion { .. }));
		assert!(available_recents(&store).unwrap().is_empty());
	}

	#[test]
	fn an_older_manifest_still_opens() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		set_manifest_version(&root, MANIFEST_VERSION - 1);

		assert_eq!(read_manifest(&root).unwrap().version, MANIFEST_VERSION - 1);
	}

	#[test]
	fn the_recent_list_hides_projects_that_have_gone() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let gone =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		create_and_remember(
			&store,
			parent.path(),
			"Penelope",
			Format::Novel,
			fixed_time(),
		)
		.unwrap();
		fs::remove_dir_all(&gone).unwrap();

		let recents = available_recents(&store).unwrap();
		assert_eq!(recents.len(), 1);
		assert_eq!(recents[0].name, "Penelope");
	}

	#[test]
	fn the_recent_list_counts_every_document_in_a_project() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		let manuscript = root.join("Manuscript");
		fs::write(manuscript.join("Scene 1.md"), "one two three").unwrap();
		fs::write(manuscript.join("Chapter 2.md"), "four five").unwrap();
		refresh(&root).unwrap();

		let recents = summarise_recents(&store).unwrap();
		assert_eq!(recents.len(), 1);
		assert_eq!(recents[0].words, Some(5));
	}

	#[test]
	fn a_document_pointing_out_of_the_project_is_not_counted() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		fs::write(parent.path().join("elsewhere.md"), "one two three").unwrap();
		// A path that climbs out of the project can only be written by hand,
		// and only at the top of the tree: a name under a folder is one part.
		let mut manifest = read_manifest(&root).unwrap();
		manifest.nodes.insert(0, Node::document("../elsewhere.md"));
		write_manifest(&root, &mut manifest).unwrap();

		assert_eq!(summarise_recents(&store).unwrap()[0].words, Some(0));
	}
}
