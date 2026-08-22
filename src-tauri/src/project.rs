use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, Serializer};
use tauri::{AppHandle, Manager};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store;

/// A folder inside a project, together with the document it starts life with.
pub struct Section {
	pub folder: &'static str,
	pub seed: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
	Novel,
	Screenplay,
	ShortStories,
	StagePlay,
}

const NOVEL: &[Section] = &[
	Section {
		folder: "Manuscript",
		seed: "Chapter 1.md",
	},
	Section {
		folder: "Outline",
		seed: "Outline.md",
	},
	Section {
		folder: "Characters",
		seed: "Characters.md",
	},
	Section {
		folder: "Locations",
		seed: "Locations.md",
	},
	Section {
		folder: "Notes",
		seed: "Notes.md",
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
pub const MANIFEST_VERSION: u32 = 2;

/// A document inside a project. The path is relative to the project root and
/// always uses forward slashes, so a manifest written on one platform still
/// resolves on another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
	pub id: Uuid,
	pub path: String,
}

impl Document {
	fn new(section: &str, file_name: &str) -> Self {
		Self {
			id: Uuid::new_v4(),
			path: format!("{section}/{file_name}"),
		}
	}

	/// The file name without its extension. The file name is the title.
	fn title(&self) -> String {
		Path::new(&self.path)
			.file_stem()
			.and_then(|stem| stem.to_str())
			.unwrap_or(&self.path)
			.to_owned()
	}
}

/// A document as the sidebar shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentView {
	pub id: Uuid,
	pub path: String,
	pub title: String,
}

impl From<&Document> for DocumentView {
	fn from(document: &Document) -> Self {
		Self {
			id: document.id,
			path: document.path.clone(),
			title: document.title(),
		}
	}
}

/// One of the project's sections and the documents in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SectionDocuments {
	pub folder: String,
	pub documents: Vec<DocumentView>,
}

/// The contents of `aurora.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
	pub version: u32,
	pub name: String,
	pub format: Format,
	#[serde(with = "time::serde::rfc3339")]
	pub created_at: OffsetDateTime,
	/// The section folders as they were at creation, so a project keeps its own
	/// layout even if the format's definition changes later.
	pub folders: Vec<String>,
	/// Every document Aurora knows about, in the order it shows them. Absent
	/// from manifests written before version 2.
	#[serde(default)]
	pub documents: Vec<Document>,
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
			folders: format
				.layout()
				.iter()
				.map(|s| s.folder.to_owned())
				.collect(),
			// Ids are random, so the seed documents are attached by `fill`,
			// leaving this deterministic.
			documents: Vec::new(),
		}
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
	DocumentMissing,
	OutsideProject,
	NotText,
	AlreadyExists,
	UnsupportedFormat(Format),
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
				"that project needs a newer version of Aurora 				 (it was saved as version {found}, this Aurora reads version {supported})"
			),
			Error::UnknownDocument => write!(f, "that document is not part of this project"),
			Error::DocumentMissing => write!(f, "that document's file is no longer there"),
			Error::OutsideProject => {
				write!(f, "that document is outside the project folder")
			}
			Error::NotText => write!(f, "that document is not text Aurora can read"),
			Error::AlreadyExists => write!(f, "a folder of that name is already there"),
			Error::UnsupportedFormat(format) => {
				write!(f, "{format:?} projects cannot be created yet")
			}
			Error::Io(e) => write!(f, "{e}"),
			Error::Json(e) => write!(f, "{e}"),
		}
	}
}

/// `#[tauri::command]` needs the error type to cross to the frontend, where the
/// message is all that is used.
impl Serialize for Error {
	fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
		serializer.serialize_str(&self.to_string())
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
			| Error::DocumentMissing
			| Error::OutsideProject
			| Error::NotText
			| Error::AlreadyExists
			| Error::UnsupportedFormat(_) => None,
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
/// in each, and the manifest. Returns the new project's root.
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

/// Writes JSON through a temporary file and a rename, so an interrupted save
/// leaves the previous version intact rather than a truncated one.
pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
	let temp = path.with_extension("tmp");
	fs::write(&temp, to_json(value)?)?;
	fs::rename(&temp, path)?;
	Ok(())
}

fn fill(
	root: &Path,
	name: &str,
	format: Format,
	sections: &[Section],
	created_at: OffsetDateTime,
) -> Result<()> {
	let mut documents = Vec::new();
	for section in sections {
		let folder = root.join(section.folder);
		fs::create_dir(&folder)?;
		fs::write(folder.join(section.seed), "")?;
		documents.push(Document::new(section.folder, section.seed));
	}

	let mut manifest = Manifest::new(name, format, created_at);
	manifest.documents = documents;
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

fn store_path(app: &AppHandle) -> Result<PathBuf> {
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
	let Some(last) = store.last() else {
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

/// The `.md` files in each of the project's sections, as paths relative to the
/// root. Sections keep the order they are given and files within one are
/// sorted. Anything else is ignored: other extensions, nested folders, symlinks
/// and names that are not valid UTF-8.
pub fn scan(root: &Path, folders: &[String]) -> Result<Vec<String>> {
	let mut found = Vec::new();

	for folder in folders {
		let entries = match fs::read_dir(root.join(folder)) {
			Ok(entries) => entries,
			// A section the writer deleted is not an error; it simply holds
			// nothing.
			Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
			Err(e) => return Err(e.into()),
		};

		let mut files = Vec::new();
		for entry in entries {
			let entry = entry?;
			if !entry.file_type()?.is_file() {
				continue;
			}

			let name = entry.file_name();
			let Some(name) = name.to_str() else {
				continue;
			};
			if !is_markdown(name) {
				continue;
			}

			files.push(format!("{folder}/{name}"));
		}

		files.sort();
		found.append(&mut files);
	}

	Ok(found)
}

fn is_markdown(name: &str) -> bool {
	Path::new(name)
		.extension()
		.is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

/// The section a document belongs to, which is the first component of its path.
fn section_of(path: &str) -> Option<&str> {
	path.split_once('/').map(|(section, _)| section)
}

/// Brings the manifest's document list back in step with what `scan` found.
/// Documents that are still there keep their id and their relative order,
/// vanished ones are dropped, and unrecorded files are adopted at the end of
/// their section. The list is regrouped into section order, since that is the
/// order the sidebar reads it in. Returns whether anything changed.
pub fn reconcile(manifest: &mut Manifest, found: &[String]) -> bool {
	let on_disk: HashSet<&str> = found.iter().map(String::as_str).collect();

	let mut seen: HashSet<&str> = HashSet::new();
	let mut kept: HashMap<&str, Vec<Document>> = HashMap::new();
	for document in &manifest.documents {
		// A path recorded twice — only a hand-edited manifest can manage it —
		// would otherwise open as two documents over one file.
		if !on_disk.contains(document.path.as_str()) || !seen.insert(&document.path) {
			continue;
		}
		if let Some(section) = section_of(&document.path) {
			kept.entry(section).or_default().push(document.clone());
		}
	}

	let mut adopted: HashMap<&str, Vec<Document>> = HashMap::new();
	for path in found {
		if seen.contains(path.as_str()) {
			continue;
		}
		if let Some(section) = section_of(path) {
			adopted.entry(section).or_default().push(Document {
				id: Uuid::new_v4(),
				path: path.clone(),
			});
		}
	}

	let mut documents = Vec::new();
	for folder in &manifest.folders {
		if let Some(existing) = kept.remove(folder.as_str()) {
			documents.extend(existing);
		}
		if let Some(new) = adopted.remove(folder.as_str()) {
			documents.extend(new);
		}
	}

	let changed = documents != manifest.documents;
	manifest.documents = documents;
	changed
}

/// Reads the manifest and brings it back in step with the folder, writing it
/// back only when reconciliation actually changed something.
pub fn refresh(root: &Path) -> Result<Manifest> {
	let mut manifest = read_manifest(root)?;
	let found = scan(root, &manifest.folders)?;
	if reconcile(&mut manifest, &found) {
		write_json(&root.join(MANIFEST_FILE), &manifest)?;
	}
	Ok(manifest)
}

/// The manifest's documents grouped under their section, in the order the
/// project records both. Sections are fixed by the format, so an empty one is
/// still listed.
fn sections(manifest: &Manifest) -> Vec<SectionDocuments> {
	manifest
		.folders
		.iter()
		.map(|folder| SectionDocuments {
			folder: folder.clone(),
			documents: manifest
				.documents
				.iter()
				.filter(|d| section_of(&d.path) == Some(folder.as_str()))
				.map(DocumentView::from)
				.collect(),
		})
		.collect()
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

#[tauri::command]
pub fn open_project(app: AppHandle, root: PathBuf) -> Result<OpenedProject> {
	open_and_remember(&store_path(&app)?, &root, OffsetDateTime::now_utc())
}

/// Turns a document id into a path on disk, refusing anything that does not
/// end up inside the project. `aurora.json` is an ordinary file a writer can
/// edit, so the path it records is not to be trusted.
fn resolve(manifest: &Manifest, root: &Path, id: Uuid) -> Result<PathBuf> {
	let document = manifest
		.documents
		.iter()
		.find(|d| d.id == id)
		.ok_or(Error::UnknownDocument)?;

	// Canonicalising both sides resolves `..` and follows symlinks, so the
	// comparison is between two real locations.
	let path = root.join(&document.path);
	let path = path.canonicalize().map_err(|e| match e.kind() {
		io::ErrorKind::NotFound => Error::DocumentMissing,
		_ => Error::Io(e),
	})?;

	if !path.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}

	Ok(path)
}

/// The text of one document.
#[tauri::command]
pub fn read_document(root: PathBuf, id: Uuid) -> Result<String> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	let path = resolve(&manifest, &root, id)?;
	String::from_utf8(fs::read(path)?).map_err(|_| Error::NotText)
}

/// The sidebar's view of the project, straight from the manifest.
#[tauri::command]
pub fn list_documents(root: PathBuf) -> Result<Vec<SectionDocuments>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Ok(sections(&read_manifest(&root)?))
}

/// The same, after looking at the folder again for anything added, removed or
/// renamed outside Aurora.
#[tauri::command]
pub fn refresh_documents(root: PathBuf) -> Result<Vec<SectionDocuments>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Ok(sections(&refresh(&root)?))
}

#[tauri::command]
pub fn recent_projects(app: AppHandle) -> Result<Vec<store::RecentProject>> {
	available_recents(&store_path(&app)?)
}

#[tauri::command]
pub fn last_project(app: AppHandle) -> Result<LastProject> {
	resolve_last(&store_path(&app)?)
}

#[tauri::command]
pub fn forget_project(app: AppHandle, root: PathBuf) -> Result<()> {
	forget(&store_path(&app)?, &root)
}

#[cfg(test)]
mod tests {
	use super::*;

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
	fn every_section_seeds_a_markdown_file() {
		for section in Format::Novel.layout() {
			assert!(
				section.seed.ends_with(".md"),
				"{} seeds {}",
				section.folder,
				section.seed
			);
		}
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
			manifest.folders,
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
			let seed = folder.join(section.seed);
			assert!(seed.is_file(), "{} is missing", section.seed);
			assert_eq!(fs::read_to_string(&seed).unwrap(), "");
		}

		let json = fs::read_to_string(root.join(MANIFEST_FILE)).unwrap();
		let manifest: Manifest = serde_json::from_str(&json).unwrap();
		// The seed documents carry random ids and are checked on their own.
		assert_eq!(
			Manifest {
				documents: Vec::new(),
				..manifest
			},
			Manifest::new("Ithaca", Format::Novel, fixed_time())
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

		let paths: Vec<_> = manifest.documents.iter().map(|d| d.path.as_str()).collect();
		assert_eq!(
			paths,
			[
				"Manuscript/Chapter 1.md",
				"Outline/Outline.md",
				"Characters/Characters.md",
				"Locations/Locations.md",
				"Notes/Notes.md",
			]
		);

		let ids: HashSet<_> = manifest.documents.iter().map(|d| d.id).collect();
		assert_eq!(ids.len(), manifest.documents.len(), "ids must be unique");
	}

	#[test]
	fn every_recorded_document_is_on_disk() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		for document in read_manifest(&root).unwrap().documents {
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
		assert!(manifest.documents.is_empty());
	}

	#[test]
	fn a_document_id_survives_the_round_trip() {
		let document = Document::new("Manuscript", "Chapter 1.md");
		let json = serde_json::to_value(&document).unwrap();
		assert_eq!(json["path"], "Manuscript/Chapter 1.md");
		assert_eq!(json["id"], document.id.to_string());
		assert_eq!(serde_json::from_value::<Document>(json).unwrap(), document);
	}

	fn novel_folders() -> Vec<String> {
		Format::Novel
			.layout()
			.iter()
			.map(|s| s.folder.to_owned())
			.collect()
	}

	#[test]
	fn scan_finds_the_seed_files_in_section_order() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		assert_eq!(
			scan(&root, &novel_folders()).unwrap(),
			[
				"Manuscript/Chapter 1.md",
				"Outline/Outline.md",
				"Characters/Characters.md",
				"Locations/Locations.md",
				"Notes/Notes.md",
			]
		);
	}

	#[test]
	fn files_within_a_section_are_sorted() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		for name in ["Chapter 3.md", "Chapter 2.md"] {
			fs::write(root.join("Manuscript").join(name), "").unwrap();
		}

		let found = scan(&root, &novel_folders()).unwrap();
		assert_eq!(
			&found[..3],
			[
				"Manuscript/Chapter 1.md",
				"Manuscript/Chapter 2.md",
				"Manuscript/Chapter 3.md",
			]
		);
	}

	#[test]
	fn scan_ignores_everything_that_is_not_a_markdown_file() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manuscript = root.join("Manuscript");
		fs::write(manuscript.join("cover.png"), "").unwrap();
		fs::write(manuscript.join("notes.txt"), "").unwrap();
		fs::create_dir(manuscript.join("Part One")).unwrap();
		fs::write(manuscript.join("Part One").join("Chapter 2.md"), "").unwrap();

		let found = scan(&root, &novel_folders()).unwrap();
		assert_eq!(
			found
				.iter()
				.filter(|p| p.starts_with("Manuscript/"))
				.count(),
			1,
			"only the seed chapter belongs to Manuscript"
		);
	}

	#[test]
	fn an_uppercase_extension_is_still_markdown() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Notes").join("Ideas.MD"), "").unwrap();

		assert!(
			scan(&root, &novel_folders())
				.unwrap()
				.contains(&"Notes/Ideas.MD".to_owned())
		);
	}

	#[test]
	fn a_missing_section_is_skipped_rather_than_failing() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::remove_dir_all(root.join("Outline")).unwrap();

		let found = scan(&root, &novel_folders()).unwrap();
		assert!(!found.iter().any(|p| p.starts_with("Outline/")));
		assert_eq!(found.len(), 4);
	}

	#[test]
	fn scan_looks_only_where_it_is_told() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::create_dir(root.join("Scraps")).unwrap();
		fs::write(root.join("Scraps").join("Offcut.md"), "").unwrap();

		let found = scan(&root, &novel_folders()).unwrap();
		assert!(!found.iter().any(|p| p.starts_with("Scraps/")));
	}

	fn manifest_with(paths: &[&str]) -> Manifest {
		let mut manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());
		manifest.documents = paths
			.iter()
			.map(|path| Document {
				id: Uuid::new_v4(),
				path: (*path).to_owned(),
			})
			.collect();
		manifest
	}

	fn paths_of(manifest: &Manifest) -> Vec<&str> {
		manifest.documents.iter().map(|d| d.path.as_str()).collect()
	}

	fn owned(paths: &[&str]) -> Vec<String> {
		paths.iter().map(|p| (*p).to_owned()).collect()
	}

	#[test]
	fn reconcile_leaves_a_manifest_that_already_agrees_alone() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 1.md", "Notes/Notes.md"]);
		let before = manifest.documents.clone();

		assert!(!reconcile(
			&mut manifest,
			&owned(&["Manuscript/Chapter 1.md", "Notes/Notes.md"])
		));
		assert_eq!(manifest.documents, before);
	}

	#[test]
	fn a_new_file_is_adopted_at_the_end_of_its_section() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 1.md", "Notes/Notes.md"]);
		let chapter_one = manifest.documents[0].id;

		assert!(reconcile(
			&mut manifest,
			&owned(&[
				"Manuscript/Chapter 1.md",
				"Manuscript/Chapter 2.md",
				"Notes/Notes.md",
			])
		));
		assert_eq!(
			paths_of(&manifest),
			[
				"Manuscript/Chapter 1.md",
				"Manuscript/Chapter 2.md",
				"Notes/Notes.md",
			]
		);
		assert_eq!(
			manifest.documents[0].id, chapter_one,
			"an existing document keeps its id"
		);
	}

	#[test]
	fn a_vanished_file_is_dropped() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 1.md", "Notes/Notes.md"]);

		assert!(reconcile(&mut manifest, &owned(&["Notes/Notes.md"])));
		assert_eq!(paths_of(&manifest), ["Notes/Notes.md"]);
	}

	#[test]
	fn the_recorded_order_wins_over_the_order_on_disk() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 2.md", "Manuscript/Chapter 1.md"]);

		assert!(!reconcile(
			&mut manifest,
			&owned(&["Manuscript/Chapter 1.md", "Manuscript/Chapter 2.md"])
		));
		assert_eq!(
			paths_of(&manifest),
			["Manuscript/Chapter 2.md", "Manuscript/Chapter 1.md"]
		);
	}

	#[test]
	fn a_manifest_without_documents_adopts_everything() {
		let mut manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());
		let found = owned(&["Manuscript/Chapter 1.md", "Notes/Notes.md"]);

		assert!(reconcile(&mut manifest, &found));
		assert_eq!(paths_of(&manifest), found.as_slice());
		let ids: HashSet<_> = manifest.documents.iter().map(|d| d.id).collect();
		assert_eq!(ids.len(), 2, "each adopted file gets its own id");
	}

	#[test]
	fn documents_are_regrouped_into_section_order() {
		let mut manifest = manifest_with(&[
			"Notes/Notes.md",
			"Manuscript/Chapter 1.md",
			"Notes/Ideas.md",
		]);

		assert!(reconcile(
			&mut manifest,
			&owned(&[
				"Manuscript/Chapter 1.md",
				"Notes/Notes.md",
				"Notes/Ideas.md",
			])
		));
		assert_eq!(
			paths_of(&manifest),
			[
				"Manuscript/Chapter 1.md",
				"Notes/Notes.md",
				"Notes/Ideas.md"
			]
		);
	}

	#[test]
	fn a_path_recorded_twice_is_collapsed() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 1.md", "Manuscript/Chapter 1.md"]);
		let first = manifest.documents[0].id;

		assert!(reconcile(
			&mut manifest,
			&owned(&["Manuscript/Chapter 1.md"])
		));
		assert_eq!(paths_of(&manifest), ["Manuscript/Chapter 1.md"]);
		assert_eq!(manifest.documents[0].id, first, "the first id wins");
	}

	#[test]
	fn a_document_outside_the_projects_sections_is_dropped() {
		let mut manifest = manifest_with(&["Scraps/Offcut.md", "Notes/Notes.md"]);

		assert!(reconcile(
			&mut manifest,
			&owned(&["Scraps/Offcut.md", "Notes/Notes.md"])
		));
		assert_eq!(paths_of(&manifest), ["Notes/Notes.md"]);
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
		assert!(root.join("Manuscript").join("Chapter 1.md").is_file());

		let remembered = store::load(&store).unwrap();
		assert_eq!(remembered.last().unwrap().name, "Ithaca");
		assert_eq!(remembered.last().unwrap().root, root);
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
		assert!(store::load(&store).unwrap().last().is_none());
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
		assert!(paths_of(&manifest).contains(&"Manuscript/Chapter 2.md"));
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
		assert!(!paths_of(&manifest).contains(&"Notes/Notes.md"));
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
	fn listing_groups_documents_under_their_section() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let listed = sections(&read_manifest(&root).unwrap());

		let folders: Vec<_> = listed.iter().map(|s| s.folder.as_str()).collect();
		assert_eq!(
			folders,
			["Manuscript", "Outline", "Characters", "Locations", "Notes"]
		);
		assert_eq!(listed[0].documents.len(), 1);
		assert_eq!(listed[0].documents[0].title, "Chapter 1");
		assert_eq!(listed[0].documents[0].path, "Manuscript/Chapter 1.md");
	}

	#[test]
	fn an_empty_section_is_still_listed() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::remove_file(root.join("Notes").join("Notes.md")).unwrap();
		refresh(&root).unwrap();

		let listed = sections(&read_manifest(&root).unwrap());
		let notes = listed.iter().find(|s| s.folder == "Notes").unwrap();
		assert!(notes.documents.is_empty());
	}

	#[test]
	fn listing_does_not_look_at_the_folder_but_refreshing_does() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Notes").join("Ideas.md"), "").unwrap();

		let listed = sections(&read_manifest(&root).unwrap());
		let notes = listed.iter().find(|s| s.folder == "Notes").unwrap();
		assert_eq!(
			notes.documents.len(),
			1,
			"the manifest has not been re-read"
		);

		let refreshed = sections(&refresh(&root).unwrap());
		let notes = refreshed.iter().find(|s| s.folder == "Notes").unwrap();
		assert_eq!(notes.documents.len(), 2);
		assert_eq!(notes.documents[1].title, "Ideas");
	}

	#[test]
	fn the_listing_commands_refuse_a_relative_path() {
		let relative = PathBuf::from("some/where");
		assert!(matches!(
			list_documents(relative.clone()).unwrap_err(),
			Error::RelativePath
		));
		assert!(matches!(
			refresh_documents(relative).unwrap_err(),
			Error::RelativePath
		));
	}

	#[test]
	fn a_title_is_the_file_name_without_its_extension() {
		let document = Document::new("Manuscript", "Chapter 1.md");
		assert_eq!(document.title(), "Chapter 1");
		assert_eq!(DocumentView::from(&document).title, "Chapter 1");
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
		assert_eq!(store::load(&store).unwrap().last().unwrap().name, "Ithaca");
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

	/// Rewrites one document's recorded path, the way a hand-edited manifest
	/// could.
	fn set_document_path(root: &Path, index: usize, path: &str) {
		let file = root.join(MANIFEST_FILE);
		let mut value: serde_json::Value =
			serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
		value["documents"][index]["path"] = path.into();
		fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
	}

	fn first_document(root: &Path) -> Document {
		read_manifest(root).unwrap().documents[0].clone()
	}

	#[test]
	fn a_document_reads_back_what_was_written_to_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(
			root.join("Manuscript").join("Chapter 1.md"),
			"Sing to me of the man, Muse.",
		)
		.unwrap();

		let id = first_document(&root).id;
		assert_eq!(
			read_document(root, id).unwrap(),
			"Sing to me of the man, Muse."
		);
	}

	#[test]
	fn an_unknown_id_is_not_a_document() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = read_document(root, Uuid::new_v4()).unwrap_err();
		assert!(matches!(err, Error::UnknownDocument));
	}

	#[test]
	fn a_recorded_document_whose_file_has_gone_is_reported() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;
		fs::remove_file(root.join("Manuscript").join("Chapter 1.md")).unwrap();

		let err = read_document(root, id).unwrap_err();
		assert!(matches!(err, Error::DocumentMissing));
	}

	#[test]
	fn a_path_that_climbs_out_of_the_project_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(parent.path().join("secrets.md"), "not yours").unwrap();
		set_document_path(&root, 0, "../secrets.md");

		let id = first_document(&root).id;
		let err = read_document(root, id).unwrap_err();
		assert!(matches!(err, Error::OutsideProject));
	}

	#[cfg(unix)]
	#[test]
	fn a_symlink_out_of_the_project_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let outside = parent.path().join("secrets.md");
		fs::write(&outside, "not yours").unwrap();
		std::os::unix::fs::symlink(&outside, root.join("Manuscript").join("Link.md")).unwrap();
		set_document_path(&root, 0, "Manuscript/Link.md");

		let id = first_document(&root).id;
		let err = read_document(root, id).unwrap_err();
		assert!(matches!(err, Error::OutsideProject));
	}

	#[test]
	fn a_document_that_is_not_text_is_reported() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Manuscript").join("Chapter 1.md"), [0xff, 0xfe]).unwrap();

		let id = first_document(&root).id;
		let err = read_document(root, id).unwrap_err();
		assert!(matches!(err, Error::NotText));
	}

	#[test]
	fn reading_refuses_a_relative_path() {
		let err = read_document(PathBuf::from("some/where"), Uuid::new_v4()).unwrap_err();
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
	fn errors_cross_to_the_frontend_as_their_message() {
		let err = Error::InvalidName(NameError::IllegalCharacter(':'));
		assert_eq!(
			serde_json::to_string(&err).unwrap(),
			"\"the name cannot contain ':'\""
		);
		assert_eq!(
			serde_json::to_value(Error::AlreadyExists).unwrap(),
			"a folder of that name is already there"
		);
	}
}
