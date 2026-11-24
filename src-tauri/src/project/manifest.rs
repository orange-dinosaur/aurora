use super::*;

/// A folder inside a project, together with the document it starts life with.
/// A section with no seed is created empty.
pub struct Section {
	pub folder: &'static str,
	pub seed: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum Format {
	Novel,
	Screenplay,
	ShortStories,
	StagePlay,
}

/// The one section with a hierarchy of its own. A folder anywhere else is a
/// folder and nothing more.
pub const MANUSCRIPT: &str = "Manuscript";

/// The two folders that hold what surrounds the story. Nothing on disk marks
/// them: a writer makes them by spelling the name.
pub const FRONT_MATTER: &str = "Front Matter";
pub const BACK_MATTER: &str = "Back Matter";

/// Whether a folder of this kind is the Manuscript's front or back matter. An
/// acknowledgement is words the writer wrote and not words of the story, so the
/// counts pass these two over.
///
/// The kind is what decides it, not the name: the folder may be called anything
/// once it exists, and only the Manuscript can hold one at all.
pub fn matter(kind: Option<FolderKind>) -> bool {
	kind.is_some_and(FolderKind::is_matter)
}

/// Gives the two matter folders their kind in a project made before the kind
/// existed, and in one where the writer made the folder outside Aurora. A
/// folder directly inside the Manuscript, with no kind and one of the two
/// names, is what it says it is. Says whether it changed anything, so a caller
/// knows whether the manifest is worth writing back.
///
/// This is the one place a name still decides a kind, and it only ever looks at
/// a folder that has none: renaming an adopted folder afterwards leaves it
/// front matter, and a `Front Matter` deeper in the Manuscript stays the
/// ordinary folder it has always been.
pub(crate) fn adopt_matter(nodes: &mut [Node]) -> bool {
	let Some(Node::Folder { children, .. }) = nodes
		.iter_mut()
		.find(|node| node.name() == MANUSCRIPT && matches!(node, Node::Folder { .. }))
	else {
		return false;
	};

	let mut adopted = false;
	for node in children.iter_mut() {
		let Node::Folder { name, kind, .. } = node else {
			continue;
		};
		if kind.is_some() {
			continue;
		}
		*kind = match name.as_str() {
			FRONT_MATTER => Some(FolderKind::FrontMatter),
			BACK_MATTER => Some(FolderKind::BackMatter),
			_ => continue,
		};
		adopted = true;
	}

	adopted
}

pub(super) const NOVEL: &[Section] = &[
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

/// A screenplay's Manuscript holds acts, and an act holds scenes.
pub(super) const SCREENPLAY: &[Section] = &[
	Section {
		folder: "Manuscript",
		seed: Some("Act One/Scene 1.md"),
	},
	Section {
		folder: "Outline",
		seed: Some("Outline.md"),
	},
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

/// A collection's Manuscript holds stories, each laid out as a novel's part.
pub(super) const SHORT_STORIES: &[Section] = &[
	Section {
		folder: "Manuscript",
		seed: Some("Story 1/Scene 1.md"),
	},
	Section {
		folder: "Outline",
		seed: Some("Outline.md"),
	},
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
	/// The formats the Welcome screen offers, in order. Stage Play stays in the
	/// enum so older manifests still parse, but is no longer offered.
	pub const ALL: &'static [Format] = &[Format::Novel, Format::Screenplay, Format::ShortStories];

	/// The sections a new project of this format is created with. An empty
	/// layout means the format cannot be created.
	pub fn layout(self) -> &'static [Section] {
		match self {
			Format::Novel => NOVEL,
			Format::Screenplay => SCREENPLAY,
			Format::ShortStories => SHORT_STORIES,
			Format::StagePlay => &[],
		}
	}
}

/// A format as the Welcome screen needs to describe it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
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
pub const MANIFEST_VERSION: u32 = 5;

/// What someone did on the book besides write it. Every format Aurora exports
/// wants the relationship named rather than a free line of text, so the set is
/// fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Role {
	Editor,
	CopyEditor,
	Proofreader,
	CoverDesigner,
	Illustrator,
	Translator,
	Narrator,
}

/// Someone who worked on the book, and what they did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Contributor {
	pub name: String,
	pub role: Role,
}

/// What the project's one book says about itself. Everything here is the
/// writer's to fill in and may stay empty; an export takes what it finds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Book {
	/// Generated once and never rewritten. A reader tells one book from another
	/// by this, so a new one would make every earlier export a different book.
	pub identifier: Uuid,
	pub title: String,
	#[serde(default)]
	pub subtitle: String,
	#[serde(default)]
	pub author: String,
	#[serde(default)]
	pub contributors: Vec<Contributor>,
	#[serde(default)]
	pub series: String,
	/// Where in the series this one falls. Text, because a book can be 1.5.
	#[serde(default)]
	pub series_number: String,
	/// A BCP 47 tag such as `en` or `pt-BR`.
	#[serde(default)]
	pub language: String,
	#[serde(default)]
	pub blurb: String,
	/// Where the cover image is, relative to the project root.
	#[serde(default)]
	pub cover: String,
	#[serde(default)]
	pub publisher: String,
	/// As the writer typed it: a year alone is as valid an answer as a date.
	#[serde(default)]
	pub publication_date: String,
	#[serde(default)]
	pub isbn: String,
	#[serde(default)]
	pub keywords: Vec<String>,
	#[serde(default)]
	pub copyright: String,
	/// How to reach the writer, for a screenplay's title page.
	#[serde(default)]
	pub contact: String,
	/// Which draft this is, for a screenplay's title page. Free text: "Second
	/// draft", "Blue revisions".
	#[serde(default)]
	pub draft: String,
	/// The formats ticked on the Export panel, so the next export starts from
	/// the choice the last one made.
	#[serde(default)]
	pub export_formats: Vec<ExportFormat>,
}

/// A kind of file the book can be exported as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum ExportFormat {
	Markdown,
	Epub,
	Kepub,
	Docx,
	Text,
	Fountain,
	Pdf,
	Fdx,
}

impl ExportFormat {
	/// Whether this is a screenplay's format rather than a book's. A
	/// screenplay exports only these, and every other project only the rest.
	pub fn for_scripts(self) -> bool {
		matches!(
			self,
			ExportFormat::Fountain | ExportFormat::Pdf | ExportFormat::Fdx
		)
	}

	/// What the writer knows it as, as the Export dialog names it.
	pub fn name(self) -> &'static str {
		match self {
			ExportFormat::Markdown => "Markdown",
			ExportFormat::Epub => "EPUB",
			ExportFormat::Kepub => "KEPUB",
			ExportFormat::Docx => "DOCX",
			ExportFormat::Text => "plain text",
			ExportFormat::Fountain => "Fountain",
			ExportFormat::Pdf => "PDF",
			ExportFormat::Fdx => "Final Draft",
		}
	}
}

impl Book {
	/// A book the writer has said nothing about yet. The project's own name is
	/// a better first guess at the title than an empty box.
	pub fn new(title: impl Into<String>) -> Self {
		Self {
			identifier: Uuid::new_v4(),
			title: title.into(),
			subtitle: String::new(),
			author: String::new(),
			contributors: Vec::new(),
			series: String::new(),
			series_number: String::new(),
			language: String::new(),
			blurb: String::new(),
			cover: String::new(),
			publisher: String::new(),
			publication_date: String::new(),
			isbn: String::new(),
			keywords: Vec::new(),
			copyright: String::new(),
			contact: String::new(),
			draft: String::new(),
			export_formats: Vec::new(),
		}
	}
}

/// The contents of `aurora.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
	pub version: u32,
	pub name: String,
	pub format: Format,
	#[serde(with = "time::serde::rfc3339")]
	pub created_at: OffsetDateTime,
	/// A project holds exactly one book, whether or not the writer has filled
	/// anything in about it.
	pub book: Book,
	/// Everything the project holds: its sections in the order it was made
	/// with, and under each of them the folders and documents the writer put
	/// there. A project keeps its own sections even if the format's definition
	/// changes later.
	pub nodes: Vec<Node>,
	/// The names the writer put on stretches of the timeline. A project with
	/// none writes no key.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub markers: Vec<Marker>,
}

/// A name on the timeline, such as "Three years later", for the stretch from
/// the scene `before` up to the next marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Marker {
	pub id: Uuid,
	pub name: String,
	pub before: Uuid,
}

impl Manifest {
	pub fn new(name: impl Into<String>, format: Format, created_at: OffsetDateTime) -> Self {
		let name = name.into();
		Self {
			version: MANIFEST_VERSION,
			book: Book::new(name.clone()),
			name,
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
			markers: Vec::new(),
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
/// Which fields are there decides how it is read, rather than the version
/// number, because that number is a line in a file the writer can edit.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Stored {
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
	book: Option<Book>,
	#[serde(default)]
	markers: Vec<Marker>,
}

impl<'de> Deserialize<'de> for Manifest {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
		let stored = Stored::deserialize(deserializer)?;
		Ok(Self {
			version: stored.version,
			// A project written before books existed is given one, named after
			// itself. The identifier is settled the next time anything writes
			// the manifest.
			book: stored
				.book
				.unwrap_or_else(|| Book::new(stored.name.as_str())),
			name: stored.name,
			format: stored.format,
			created_at: stored.created_at,
			nodes: stored
				.nodes
				.unwrap_or_else(|| tree_from_flat(&stored.folders, &stored.documents)),
			markers: stored.markers,
		})
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
	use std::io::Write;

	let temp = path.with_extension("tmp");
	let written = fs::File::create(&temp)
		.and_then(|mut file| {
			file.write_all(bytes)?;
			file.sync_all()
		})
		.and_then(|()| fs::rename(&temp, path));
	if let Err(e) = written {
		let _ = fs::remove_file(&temp);
		return Err(e.into());
	}
	Ok(())
}

pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
	write_atomic(path, &to_json(value)?)
}

pub fn read_manifest(root: &Path) -> Result<Manifest> {
	let bytes = fs::read(root.join(MANIFEST_FILE)).map_err(|e| match e.kind() {
		io::ErrorKind::NotFound => Error::NotAProject,
		_ => Error::Io(e),
	})?;
	let mut manifest: Manifest = serde_json::from_slice(&bytes)?;
	// An older Aurora cannot know what a newer one added, so refuse rather than
	// silently dropping fields and writing them away on the next save.
	if manifest.version > MANIFEST_VERSION {
		return Err(Error::UnsupportedVersion {
			found: manifest.version,
			supported: MANIFEST_VERSION,
		});
	}
	// Every read agrees about what the matter folders are, whatever version
	// wrote them. What is on disk catches up the next time anything is saved,
	// which is the same bargain the rest of the manifest is read under.
	adopt_matter(&mut manifest.nodes);
	Ok(manifest)
}

/// The manifest, for a command that changes the project. None may while a
/// merge is under way: the binder and the files are what the merge is settling.
pub(crate) fn editable(root: &Path) -> Result<Manifest> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	if versions::merge_underway(root)? {
		return Err(Error::MergeUnderway);
	}
	read_manifest(root)
}

/// Writes the manifest back, at the current version whatever version it was
/// read as: what goes to disk from here is always a tree.
pub(crate) fn write_manifest(root: &Path, manifest: &mut Manifest) -> Result<()> {
	manifest.version = MANIFEST_VERSION;
	write_json(&root.join(MANIFEST_FILE), manifest)
}

/// Deletes a file if it is there. A file that was already gone is the state
/// the caller wanted, not a failure.
pub(crate) fn remove_if_there(path: &Path) -> Result<()> {
	match fs::remove_file(path) {
		Err(e) if e.kind() != io::ErrorKind::NotFound => Err(Error::Io(e)),
		_ => Ok(()),
	}
}
