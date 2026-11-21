use super::*;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
	InvalidName(NameError),
	RelativePath,
	NoConfigDir,
	NoHomeDir,
	NotAProject,
	UnsupportedVersion {
		found: u32,
		supported: u32,
	},
	UnknownDocument,
	UnknownSection,
	UnknownFolder,
	FolderNotAllowed,
	SectionFixed,
	MoveInsideItself,
	DocumentMissing,
	ChangedOnDisk,
	DocumentExists,
	BadDocumentPath,
	OutsideProject,
	NotText,
	NotAnImage,
	CoverMissing,
	AlreadyExists,
	EmptyMessage,
	VersionName,
	VersionExists,
	NothingKept,
	MainFixed,
	CurrentVersion,
	ChangesPending,
	AlreadyMerged,
	ConflictsRemain,
	MergeUnderway,
	NotInManuscript,
	TakeExists,
	UnsupportedFormat(Format),
	StartOutOfRange,
	NothingToKeep,
	NameStaged,
	NoManifest,
	ScenesUnreadable(Vec<String>),
	ExportInsideProject,
	ExportStopped {
		written: Vec<String>,
		cause: Box<Error>,
	},
	Trash(trash::Error),
	Git(git2::Error),
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
			Error::NoHomeDir => write!(f, "Aurora could not find your home folder"),
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
			Error::ChangedOnDisk => write!(f, "Changed outside Aurora"),
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
			Error::NotAnImage => write!(f, "a cover has to be a PNG or a JPEG image"),
			Error::CoverMissing => write!(f, "the cover image is no longer in the project"),
			Error::AlreadyExists => write!(f, "a folder of that name is already there"),
			Error::EmptyMessage => write!(f, "a checkpoint needs a few words on what changed"),
			Error::VersionName => write!(f, "a version's name needs a letter or a digit"),
			Error::VersionExists => write!(f, "a version of that name is already there"),
			Error::NothingKept => {
				write!(f, "add a first checkpoint before starting another version")
			}
			Error::MainFixed => write!(f, "Main cannot be renamed or deleted"),
			Error::CurrentVersion => write!(f, "the version you are in cannot be deleted"),
			Error::ChangesPending => {
				write!(
					f,
					"the changes since the last checkpoint would be left behind"
				)
			}
			Error::AlreadyMerged => {
				write!(f, "everything in that version is already in this one")
			}
			Error::ConflictsRemain => {
				write!(f, "settle every conflict before adding the checkpoint")
			}
			Error::MergeUnderway => write!(f, "finish the merge first"),
			Error::NotInManuscript => {
				write!(f, "only a document in the Manuscript can start a take")
			}
			Error::TakeExists => write!(f, "a take of that name is already there"),
			Error::UnsupportedFormat(format) => {
				write!(f, "{format:?} projects cannot be created yet")
			}
			Error::StartOutOfRange => write!(
				f,
				"a structure takes 1 to {MAX_PARTS} parts, at least one chapter a part and at \
				 most {MAX_CHAPTERS} in all, and 0 to {MAX_SCENES} scenes a chapter"
			),
			Error::NothingToKeep => write!(f, "Nothing has changed since the last checkpoint"),
			Error::NameStaged => write!(f, "Stage the other renamed row too"),
			Error::NoManifest => write!(f, "that version holds no Aurora project to merge"),
			Error::ScenesUnreadable(paths) => {
				write!(f, "these scenes could not be read: {}", paths.join(", "))
			}
			Error::ExportInsideProject => write!(f, "Pick a folder outside the project"),
			Error::ExportStopped { written, cause } if written.is_empty() => {
				write!(f, "the export stopped before writing anything: {cause}")
			}
			Error::ExportStopped { written, cause } => write!(
				f,
				"the export stopped after writing {}: {cause}",
				written.join(", ")
			),
			Error::Trash(e) => write!(f, "the desktop's trash would not take the folder: {e}"),
			Error::Git(e) => write!(f, "Aurora could not keep this project's versions: {e}"),
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
			Error::NoHomeDir => "noHomeDir",
			Error::NotAProject => "notAProject",
			Error::UnsupportedVersion { .. } => "unsupportedVersion",
			Error::UnknownDocument => "unknownDocument",
			Error::UnknownSection => "unknownSection",
			Error::UnknownFolder => "unknownFolder",
			Error::FolderNotAllowed => "folderNotAllowed",
			Error::SectionFixed => "sectionFixed",
			Error::MoveInsideItself => "moveInsideItself",
			Error::DocumentMissing => "documentMissing",
			Error::ChangedOnDisk => "changedOnDisk",
			Error::DocumentExists => "documentExists",
			Error::BadDocumentPath => "badDocumentPath",
			Error::OutsideProject => "outsideProject",
			Error::NotText => "notText",
			Error::NotAnImage => "notAnImage",
			Error::CoverMissing => "coverMissing",
			Error::AlreadyExists => "alreadyExists",
			Error::EmptyMessage => "emptyMessage",
			Error::VersionName => "versionName",
			Error::VersionExists => "versionExists",
			Error::NothingKept => "nothingKept",
			Error::MainFixed => "mainFixed",
			Error::CurrentVersion => "currentVersion",
			Error::ChangesPending => "changesPending",
			Error::AlreadyMerged => "alreadyMerged",
			Error::ConflictsRemain => "conflictsRemain",
			Error::MergeUnderway => "mergeUnderway",
			Error::NotInManuscript => "notInManuscript",
			Error::TakeExists => "takeExists",
			Error::UnsupportedFormat(_) => "unsupportedFormat",
			Error::StartOutOfRange => "startOutOfRange",
			Error::NothingToKeep => "nothingToKeep",
			Error::NameStaged => "nameStaged",
			Error::NoManifest => "noManifest",
			Error::ScenesUnreadable(_) => "scenesUnreadable",
			Error::ExportInsideProject => "exportInsideProject",
			Error::ExportStopped { .. } => "exportStopped",
			Error::Trash(_) => "trash",
			Error::Git(_) => "git",
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
			Error::ExportStopped { cause, .. } => Some(cause.as_ref()),
			Error::Trash(e) => Some(e),
			Error::Git(e) => Some(e),
			Error::Io(e) => Some(e),
			Error::Json(e) => Some(e),
			_ => None,
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

impl From<git2::Error> for Error {
	fn from(e: git2::Error) -> Self {
		Error::Git(e)
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
pub(super) const ILLEGAL: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Windows refuses these as filenames whatever the extension.
pub(crate) const RESERVED: &[&str] = &[
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
