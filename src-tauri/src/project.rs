use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, Serializer};
use time::OffsetDateTime;

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
	/// The sections a new project of this format is created with. An empty
	/// layout means the format cannot be created yet.
	pub fn layout(self) -> &'static [Section] {
		match self {
			Format::Novel => NOVEL,
			Format::Screenplay | Format::ShortStories | Format::StagePlay => &[],
		}
	}
}

/// Bumped when the on-disk shape changes in a way older builds cannot read.
pub const MANIFEST_VERSION: u32 = 1;

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
}

impl Manifest {
	pub fn new(name: impl Into<String>, format: Format, created_at: OffsetDateTime) -> Self {
		Self {
			version: MANIFEST_VERSION,
			name: name.into(),
			format,
			created_at,
			folders: format
				.layout()
				.iter()
				.map(|s| s.folder.to_owned())
				.collect(),
		}
	}
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
	InvalidName(NameError),
	RelativePath,
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

fn fill(
	root: &Path,
	name: &str,
	format: Format,
	sections: &[Section],
	created_at: OffsetDateTime,
) -> Result<()> {
	for section in sections {
		let folder = root.join(section.folder);
		fs::create_dir(&folder)?;
		fs::write(folder.join(section.seed), "")?;
	}

	let manifest = Manifest::new(name, format, created_at);
	let json = serde_json::to_vec_pretty(&manifest)?;
	fs::write(root.join(MANIFEST_FILE), json)?;
	Ok(())
}

#[tauri::command]
pub fn create_project(parent: PathBuf, name: String, format: Format) -> Result<PathBuf> {
	// The path arrives from the frontend, so it is not trusted to be sensible.
	if !parent.is_absolute() {
		return Err(Error::RelativePath);
	}
	create(&parent, &name, format, OffsetDateTime::now_utc())
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
	fn manifest_serializes_with_camel_case_keys() {
		let manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());
		let json = serde_json::to_value(&manifest).unwrap();
		assert_eq!(json["version"], 1);
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
		assert_eq!(
			manifest,
			Manifest::new("Ithaca", Format::Novel, fixed_time())
		);
	}

	#[test]
	fn the_manifest_is_written_for_a_human_to_read() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let json = fs::read_to_string(root.join(MANIFEST_FILE)).unwrap();
		assert!(json.contains("\n"), "the manifest should be pretty-printed");
		assert!(json.contains("\"createdAt\": \"2023-11-14T22:13:20Z\""));
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
	fn the_command_creates_a_project() {
		let parent = tempfile::tempdir().unwrap();
		let root = create_project(
			parent.path().to_path_buf(),
			"Ithaca".to_owned(),
			Format::Novel,
		)
		.unwrap();

		assert!(root.join(MANIFEST_FILE).is_file());
		assert!(root.join("Manuscript").join("Chapter 1.md").is_file());
	}

	#[test]
	fn the_command_stamps_the_current_time() {
		let parent = tempfile::tempdir().unwrap();
		let before = OffsetDateTime::now_utc();
		let root = create_project(
			parent.path().to_path_buf(),
			"Ithaca".to_owned(),
			Format::Novel,
		)
		.unwrap();

		let json = fs::read_to_string(root.join(MANIFEST_FILE)).unwrap();
		let manifest: Manifest = serde_json::from_str(&json).unwrap();
		assert!(manifest.created_at >= before);
		assert!(manifest.created_at <= OffsetDateTime::now_utc());
	}

	#[test]
	fn the_command_refuses_a_relative_path() {
		let err = create_project(
			PathBuf::from("some/where"),
			"Ithaca".to_owned(),
			Format::Novel,
		)
		.unwrap_err();
		assert!(matches!(err, Error::RelativePath));
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
