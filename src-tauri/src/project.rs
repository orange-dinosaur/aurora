use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use tauri::{AppHandle, Manager};
use time::OffsetDateTime;

use crate::document::{Document, refresh};
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
	UnknownSection,
	DocumentMissing,
	DocumentExists,
	BadDocumentPath,
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
				"that project needs a newer version of Aurora (it was saved as version \
				 {found}, this Aurora reads version {supported})"
			),
			Error::UnknownDocument => write!(f, "that document is not part of this project"),
			Error::UnknownSection => write!(f, "that section is not part of this project"),
			Error::DocumentMissing => write!(f, "that document's file is no longer there"),
			Error::DocumentExists => write!(
				f,
				"that document's file is there again — open it rather than writing over it"
			),
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
			Error::DocumentMissing => "documentMissing",
			Error::DocumentExists => "documentExists",
			Error::BadDocumentPath => "badDocumentPath",
			Error::OutsideProject => "outsideProject",
			Error::NotText => "notText",
			Error::AlreadyExists => "alreadyExists",
			Error::UnsupportedFormat(_) => "unsupportedFormat",
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
			| Error::DocumentMissing
			| Error::DocumentExists
			| Error::BadDocumentPath
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

	fn paths_of(manifest: &Manifest) -> Vec<&str> {
		manifest.documents.iter().map(|d| d.path.as_str()).collect()
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
}
