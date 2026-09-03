use std::collections::HashSet;
use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::{Date, Month, OffsetDateTime};
use uuid::Uuid;

use crate::project::{
	Error, Manifest, Result, read_manifest, validate_name, write_atomic, write_manifest,
};
use crate::tree::{self, FolderKind, Node};

/// A document inside a project. The path is relative to the project root and
/// always uses forward slashes, so a manifest written on one platform still
/// resolves on another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
	pub id: Uuid,
	pub path: String,
	/// How many words the writer is aiming at, if they have said. Left out of
	/// the manifest entirely when there is no target, so a project that never
	/// uses them reads exactly as it did before.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub target: Option<u32>,
}

impl Document {
	/// A document at a path, for tests that want one without a project around
	/// it. Everywhere else a document comes out of the tree.
	#[cfg(test)]
	pub(crate) fn new(section: &str, file_name: &str) -> Self {
		Self {
			id: Uuid::new_v4(),
			path: format!("{section}/{file_name}"),
			target: None,
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
	pub folder: String,
	pub title: String,
	/// Always present here, `null` when unset, so the front end has one shape
	/// to read rather than a field that comes and goes.
	pub target: Option<u32>,
}

impl From<&Document> for DocumentView {
	fn from(document: &Document) -> Self {
		Self {
			id: document.id,
			path: document.path.clone(),
			folder: section_of(&document.path).unwrap_or_default().to_owned(),
			title: document.title(),
			target: document.target,
		}
	}
}

/// Where a deleted document goes. It sits inside the project so that a move
/// into it is a rename rather than a copy, and `scan` never looks at it: it
/// only ever reads the section folders the manifest names.
pub const TRASH_DIR: &str = ".trash";

/// How much of a document's opening the overview carries. Long enough to
/// recognise a chapter by, short enough that a whole section stays small.
const EXCERPT_CHARS: usize = 240;

/// A document as the section overview shows it: the sidebar's view plus enough
/// of the file to tell one chapter from another without opening it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentSummary {
	#[serde(flatten)]
	pub document: DocumentView,
	pub words: usize,
	pub excerpt: String,
	#[serde(with = "time::serde::rfc3339::option")]
	pub modified: Option<OffsetDateTime>,
}

impl DocumentSummary {
	/// A card for a document whose file cannot be read.
	fn blank(document: &Document) -> Self {
		Self {
			document: document.into(),
			words: 0,
			excerpt: String::new(),
			modified: None,
		}
	}
}

/// One of a folder's children, as the overview draws it. A document gets the
/// card it has always had; a folder says what it is and how much it holds,
/// which is all a card can show without reading everything below it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "node", rename_all = "camelCase")]
pub enum ChildSummary {
	Folder {
		id: Uuid,
		name: String,
		kind: Option<FolderKind>,
		/// How many nodes it holds directly, folders and documents alike.
		children: usize,
	},
	Document(DocumentSummary),
}

/// One of the project's sections and the documents in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SectionDocuments {
	pub folder: String,
	pub documents: Vec<DocumentView>,
}

/// One entry in the project's tree as the front end reads it: [`tree::Node`]
/// with the things a node's place decides already worked out. A document
/// carries the same view the flat lists send, so nothing outside Rust ever
/// takes a path apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "node", rename_all = "camelCase")]
pub enum NodeView {
	Folder {
		id: Uuid,
		name: String,
		/// Always present, `null` outside the Manuscript, for the same reason
		/// a document's target is.
		kind: Option<FolderKind>,
		children: Vec<NodeView>,
	},
	Document(DocumentView),
}

/// The tree under `prefix`, ready to send. The prefix is how far down the walk
/// has come, which is what turns a node's name into its path.
fn views(nodes: &[Node], prefix: &str) -> Vec<NodeView> {
	nodes
		.iter()
		.map(|node| match node {
			Node::Folder {
				id,
				name,
				kind,
				children,
			} => NodeView::Folder {
				id: *id,
				name: name.clone(),
				kind: *kind,
				children: views(children, &format!("{prefix}{name}/")),
			},
			Node::Document { id, name, target } => {
				let document = Document {
					id: *id,
					path: format!("{prefix}{name}"),
					target: *target,
				};
				NodeView::Document((&document).into())
			}
		})
		.collect()
}

/// A document and the whole of its text, for reading the project in one go.
/// The text is `None` when the file could not be read, which the front end
/// shows rather than swallows: a document nobody could look at must not read
/// as a document with nothing in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentText {
	#[serde(flatten)]
	pub document: DocumentView,
	pub text: Option<String>,
}

/// What each of the project's sections holds, as a tree: every `.md` file
/// under it and every directory on the way to one, however deep. Sections keep
/// the order they are given and what a directory holds is sorted by name.
/// Anything else is ignored: other extensions, symlinks, hidden names and names
/// that are not valid UTF-8.
///
/// The disk has no ids to give, so every node comes back with a fresh one.
/// Matching what is here against what the project already knows is
/// `reconcile`'s job.
pub fn scan(root: &Path, folders: &[String]) -> Result<Vec<Node>> {
	let mut sections = Vec::new();

	for folder in folders {
		// A section the writer deleted is not an error; it simply is not there.
		let Some(children) = read_level(&root.join(folder))? else {
			continue;
		};
		sections.push(Node::Folder {
			id: Uuid::new_v4(),
			name: folder.clone(),
			kind: None,
			children,
		});
	}

	Ok(sections)
}

/// What one directory holds, or `None` when there is no such directory.
fn read_level(dir: &Path) -> Result<Option<Vec<Node>>> {
	let entries = match fs::read_dir(dir) {
		Ok(entries) => entries,
		Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
		Err(e) => return Err(e.into()),
	};

	let mut found = Vec::new();
	for entry in entries {
		let entry = entry?;
		let name = entry.file_name();
		let Some(name) = name.to_str() else {
			continue;
		};
		// The trash lives inside the project without being part of it, and
		// nothing else hidden is the writer's work either.
		if name.starts_with('.') {
			continue;
		}

		// A symlink is neither, so it is passed over as it always has been.
		let entry_type = entry.file_type()?;
		if entry_type.is_dir() {
			found.push(Node::Folder {
				id: Uuid::new_v4(),
				name: name.to_owned(),
				kind: None,
				children: read_level(&entry.path())?.unwrap_or_default(),
			});
		} else if entry_type.is_file() && is_markdown(name) {
			found.push(Node::Document {
				id: Uuid::new_v4(),
				name: name.to_owned(),
				target: None,
			});
		}
	}

	found.sort_by(|a, b| a.name().cmp(b.name()));
	Ok(Some(found))
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

/// Brings the manifest's tree back in step with what `scan` found. The
/// project's sections are its own and stay whether or not their folders are
/// still on disk; under them, a node whose file or directory is still there
/// keeps its id and its place, a vanished one is dropped, and anything the
/// project has not seen is adopted at the end of the level it was found in.
/// Returns whether anything changed.
pub fn reconcile(manifest: &mut Manifest, found: &[Node]) -> bool {
	let mut sections = Vec::new();
	let mut seen: HashSet<&str> = HashSet::new();

	for section in &manifest.nodes {
		// Only folders belong at the top of a project. A document up there is
		// something hand-editing put in, and it is not carried over.
		let Node::Folder {
			id,
			name,
			kind,
			children,
		} = section
		else {
			continue;
		};
		// A section recorded twice — again only by hand — would otherwise show
		// twice in the sidebar over the one folder.
		if !seen.insert(name.as_str()) {
			continue;
		}

		// A section folder the writer deleted keeps its place in the project.
		// What was inside it has gone, which reads the same as an empty one.
		let below = match found.iter().find(|node| node.name() == name) {
			Some(Node::Folder { children, .. }) => children.as_slice(),
			_ => &[],
		};

		sections.push(Node::Folder {
			id: *id,
			name: name.clone(),
			kind: *kind,
			children: merge(children, below),
		});
	}

	let changed = sections != manifest.nodes;
	manifest.nodes = sections;
	changed
}

/// One level of the project against the same level on disk.
fn merge(known: &[Node], found: &[Node]) -> Vec<Node> {
	let mut merged = Vec::new();
	let mut seen: HashSet<&str> = HashSet::new();

	for node in known {
		// A name recorded twice would otherwise be two nodes over one file.
		if !seen.insert(node.name()) {
			continue;
		}
		let Some(on_disk) = found.iter().find(|other| other.name() == node.name()) else {
			continue;
		};

		merged.push(match (node, on_disk) {
			(
				Node::Folder {
					id,
					name,
					kind,
					children,
				},
				Node::Folder {
					children: below, ..
				},
			) => Node::Folder {
				id: *id,
				name: name.clone(),
				kind: *kind,
				children: merge(children, below),
			},
			(Node::Document { .. }, Node::Document { .. }) => node.clone(),
			// A name that was a file and is now a directory, or the other way
			// about, is a new thing under an old name. What is on disk wins.
			_ => on_disk.clone(),
		});
	}

	for node in found {
		if seen.insert(node.name()) {
			merged.push(node.clone());
		}
	}

	merged
}

/// Reads the manifest and brings it back in step with the folder, writing it
/// back only when reconciliation actually changed something.
pub fn refresh(root: &Path) -> Result<Manifest> {
	let mut manifest = read_manifest(root)?;
	let found = scan(root, &manifest.folders())?;
	if reconcile(&mut manifest, &found) {
		write_manifest(root, &mut manifest)?;
	}
	Ok(manifest)
}

/// The manifest's documents grouped under their section, in the order the
/// project records both. Sections are fixed by the format, so an empty one is
/// still listed.
fn sections(manifest: &Manifest) -> Vec<SectionDocuments> {
	let documents = manifest.documents();
	manifest
		.folders()
		.into_iter()
		.map(|folder| SectionDocuments {
			documents: documents
				.iter()
				.filter(|d| section_of(&d.path) == Some(folder.as_str()))
				.map(DocumentView::from)
				.collect(),
			folder,
		})
		.collect()
}

/// The opening of a document collapsed onto one line and cut to something a
/// card can hold.
fn excerpt(text: &str) -> String {
	let mut excerpt = String::new();
	let mut length = 0;

	for word in text.split_whitespace() {
		let separator = usize::from(!excerpt.is_empty());
		let width = word.chars().count();

		if length + separator + width > EXCERPT_CHARS {
			// A first word longer than the whole excerpt still has to show
			// something.
			if excerpt.is_empty() {
				excerpt.extend(word.chars().take(EXCERPT_CHARS));
			}
			excerpt.push('\u{2026}');
			break;
		}

		if separator == 1 {
			excerpt.push(' ');
		}
		excerpt.push_str(word);
		length += separator + width;
	}

	excerpt
}

/// Everything the overview shows about one document, from a single open of its
/// file. A file that cannot be read gives a blank card rather than failing the
/// whole section.
fn summarise(document: &Document, path: &Path) -> DocumentSummary {
	let Ok(mut file) = fs::File::open(path) else {
		return DocumentSummary::blank(document);
	};

	let modified = file
		.metadata()
		.and_then(|metadata| metadata.modified())
		.ok()
		.map(OffsetDateTime::from);

	let mut bytes = Vec::new();
	let text = match file.read_to_end(&mut bytes) {
		Ok(_) => String::from_utf8(bytes).unwrap_or_default(),
		Err(_) => String::new(),
	};

	DocumentSummary {
		document: document.into(),
		words: text.split_whitespace().count(),
		excerpt: excerpt(&text),
		modified,
	}
}

/// Turns a document id into a path on disk, refusing anything that does not
/// end up inside the project. `aurora.json` is an ordinary file a writer can
/// edit, so the path it records is not to be trusted.
fn resolve(manifest: &Manifest, root: &Path, id: Uuid) -> Result<PathBuf> {
	let relative = match tree::find(&manifest.nodes, id) {
		Some(Node::Document { .. }) => {
			tree::path(&manifest.nodes, id).ok_or(Error::UnknownDocument)?
		}
		// A folder is not a document, and neither is an id nothing carries.
		_ => return Err(Error::UnknownDocument),
	};

	// Canonicalising both sides resolves `..` and follows symlinks, so the
	// comparison is between two real locations.
	let path = root.join(relative);
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

/// Replaces one document's text. The document has to be one the project knows
/// about and its file has to still be there; writing back a file that has
/// vanished is a different thing to ask for.
#[tauri::command]
pub fn write_document(root: PathBuf, id: Uuid, text: String) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	let path = resolve(&manifest, &root, id)?;
	write_atomic(&path, text.as_bytes())
}

/// Turns a relative path into somewhere this project is willing to keep a
/// document: a Markdown file under one of the format's sections, at whatever
/// depth. `base` is the project root, or the trash inside it, which is laid out
/// the same way. The path may come from the frontend or be walked out of the
/// manifest, and neither is trusted.
fn document_path(manifest: &Manifest, base: &Path, path: &str) -> Result<PathBuf> {
	let Some((section, rest)) = path.split_once('/') else {
		return Err(Error::BadDocumentPath);
	};

	let ordinary =
		|part: &str| !part.is_empty() && part != "." && part != ".." && !part.contains('\\');

	let mut parts = rest.split('/').peekable();
	let mut file = base.join(section);
	if !ordinary(section) || !manifest.folders().iter().any(|folder| folder == section) {
		return Err(Error::BadDocumentPath);
	}

	while let Some(part) = parts.next() {
		// Only the last part is the file, and only it has to be Markdown.
		if !ordinary(part) || (parts.peek().is_none() && !is_markdown(part)) {
			return Err(Error::BadDocumentPath);
		}
		file.push(part);
	}

	Ok(file)
}

/// Puts a document that has gone missing back on disk and brings the manifest
/// with it. The path comes from the tab that still holds the text rather than
/// from the manifest, since a refresh may already have dropped the document.
/// The document it returns is the one to carry on editing: the same id if the
/// manifest still knew the path, a fresh one if it had to be adopted again.
#[tauri::command]
pub fn restore_document(root: PathBuf, path: String, text: String) -> Result<DocumentView> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	add_document(&root, &manifest, &path, &text)
}

/// Puts a file that is not there yet at a relative path this project is willing
/// to accept, and brings the manifest with it. Shared by restoring a vanished
/// document and creating a new one: on disk the two are the same act, and the
/// document that comes back is the one to start editing.
fn add_document(root: &Path, manifest: &Manifest, path: &str, text: &str) -> Result<DocumentView> {
	let file = document_path(manifest, root, path)?;
	if file.exists() {
		return Err(Error::DocumentExists);
	}

	let folder = file.parent().ok_or(Error::BadDocumentPath)?;
	fs::create_dir_all(folder)?;
	// A section folder can be a symlink the writer made, so where it actually
	// leads is what decides whether this write stays inside the project.
	if !folder.canonicalize()?.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}

	write_atomic(&file, text.as_bytes())?;

	refresh(root)?
		.documents()
		.iter()
		.find(|d| d.path == path)
		.map(DocumentView::from)
		.ok_or(Error::UnknownDocument)
}

/// `.md` is Aurora's business rather than the writer's, so a name that already
/// carries it means the same thing as one that does not, and gets it taken off
/// rather than doubled.
fn without_extension(name: &str) -> &str {
	match name.rfind('.') {
		Some(dot) if name[dot..].eq_ignore_ascii_case(".md") => &name[..dot],
		_ => name,
	}
}

/// Starts a new, empty document in one of the project's sections. The name is
/// the title.
#[tauri::command]
pub fn create_document(root: PathBuf, section: String, name: String) -> Result<DocumentView> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let name = without_extension(&name);
	validate_name(name)?;

	let manifest = read_manifest(&root)?;
	if !manifest.folders().contains(&section) {
		return Err(Error::UnknownSection);
	}

	add_document(&root, &manifest, &format!("{section}/{name}.md"), "")
}

/// The sidebar's view of the project, straight from the manifest.
#[tauri::command]
pub fn list_documents(root: PathBuf) -> Result<Vec<SectionDocuments>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Ok(sections(&read_manifest(&root)?))
}

/// The project as a tree, straight from the manifest. The sidebar reads this;
/// the flat lists above it are what everything else still asks for.
#[tauri::command]
pub fn document_tree(root: PathBuf) -> Result<Vec<NodeView>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Ok(views(&read_manifest(&root)?.nodes, ""))
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

/// Gives a document a new title, which is to say a new file name. It stays in
/// its section, and keeps its id and its place in the order.
#[tauri::command]
pub fn rename_document(root: PathBuf, id: Uuid, name: String) -> Result<DocumentView> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let name = without_extension(&name);
	validate_name(name)?;

	let mut manifest = read_manifest(&root)?;
	let from = resolve(&manifest, &root, id)?;

	let was = tree::path(&manifest.nodes, id).ok_or(Error::UnknownDocument)?;
	let file = format!("{name}.md");
	// A document is renamed where it stands, so its path is the one it has
	// with the last part swapped.
	let path = match was.rfind('/') {
		Some(slash) => format!("{}/{file}", &was[..slash]),
		None => return Err(Error::BadDocumentPath),
	};

	let view = |manifest: &Manifest| {
		manifest
			.documents()
			.iter()
			.find(|d| d.id == id)
			.map(DocumentView::from)
			.ok_or(Error::UnknownDocument)
	};

	// Being renamed to what it is already called is not a collision with
	// itself.
	if path == was {
		return view(&manifest);
	}

	let to = document_path(&manifest, &root, &path)?;
	if to.exists() {
		return Err(Error::DocumentExists);
	}

	// The file moves first. If writing the manifest then fails, the next
	// refresh adopts the renamed file under a new id rather than losing it.
	fs::rename(&from, &to)?;

	match tree::find_mut(&mut manifest.nodes, id) {
		Some(Node::Document { name, .. }) => *name = file,
		_ => return Err(Error::UnknownDocument),
	}
	write_manifest(&root, &mut manifest)?;
	view(&manifest)
}

/// Sets the word target a document is written towards, or clears it with
/// `None`. Nothing on disk changes but the manifest: the target is the writer's
/// intention, not part of the text.
#[tauri::command]
pub fn set_document_target(root: PathBuf, id: Uuid, target: Option<u32>) -> Result<DocumentView> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	// Aiming at no words is the same as not aiming, and saying so here keeps
	// the two apart everywhere above.
	let target = target.filter(|words| *words > 0);

	let mut manifest = read_manifest(&root)?;
	match tree::find_mut(&mut manifest.nodes, id) {
		Some(Node::Document { target: aim, .. }) => *aim = target,
		_ => return Err(Error::UnknownDocument),
	}

	write_manifest(&root, &mut manifest)?;
	manifest
		.documents()
		.iter()
		.find(|d| d.id == id)
		.map(DocumentView::from)
		.ok_or(Error::UnknownDocument)
}

/// One document in the project's trash, as the Trash view shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrashEntry {
	/// Where it is, relative to the trash, in the same `section/file` shape a
	/// document's own path uses.
	pub path: String,
	pub folder: String,
	/// The title it had before it was deleted.
	pub title: String,
	/// When it was deleted, or nothing at all if its name does not carry a
	/// moment Aurora recognises.
	#[serde(with = "time::serde::rfc3339::option")]
	pub deleted: Option<OffsetDateTime>,
}

/// The moment of a deletion, as the prefix on its file in the trash: sortable,
/// and made of nothing a filesystem objects to.
fn stamp(at: OffsetDateTime) -> String {
	format!(
		"{:04}{:02}{:02}-{:02}{:02}{:02}",
		at.year(),
		at.month() as u8,
		at.day(),
		at.hour(),
		at.minute(),
		at.second()
	)
}

/// Moves a document into the project's trash and drops it from the manifest.
/// The file keeps its name behind the moment it was deleted, so deleting two
/// documents called the same thing does not lose the first.
fn trash(root: &Path, id: Uuid, at: OffsetDateTime) -> Result<()> {
	let mut manifest = read_manifest(root)?;
	let from = resolve(&manifest, root, id)?;

	// The trash is laid out in sections, so a document deleted from deeper in
	// the tree lands in the one it belongs to, under its own file name.
	let path = tree::path(&manifest.nodes, id).ok_or(Error::UnknownDocument)?;
	let (section, rest) = path.split_once('/').ok_or(Error::BadDocumentPath)?;
	let file_name = rest.rsplit('/').next().unwrap_or(rest);

	let folder = root.join(TRASH_DIR).join(section);
	fs::create_dir_all(&folder)?;
	// The trash is an ordinary folder a writer can replace with a symlink, so
	// where it actually leads is what decides whether this move stays inside
	// the project.
	if !folder.canonicalize()?.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}

	let at = stamp(at);
	let mut to = folder.join(format!("{at} {file_name}"));
	// Two deletions within the same second would otherwise write over each
	// other.
	let mut again = 1;
	while to.exists() {
		to = folder.join(format!("{at}-{again} {file_name}"));
		again += 1;
	}

	// The file moves first. If writing the manifest then fails, the next
	// refresh drops the document anyway, since its file is no longer in the
	// section.
	fs::rename(&from, &to)?;

	tree::remove(&mut manifest.nodes, id);
	write_manifest(root, &mut manifest)
}

/// Deletes a document, which is to say puts it in the project's trash.
#[tauri::command]
pub fn delete_document(root: PathBuf, id: Uuid) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	trash(&root, id, OffsetDateTime::now_utc())
}

/// Moves a document to a different place in its own section. Ordering lives in
/// the manifest and nowhere else, so this touches no files — and a document
/// whose file has gone can still be moved, since where it sits in the list is
/// not a question about the disk.
fn reorder(manifest: &mut Manifest, id: Uuid, index: usize) -> Result<()> {
	if tree::move_to(&mut manifest.nodes, id, index) {
		Ok(())
	} else {
		Err(Error::UnknownDocument)
	}
}

/// Puts a document at a given place among the others in its section. An index
/// past the end means the end.
#[tauri::command]
pub fn reorder_document(root: PathBuf, id: Uuid, index: usize) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let mut manifest = read_manifest(&root)?;
	reorder(&mut manifest, id, index)?;
	write_manifest(&root, &mut manifest)
}

/// Reads a trash file's name back: the moment it was deleted, and the name it
/// had before that. Anything that is not a stamp Aurora wrote is not one.
fn unstamp(name: &str) -> Option<(OffsetDateTime, &str)> {
	let (token, was) = name.split_once(' ')?;
	let (date, rest) = token.split_once('-')?;
	// A second deletion in the same second carries `-1`, `-2` after the stamp.
	let (time, again) = match rest.split_once('-') {
		Some((time, again)) => (time, Some(again)),
		None => (rest, None),
	};

	fn digits(part: &str, len: usize) -> Option<&str> {
		(part.len() == len && part.bytes().all(|b| b.is_ascii_digit())).then_some(part)
	}
	let date = digits(date, 8)?;
	let time = digits(time, 6)?;
	if again.is_some_and(|n| n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit())) {
		return None;
	}
	let number = |from: usize, to: usize, of: &str| of[from..to].parse::<u32>().ok();

	let at = Date::from_calendar_date(
		number(0, 4, date)? as i32,
		Month::try_from(number(4, 6, date)? as u8).ok()?,
		number(6, 8, date)? as u8,
	)
	.ok()?
	.with_hms(
		number(0, 2, time)? as u8,
		number(2, 4, time)? as u8,
		number(4, 6, time)? as u8,
	)
	.ok()?
	.assume_utc();

	Some((at, was))
}

/// Everything in the project's trash, most recently deleted first.
#[tauri::command]
pub fn list_trash(root: PathBuf) -> Result<Vec<TrashEntry>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	// The trash is laid out in sections exactly as the project is, so the same
	// scan reads it.
	let mut entries: Vec<TrashEntry> =
		tree::documents(&scan(&root.join(TRASH_DIR), &manifest.folders())?)
			.iter()
			.map(|document| {
				let path = &document.path;
				let file = path.split_once('/').map_or(path.as_str(), |(_, file)| file);
				let (deleted, was) = match unstamp(file) {
					Some((at, was)) => (Some(at), was),
					// A file somebody put there by hand is still shown, so it can
					// at least be got rid of.
					None => (None, file),
				};
				TrashEntry {
					path: path.clone(),
					folder: section_of(path).unwrap_or_default().to_owned(),
					title: Path::new(was)
						.file_stem()
						.and_then(|stem| stem.to_str())
						.unwrap_or(was)
						.to_owned(),
					deleted,
				}
			})
			.collect();

	// Newest first, with anything undated behind the rest.
	entries.sort_by(|a, b| b.deleted.cmp(&a.deleted).then_with(|| a.path.cmp(&b.path)));
	Ok(entries)
}

/// A file in the trash, once the project is satisfied it is really in there.
fn trash_entry(manifest: &Manifest, root: &Path, path: &str) -> Result<PathBuf> {
	let file = document_path(manifest, &root.join(TRASH_DIR), path)?;
	if !file.exists() {
		return Err(Error::DocumentMissing);
	}
	// The trash is an ordinary folder a writer can replace with a symlink.
	if !file.canonicalize()?.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}
	Ok(file)
}

/// Puts a deleted document back in the section it came from, under the name it
/// had. A document already using that name is not written over.
#[tauri::command]
pub fn restore_from_trash(root: PathBuf, path: String) -> Result<DocumentView> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	let from = trash_entry(&manifest, &root, &path)?;

	let (section, file) = path.split_once('/').ok_or(Error::BadDocumentPath)?;
	let was = unstamp(file).map_or(file, |(_, was)| was);
	let back = format!("{section}/{was}");

	let to = document_path(&manifest, &root, &back)?;
	if to.exists() {
		return Err(Error::DocumentExists);
	}

	let folder = to.parent().ok_or(Error::BadDocumentPath)?;
	fs::create_dir_all(folder)?;
	if !folder.canonicalize()?.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}

	fs::rename(&from, &to)?;

	refresh(&root)?
		.documents()
		.iter()
		.find(|d| d.path == back)
		.map(DocumentView::from)
		.ok_or(Error::UnknownDocument)
}

/// Throws one document in the trash away for good.
#[tauri::command]
pub fn purge_trash_entry(root: PathBuf, path: String) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	let file = trash_entry(&manifest, &root, &path)?;
	Ok(fs::remove_file(file)?)
}

/// What a folder holds, one card at a time, in the order the folder keeps
/// them. Reads the manifest rather than the folder, the way `list_documents`
/// does.
#[tauri::command]
pub fn folder_overview(root: PathBuf, id: Uuid) -> Result<Vec<ChildSummary>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	// The folder's own path, which is the prefix of everything it holds. Ask
	// for it first: an id that names a document, or nothing at all, is not a
	// folder to look inside.
	let prefix = match tree::find(&manifest.nodes, id) {
		Some(Node::Folder { .. }) => tree::path(&manifest.nodes, id).unwrap_or_default(),
		_ => return Err(Error::UnknownSection),
	};

	Ok(tree::children(&manifest.nodes, id)
		.unwrap_or_default()
		.iter()
		.map(|node| match node {
			Node::Folder {
				id,
				name,
				kind,
				children,
			} => ChildSummary::Folder {
				id: *id,
				name: name.clone(),
				kind: *kind,
				children: children.len(),
			},
			Node::Document { id, name, target } => {
				let document = Document {
					id: *id,
					path: format!("{prefix}/{name}"),
					target: *target,
				};
				ChildSummary::Document(match resolve(&manifest, &root, document.id) {
					Ok(path) => summarise(&document, &path),
					// A vanished file, or one the manifest points outside the
					// project.
					Err(_) => DocumentSummary::blank(&document),
				})
			}
		})
		.collect())
}

/// Every document in the project with its text, in manifest order. One call
/// rather than one per document: the search reads the whole project on the
/// first query, and a round trip per file would be the bulk of that cost.
#[tauri::command]
pub fn read_all_documents(root: PathBuf) -> Result<Vec<DocumentText>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;

	Ok(manifest
		.documents()
		.iter()
		.map(|document| DocumentText {
			document: document.into(),
			// A file that has vanished, is not UTF-8, or that the manifest
			// points outside the project, costs that one document rather than
			// the whole search.
			text: resolve(&manifest, &root, document.id)
				.and_then(|path| String::from_utf8(fs::read(path)?).map_err(|_| Error::NotText))
				.ok(),
		})
		.collect())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::project::{Format, NameError, create};
	use crate::tree::{self, tree_from_flat};
	use time::OffsetDateTime;

	fn fixed_time() -> OffsetDateTime {
		OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
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

	/// The documents a scan found, in the flat shape the older tests read.
	fn scan_paths(root: &Path) -> Vec<String> {
		tree::documents(&scan(root, &novel_folders()).unwrap())
			.into_iter()
			.map(|document| document.path)
			.collect()
	}

	#[test]
	fn scan_finds_the_seed_files_in_section_order() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		assert_eq!(
			scan_paths(&root),
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

		let found = scan_paths(&root);
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

		let found = scan_paths(&root);
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
	fn a_file_inside_a_directory_is_found() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = root.join("Manuscript").join("Part One");
		fs::create_dir(&part).unwrap();
		fs::write(part.join("Chapter 2.md"), "").unwrap();

		assert_eq!(
			scan_paths(&root)
				.iter()
				.filter(|p| p.starts_with("Manuscript/"))
				.collect::<Vec<_>>(),
			[
				"Manuscript/Chapter 1.md",
				"Manuscript/Part One/Chapter 2.md"
			],
			"a directory sorts among the files beside it"
		);
	}

	#[test]
	fn a_file_three_levels_down_is_found() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let chapter = root.join("Manuscript").join("Part One").join("Chapter 3");
		fs::create_dir_all(&chapter).unwrap();
		fs::write(chapter.join("Scene 2.md"), "").unwrap();

		assert!(
			scan_paths(&root).contains(&"Manuscript/Part One/Chapter 3/Scene 2.md".to_owned()),
			"the walk goes as deep as the writer does"
		);
	}

	#[test]
	fn an_empty_directory_is_a_folder_holding_nothing() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::create_dir(root.join("Manuscript").join("Part Two")).unwrap();

		let found = scan(&root, &novel_folders()).unwrap();

		let manuscript = &found[0];
		let Some([_, Node::Folder { name, children, .. }]) =
			tree::children(&found, manuscript.id())
		else {
			panic!("the seed chapter and the new directory, in that order")
		};
		assert_eq!(name, "Part Two");
		assert!(children.is_empty(), "nothing is in it yet");
		assert!(
			!scan_paths(&root).iter().any(|p| p.contains("Part Two")),
			"and it holds no documents to speak of"
		);
	}

	#[test]
	fn a_hidden_directory_is_passed_over() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let hidden = root.join("Manuscript").join(".drafts");
		fs::create_dir(&hidden).unwrap();
		fs::write(hidden.join("Chapter 1.md"), "").unwrap();

		assert!(!scan_paths(&root).iter().any(|p| p.contains(".drafts")));
	}

	#[test]
	fn an_uppercase_extension_is_still_markdown() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Notes").join("Ideas.MD"), "").unwrap();

		assert!(scan_paths(&root).contains(&"Notes/Ideas.MD".to_owned()));
	}

	#[test]
	fn a_missing_section_is_skipped_rather_than_failing() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::remove_dir_all(root.join("Outline")).unwrap();

		let found = scan(&root, &novel_folders()).unwrap();

		assert!(
			!found.iter().any(|node| node.name() == "Outline"),
			"a section that is not there is not a folder either"
		);
		assert_eq!(scan_paths(&root).len(), 4);
	}

	#[test]
	fn scan_looks_only_where_it_is_told() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::create_dir(root.join("Scraps")).unwrap();
		fs::write(root.join("Scraps").join("Offcut.md"), "").unwrap();

		assert!(!scan_paths(&root).iter().any(|p| p.starts_with("Scraps/")));
	}

	fn flat_documents(paths: &[&str]) -> Vec<Document> {
		paths
			.iter()
			.map(|path| Document {
				id: Uuid::new_v4(),
				path: (*path).to_owned(),
				target: None,
			})
			.collect()
	}

	fn manifest_with(paths: &[&str]) -> Manifest {
		let mut manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());
		manifest.nodes = tree_from_flat(&novel_folders(), &flat_documents(paths));
		manifest
	}

	fn paths_of(manifest: &Manifest) -> Vec<String> {
		manifest
			.documents()
			.into_iter()
			.map(|document| document.path)
			.collect()
	}

	/// What `scan` would report for these paths: a tree, holding only what
	/// sits under one of the project's sections.
	fn found(paths: &[&str]) -> Vec<Node> {
		let mut nodes = tree_from_flat(&novel_folders(), &flat_documents(paths));
		nodes.retain(|node| novel_folders().iter().any(|folder| folder == node.name()));
		nodes
	}

	#[test]
	fn reconcile_leaves_a_manifest_that_already_agrees_alone() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 1.md", "Notes/Notes.md"]);
		let before = manifest.documents();

		assert!(!reconcile(
			&mut manifest,
			&found(&["Manuscript/Chapter 1.md", "Notes/Notes.md"])
		));
		assert_eq!(manifest.documents(), before);
	}

	#[test]
	fn a_new_file_is_adopted_at_the_end_of_its_section() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 1.md", "Notes/Notes.md"]);
		let chapter_one = manifest.documents()[0].id;

		assert!(reconcile(
			&mut manifest,
			&found(&[
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
			manifest.documents()[0].id,
			chapter_one,
			"an existing document keeps its id"
		);
	}

	#[test]
	fn a_vanished_file_is_dropped() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 1.md", "Notes/Notes.md"]);

		assert!(reconcile(&mut manifest, &found(&["Notes/Notes.md"])));
		assert_eq!(paths_of(&manifest), ["Notes/Notes.md"]);
	}

	#[test]
	fn the_recorded_order_wins_over_the_order_on_disk() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 2.md", "Manuscript/Chapter 1.md"]);

		assert!(!reconcile(
			&mut manifest,
			&found(&["Manuscript/Chapter 1.md", "Manuscript/Chapter 2.md"])
		));
		assert_eq!(
			paths_of(&manifest),
			["Manuscript/Chapter 2.md", "Manuscript/Chapter 1.md"]
		);
	}

	#[test]
	fn a_manifest_without_documents_adopts_everything() {
		let mut manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());

		assert!(reconcile(
			&mut manifest,
			&found(&["Manuscript/Chapter 1.md", "Notes/Notes.md"])
		));
		assert_eq!(
			paths_of(&manifest),
			["Manuscript/Chapter 1.md", "Notes/Notes.md"]
		);
		let ids: HashSet<_> = manifest.documents().iter().map(|d| d.id).collect();
		assert_eq!(ids.len(), 2, "each adopted file gets its own id");
	}

	#[test]
	fn a_path_recorded_twice_is_collapsed() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 1.md", "Manuscript/Chapter 1.md"]);
		let first = manifest.documents()[0].id;

		assert!(reconcile(
			&mut manifest,
			&found(&["Manuscript/Chapter 1.md"])
		));
		assert_eq!(paths_of(&manifest), ["Manuscript/Chapter 1.md"]);
		assert_eq!(manifest.documents()[0].id, first, "the first id wins");
	}

	#[test]
	fn a_document_outside_the_projects_sections_is_dropped() {
		let mut manifest = manifest_with(&["Scraps/Offcut.md", "Notes/Notes.md"]);

		assert!(reconcile(
			&mut manifest,
			&found(&["Scraps/Offcut.md", "Notes/Notes.md"])
		));
		assert_eq!(paths_of(&manifest), ["Notes/Notes.md"]);
	}

	#[test]
	fn a_section_whose_folder_has_gone_keeps_its_place_and_loses_what_was_in_it() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 1.md", "Notes/Notes.md"]);
		let mut on_disk = found(&["Notes/Notes.md"]);
		on_disk.retain(|node| node.name() != "Manuscript");

		assert!(reconcile(&mut manifest, &on_disk));

		assert_eq!(
			manifest.folders(),
			["Manuscript", "Outline", "Characters", "Locations", "Notes"],
			"the project's sections are its own, on disk or not"
		);
		assert_eq!(paths_of(&manifest), ["Notes/Notes.md"]);
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
		assert_eq!(listed[0].documents[0].folder, "Manuscript");
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
			refresh_documents(relative.clone()).unwrap_err(),
			Error::RelativePath
		));
		assert!(matches!(
			document_tree(relative).unwrap_err(),
			Error::RelativePath
		));
	}

	#[test]
	fn the_tree_keeps_its_shape_and_spells_out_every_path() {
		let manifest = manifest_with(&["Manuscript/Part One/Chapter 1.md", "Notes/Notes.md"]);
		let tree = views(&manifest.nodes, "");

		let NodeView::Folder { name, children, .. } = &tree[0] else {
			panic!("the top level is the project's sections");
		};
		assert_eq!(name, "Manuscript");

		let NodeView::Folder {
			name,
			kind,
			children,
			..
		} = &children[0]
		else {
			panic!("a folder inside a section is still a folder");
		};
		assert_eq!(name, "Part One");
		assert_eq!(
			*kind, None,
			"a folder read from a flat manifest has no kind"
		);

		let NodeView::Document(chapter) = &children[0] else {
			panic!("the chapter sits inside the part");
		};
		assert_eq!(chapter.path, "Manuscript/Part One/Chapter 1.md");
		assert_eq!(chapter.title, "Chapter 1");
		assert_eq!(
			chapter.folder, "Manuscript",
			"a document's section is the top of its path, however deep it sits"
		);
	}

	#[test]
	fn a_node_says_which_kind_it_is_on_the_wire() {
		let manifest = manifest_with(&["Notes/Notes.md"]);
		let json = serde_json::to_value(views(&manifest.nodes, "")).unwrap();

		let notes = &json.as_array().unwrap()[4];
		assert_eq!(notes["node"], "folder");
		assert_eq!(notes["kind"], serde_json::Value::Null);
		assert_eq!(notes["children"][0]["node"], "document");
		assert_eq!(notes["children"][0]["title"], "Notes");
	}

	#[test]
	fn a_title_is_the_file_name_without_its_extension() {
		let document = Document::new("Manuscript", "Chapter 1.md");
		assert_eq!(document.title(), "Chapter 1");
		let view = DocumentView::from(&document);
		assert_eq!(view.title, "Chapter 1");
		assert_eq!(view.folder, "Manuscript");
	}

	/// Rewrites one document's recorded path, the way a hand-edited manifest
	/// could. The node is lifted to the top of the tree, where its whole path
	/// is its name: a name under a folder cannot say `..`.
	fn set_document_path(root: &Path, index: usize, path: &str) {
		let mut manifest = read_manifest(root).unwrap();
		let id = manifest.documents()[index].id;
		let Some(Node::Document { target, .. }) = tree::remove(&mut manifest.nodes, id) else {
			panic!("no document at {index}")
		};

		manifest.nodes.insert(
			0,
			Node::Document {
				id,
				name: path.to_owned(),
				target,
			},
		);
		write_manifest(root, &mut manifest).unwrap();
	}

	fn first_document(root: &Path) -> Document {
		read_manifest(root).unwrap().documents()[0].clone()
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

	#[test]
	fn what_is_written_is_what_is_read_back() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		write_document(root.clone(), id, "Sing to me of the man, Muse.".to_owned()).unwrap();
		assert_eq!(
			read_document(root, id).unwrap(),
			"Sing to me of the man, Muse."
		);
	}

	#[test]
	fn a_second_write_replaces_the_first_rather_than_adding_to_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		write_document(root.clone(), id, "a long first draft".to_owned()).unwrap();
		write_document(root.clone(), id, "short".to_owned()).unwrap();
		assert_eq!(read_document(root, id).unwrap(), "short");
	}

	#[test]
	fn writing_leaves_no_temporary_file_behind() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		write_document(root.clone(), id, "Chapter one.".to_owned()).unwrap();

		let left: Vec<_> = fs::read_dir(root.join("Manuscript"))
			.unwrap()
			.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
			.collect();
		assert_eq!(left, ["Chapter 1.md"]);
		assert_eq!(scan_paths(&root).len(), 5);
	}

	#[test]
	fn writing_to_an_unknown_id_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = write_document(root, Uuid::new_v4(), "lost".to_owned()).unwrap_err();
		assert!(matches!(err, Error::UnknownDocument));
	}

	#[test]
	fn writing_to_a_document_whose_file_has_gone_is_reported() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;
		fs::remove_file(root.join("Manuscript").join("Chapter 1.md")).unwrap();

		let err = write_document(root, id, "back again".to_owned()).unwrap_err();
		assert!(matches!(err, Error::DocumentMissing));
	}

	#[test]
	fn writing_outside_the_project_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let outside = parent.path().join("secrets.md");
		fs::write(&outside, "not yours").unwrap();
		set_document_path(&root, 0, "../secrets.md");

		let id = first_document(&root).id;
		let err = write_document(root, id, "overwritten".to_owned()).unwrap_err();
		assert!(matches!(err, Error::OutsideProject));
		assert_eq!(fs::read_to_string(&outside).unwrap(), "not yours");
	}

	/// A project whose first document has been deleted from disk.
	fn with_chapter_one_gone(parent: &tempfile::TempDir) -> (PathBuf, Uuid) {
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;
		fs::remove_file(root.join("Manuscript").join("Chapter 1.md")).unwrap();
		(root, id)
	}

	#[test]
	fn a_vanished_document_can_be_put_back() {
		let parent = tempfile::tempdir().unwrap();
		let (root, id) = with_chapter_one_gone(&parent);

		let restored = restore_document(
			root.clone(),
			"Manuscript/Chapter 1.md".to_owned(),
			"Sing to me of the man, Muse.".to_owned(),
		)
		.unwrap();

		assert_eq!(restored.id, id, "the manifest still knew the path");
		assert_eq!(restored.title, "Chapter 1");
		assert_eq!(
			read_document(root, restored.id).unwrap(),
			"Sing to me of the man, Muse."
		);
	}

	#[test]
	fn a_document_already_forgotten_comes_back_under_a_new_id() {
		let parent = tempfile::tempdir().unwrap();
		let (root, id) = with_chapter_one_gone(&parent);
		// A refresh while the file was away drops it from the manifest, which
		// is why the path has to come from the caller.
		refresh(&root).unwrap();

		let restored = restore_document(
			root.clone(),
			"Manuscript/Chapter 1.md".to_owned(),
			"Back again.".to_owned(),
		)
		.unwrap();

		assert_ne!(restored.id, id);
		assert_eq!(read_document(root, restored.id).unwrap(), "Back again.");
	}

	#[test]
	fn restoring_recreates_a_section_folder_that_went_with_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::remove_dir_all(root.join("Manuscript")).unwrap();

		let restored = restore_document(
			root.clone(),
			"Manuscript/Chapter 1.md".to_owned(),
			"Back again.".to_owned(),
		)
		.unwrap();

		assert_eq!(restored.folder, "Manuscript");
		assert_eq!(read_document(root, restored.id).unwrap(), "Back again.");
	}

	#[test]
	fn a_document_that_came_back_on_its_own_is_not_written_over() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(
			root.join("Manuscript").join("Chapter 1.md"),
			"what the sync client brought back",
		)
		.unwrap();

		let err = restore_document(
			root.clone(),
			"Manuscript/Chapter 1.md".to_owned(),
			"my copy".to_owned(),
		)
		.unwrap_err();

		assert!(matches!(err, Error::DocumentExists));
		assert_eq!(
			fs::read_to_string(root.join("Manuscript").join("Chapter 1.md")).unwrap(),
			"what the sync client brought back"
		);
	}

	#[test]
	fn restoring_refuses_a_path_that_is_not_a_document_in_a_section() {
		let parent = tempfile::tempdir().unwrap();
		let (root, _) = with_chapter_one_gone(&parent);

		for path in [
			"secrets.md",
			"../secrets.md",
			"Manuscript/../../secrets.md",
			"Scraps/Offcut.md",
			"Manuscript/notes.txt",
			"Manuscript/",
			"Manuscript/..",
		] {
			let err =
				restore_document(root.clone(), path.to_owned(), "text".to_owned()).unwrap_err();
			assert!(
				matches!(err, Error::BadDocumentPath),
				"{path} should not be a document path"
			);
		}

		assert!(!parent.path().join("secrets.md").exists());
	}

	#[cfg(unix)]
	#[test]
	fn restoring_through_a_symlinked_section_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let elsewhere = parent.path().join("elsewhere");
		fs::create_dir(&elsewhere).unwrap();
		fs::remove_dir_all(root.join("Notes")).unwrap();
		std::os::unix::fs::symlink(&elsewhere, root.join("Notes")).unwrap();

		let err =
			restore_document(root, "Notes/Notes.md".to_owned(), "text".to_owned()).unwrap_err();

		assert!(matches!(err, Error::OutsideProject));
		assert!(!elsewhere.join("Notes.md").exists());
	}

	#[test]
	fn restoring_refuses_a_relative_path() {
		let err = restore_document(
			PathBuf::from("some/where"),
			"Notes/Notes.md".to_owned(),
			String::new(),
		)
		.unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	/// The id of one of the project's sections, which is what an overview takes
	/// now that it is a folder like any other.
	fn section_id(root: &Path, name: &str) -> Uuid {
		read_manifest(root)
			.unwrap()
			.nodes
			.iter()
			.find(|node| node.name() == name)
			.expect("the project has that section")
			.id()
	}

	/// A section's overview as the document cards it used to be, which is all
	/// a project with no folders in it can hold.
	fn cards(root: PathBuf, section: &str) -> Vec<DocumentSummary> {
		let id = section_id(&root, section);
		folder_overview(root, id)
			.unwrap()
			.into_iter()
			.map(|child| match child {
				ChildSummary::Document(card) => card,
				ChildSummary::Folder { name, .. } => panic!("{name} is a folder, not a card"),
			})
			.collect()
	}

	#[test]
	fn an_overview_carries_the_opening_the_count_and_the_time() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(
			root.join("Manuscript").join("Chapter 1.md"),
			"Sing to me of the man, Muse.",
		)
		.unwrap();

		let overview = cards(root, "Manuscript");

		assert_eq!(overview.len(), 1);
		assert_eq!(overview[0].document.title, "Chapter 1");
		assert_eq!(overview[0].excerpt, "Sing to me of the man, Muse.");
		assert_eq!(overview[0].words, 7);
		assert!(overview[0].modified.is_some());
	}

	#[test]
	fn an_overview_lists_a_section_in_the_manifests_order() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		for name in ["Chapter 3.md", "Chapter 2.md"] {
			fs::write(root.join("Manuscript").join(name), "").unwrap();
		}
		refresh(&root).unwrap();

		let overview = cards(root, "Manuscript");

		let titles: Vec<_> = overview.iter().map(|d| d.document.title.as_str()).collect();
		assert_eq!(titles, ["Chapter 1", "Chapter 2", "Chapter 3"]);
	}

	#[test]
	fn an_overview_holds_only_its_own_section() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let overview = cards(root, "Notes");

		assert_eq!(overview.len(), 1);
		assert_eq!(overview[0].document.folder, "Notes");
	}

	#[test]
	fn a_document_whose_file_has_gone_still_gets_a_card() {
		let parent = tempfile::tempdir().unwrap();
		let (root, id) = with_chapter_one_gone(&parent);

		let overview = cards(root, "Manuscript");

		assert_eq!(overview.len(), 1);
		assert_eq!(overview[0].document.id, id);
		assert_eq!(overview[0].words, 0);
		assert_eq!(overview[0].excerpt, "");
		assert!(overview[0].modified.is_none());
	}

	#[test]
	fn an_overview_does_not_read_a_document_outside_the_project() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(parent.path().join("secrets.md"), "not yours").unwrap();
		set_document_path(&root, 0, "../secrets.md");

		let overview = cards(root, "Manuscript");

		assert!(overview.is_empty(), "it is no longer in Manuscript");
	}

	#[test]
	fn an_overview_of_something_the_project_does_not_have_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = folder_overview(root, Uuid::new_v4()).unwrap_err();
		assert!(matches!(err, Error::UnknownSection));
	}

	#[test]
	fn an_overview_of_a_document_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let chapter = first_document(&root).id;

		let err = folder_overview(root, chapter).unwrap_err();
		assert!(matches!(err, Error::UnknownSection));
	}

	#[test]
	fn an_overview_refuses_a_relative_path() {
		let err = folder_overview(PathBuf::from("some/where"), Uuid::new_v4()).unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn an_overview_interleaves_folders_with_documents_and_says_what_they_hold() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = root.join("Manuscript").join("Part One");
		fs::create_dir(&part).unwrap();
		fs::write(part.join("Chapter 2.md"), "").unwrap();
		fs::write(part.join("Chapter 3.md"), "").unwrap();
		refresh(&root).unwrap();

		let id = section_id(&root, "Manuscript");
		let overview = folder_overview(root, id).unwrap();

		assert_eq!(overview.len(), 2);
		assert!(
			matches!(&overview[0], ChildSummary::Document(card) if card.document.title == "Chapter 1"),
			"the seed chapter keeps its place ahead of the new folder"
		);
		let ChildSummary::Folder {
			name,
			kind,
			children,
			..
		} = &overview[1]
		else {
			panic!("Part One is a folder");
		};
		assert_eq!(name, "Part One");
		assert_eq!(*kind, None, "a folder found on disk has no kind yet");
		assert_eq!(*children, 2);
	}

	#[test]
	fn an_overview_looks_only_at_what_a_folder_holds_directly() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = root.join("Manuscript").join("Part One");
		fs::create_dir(&part).unwrap();
		fs::write(part.join("Chapter 2.md"), "Down to the sea.").unwrap();
		refresh(&root).unwrap();

		let manifest = read_manifest(&root).unwrap();
		let id = tree::find(&manifest.nodes, section_id(&root, "Manuscript"))
			.and_then(|section| match section {
				Node::Folder { children, .. } => children.last().map(|node| node.id()),
				Node::Document { .. } => None,
			})
			.unwrap();
		let overview = folder_overview(root, id).unwrap();

		let ChildSummary::Document(card) = &overview[0] else {
			panic!("the part holds one chapter");
		};
		assert_eq!(card.document.path, "Manuscript/Part One/Chapter 2.md");
		assert_eq!(card.words, 4, "a card is read from the file, however deep");
	}

	#[test]
	fn a_summary_serializes_flat_alongside_the_document() {
		let document = Document::new("Manuscript", "Chapter 1.md");
		let summary = DocumentSummary {
			document: (&document).into(),
			words: 3,
			excerpt: "Sing to me".to_owned(),
			modified: Some(fixed_time()),
		};

		let json = serde_json::to_value(&summary).unwrap();
		assert_eq!(json["title"], "Chapter 1");
		assert_eq!(json["path"], "Manuscript/Chapter 1.md");
		assert_eq!(json["words"], 3);
		assert_eq!(json["modified"], "2023-11-14T22:13:20Z");
		assert!(json.get("document").is_none());
	}

	#[test]
	fn a_card_says_which_kind_it_is_on_the_wire() {
		let document = Document::new("Manuscript", "Chapter 1.md");
		let json = serde_json::to_value(ChildSummary::Document(DocumentSummary::blank(&document)))
			.unwrap();
		assert_eq!(json["node"], "document");
		assert_eq!(json["title"], "Chapter 1", "still flat under the tag");

		let json = serde_json::to_value(ChildSummary::Folder {
			id: Uuid::new_v4(),
			name: "Part One".to_owned(),
			kind: Some(FolderKind::Part),
			children: 2,
		})
		.unwrap();
		assert_eq!(json["node"], "folder");
		assert_eq!(json["kind"], "part");
		assert_eq!(json["children"], 2);
	}

	#[test]
	fn an_unreadable_summary_reports_no_time_at_all() {
		let document = Document::new("Manuscript", "Chapter 1.md");
		let json = serde_json::to_value(DocumentSummary::blank(&document)).unwrap();
		assert!(json["modified"].is_null());
	}

	#[test]
	fn an_excerpt_is_one_line_of_the_opening() {
		assert_eq!(
			excerpt("  Sing to me\n\nof the man,\tMuse.  "),
			"Sing to me of the man, Muse."
		);
		assert_eq!(excerpt(""), "");
	}

	#[test]
	fn a_long_excerpt_is_cut_on_a_word_boundary() {
		let text = "word ".repeat(200);
		let cut = excerpt(&text);

		assert!(cut.ends_with('\u{2026}'));
		assert!(cut.chars().count() <= EXCERPT_CHARS + 1);
		assert!(cut.trim_end_matches('\u{2026}').ends_with("word"));
	}

	#[test]
	fn a_single_endless_word_is_still_shown() {
		let cut = excerpt(&"a".repeat(EXCERPT_CHARS * 2));

		assert_eq!(cut.chars().count(), EXCERPT_CHARS + 1);
		assert!(cut.ends_with('\u{2026}'));
	}

	#[test]
	fn words_are_counted_across_lines() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(
			root.join("Notes").join("Notes.md"),
			"one two\nthree\n\n  four  ",
		)
		.unwrap();

		let overview = cards(root, "Notes");
		assert_eq!(overview[0].words, 4);
	}

	#[test]
	fn a_new_document_starts_empty_at_the_end_of_its_section() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let created = create_document(
			root.clone(),
			"Manuscript".to_owned(),
			"Chapter 2".to_owned(),
		)
		.unwrap();

		assert_eq!(created.title, "Chapter 2");
		assert_eq!(created.folder, "Manuscript");
		assert_eq!(created.path, "Manuscript/Chapter 2.md");
		assert_eq!(read_document(root.clone(), created.id).unwrap(), "");

		let manuscript = sections(&read_manifest(&root).unwrap()).remove(0);
		let titles: Vec<_> = manuscript.documents.iter().map(|d| &d.title).collect();
		assert_eq!(titles, ["Chapter 1", "Chapter 2"]);
	}

	#[test]
	fn a_new_document_can_be_written_to_straight_away() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let created =
			create_document(root.clone(), "Notes".to_owned(), "Ideas".to_owned()).unwrap();
		write_document(root.clone(), created.id, "Begin in the middle.".to_owned()).unwrap();

		assert_eq!(
			read_document(root, created.id).unwrap(),
			"Begin in the middle."
		);
	}

	#[test]
	fn a_name_already_taken_in_that_section_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(
			root.join("Manuscript").join("Chapter 1.md"),
			"Sing to me of the man, Muse.",
		)
		.unwrap();

		let err = create_document(
			root.clone(),
			"Manuscript".to_owned(),
			"Chapter 1".to_owned(),
		)
		.unwrap_err();

		assert!(matches!(err, Error::DocumentExists));
		assert_eq!(
			fs::read_to_string(root.join("Manuscript").join("Chapter 1.md")).unwrap(),
			"Sing to me of the man, Muse.",
			"the document that was already there is untouched"
		);
	}

	#[test]
	fn the_same_name_in_another_section_is_fine() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		create_document(root.clone(), "Notes".to_owned(), "Chapter 1".to_owned()).unwrap();

		assert!(root.join("Notes").join("Chapter 1.md").exists());
	}

	#[test]
	fn a_name_the_filesystem_would_not_take_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		for name in [
			"",
			"  ",
			"Chapter/Two",
			"Chapter?",
			".hidden",
			"NUL",
			"Chapter ",
		] {
			let err = create_document(root.clone(), "Manuscript".to_owned(), name.to_owned())
				.unwrap_err();
			assert!(
				matches!(err, Error::InvalidName(_)),
				"{name:?} should not be a document name"
			);
		}

		assert_eq!(scan_paths(&root).len(), 5);
	}

	#[test]
	fn creating_in_a_section_the_project_does_not_have_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err =
			create_document(root.clone(), "Scraps".to_owned(), "Offcut".to_owned()).unwrap_err();

		assert!(matches!(err, Error::UnknownSection));
		assert!(!root.join("Scraps").exists());
	}

	#[test]
	fn creating_recreates_a_section_folder_that_has_gone() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::remove_dir_all(root.join("Manuscript")).unwrap();

		let created = create_document(
			root.clone(),
			"Manuscript".to_owned(),
			"Chapter 2".to_owned(),
		)
		.unwrap();

		assert_eq!(created.path, "Manuscript/Chapter 2.md");
		assert_eq!(read_document(root, created.id).unwrap(), "");
	}

	#[cfg(unix)]
	#[test]
	fn creating_through_a_symlinked_section_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let elsewhere = parent.path().join("elsewhere");
		fs::create_dir(&elsewhere).unwrap();
		fs::remove_dir_all(root.join("Notes")).unwrap();
		std::os::unix::fs::symlink(&elsewhere, root.join("Notes")).unwrap();

		let err = create_document(root, "Notes".to_owned(), "Ideas".to_owned()).unwrap_err();

		assert!(matches!(err, Error::OutsideProject));
		assert!(!elsewhere.join("Ideas.md").exists());
	}

	#[test]
	fn creating_refuses_a_relative_path() {
		let err = create_document(
			PathBuf::from("some/where"),
			"Notes".to_owned(),
			"Ideas".to_owned(),
		)
		.unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn a_name_that_already_carries_the_extension_does_not_double_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		for (typed, title) in [
			("Chapter 2.md", "Chapter 2"),
			("Chapter 3.MD", "Chapter 3"),
			("Chapter 4.markdown", "Chapter 4.markdown"),
			("Chapter 5", "Chapter 5"),
		] {
			let created =
				create_document(root.clone(), "Manuscript".to_owned(), typed.to_owned()).unwrap();
			assert_eq!(created.title, title, "typed {typed:?}");
			assert_eq!(created.path, format!("Manuscript/{title}.md"));
		}
	}

	#[test]
	fn a_name_that_is_nothing_but_the_extension_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = create_document(root.clone(), "Notes".to_owned(), ".md".to_owned()).unwrap_err();

		assert!(matches!(err, Error::InvalidName(NameError::Empty)));
		assert_eq!(scan_paths(&root).len(), 5);
	}

	#[test]
	fn a_renamed_document_keeps_its_id_its_text_and_its_place() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		create_document(
			root.clone(),
			"Manuscript".to_owned(),
			"Chapter 2".to_owned(),
		)
		.unwrap();
		let id = first_document(&root).id;
		write_document(root.clone(), id, "Sing to me of the man, Muse.".to_owned()).unwrap();

		let renamed = rename_document(root.clone(), id, "Ithaca Falls".to_owned()).unwrap();

		assert_eq!(renamed.id, id);
		assert_eq!(renamed.title, "Ithaca Falls");
		assert_eq!(renamed.folder, "Manuscript");
		assert_eq!(renamed.path, "Manuscript/Ithaca Falls.md");
		assert_eq!(
			read_document(root.clone(), id).unwrap(),
			"Sing to me of the man, Muse."
		);

		let manuscript = sections(&read_manifest(&root).unwrap()).remove(0);
		let titles: Vec<_> = manuscript.documents.iter().map(|d| &d.title).collect();
		assert_eq!(titles, ["Ithaca Falls", "Chapter 2"], "it did not move");
	}

	#[test]
	fn renaming_takes_the_old_file_with_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		rename_document(root.clone(), id, "Ithaca Falls".to_owned()).unwrap();

		assert!(!root.join("Manuscript").join("Chapter 1.md").exists());
		assert_eq!(scan_paths(&root)[0], "Manuscript/Ithaca Falls.md");
	}

	#[test]
	fn renaming_to_the_name_it_already_has_changes_nothing() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		let renamed = rename_document(root.clone(), id, "Chapter 1".to_owned()).unwrap();

		assert_eq!(renamed.id, id);
		assert_eq!(renamed.path, "Manuscript/Chapter 1.md");
		assert!(root.join("Manuscript").join("Chapter 1.md").exists());
	}

	#[test]
	fn renaming_onto_another_document_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let other = create_document(
			root.clone(),
			"Manuscript".to_owned(),
			"Chapter 2".to_owned(),
		)
		.unwrap();
		write_document(root.clone(), other.id, "the second chapter".to_owned()).unwrap();
		let id = first_document(&root).id;

		let err = rename_document(root.clone(), id, "Chapter 2".to_owned()).unwrap_err();

		assert!(matches!(err, Error::DocumentExists));
		assert_eq!(
			read_document(root.clone(), other.id).unwrap(),
			"the second chapter",
			"the document that was already there is untouched"
		);
		assert_eq!(first_document(&root).path, "Manuscript/Chapter 1.md");
	}

	#[test]
	fn a_name_in_another_section_is_not_a_collision() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		let renamed = rename_document(root.clone(), id, "Notes".to_owned()).unwrap();

		assert_eq!(renamed.path, "Manuscript/Notes.md");
		assert!(root.join("Notes").join("Notes.md").exists());
	}

	#[test]
	fn renaming_drops_an_extension_the_writer_typed() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		let renamed = rename_document(root, id, "Ithaca Falls.md".to_owned()).unwrap();

		assert_eq!(renamed.title, "Ithaca Falls");
		assert_eq!(renamed.path, "Manuscript/Ithaca Falls.md");
	}

	#[test]
	fn renaming_to_a_name_the_filesystem_would_not_take_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		for name in ["", "  ", "Chapter/Two", "Chapter?", ".hidden", "NUL"] {
			let err = rename_document(root.clone(), id, name.to_owned()).unwrap_err();
			assert!(
				matches!(err, Error::InvalidName(_)),
				"{name:?} should not be a document name"
			);
		}

		assert_eq!(first_document(&root).path, "Manuscript/Chapter 1.md");
	}

	#[test]
	fn renaming_an_unknown_document_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = rename_document(root, Uuid::new_v4(), "Ithaca Falls".to_owned()).unwrap_err();
		assert!(matches!(err, Error::UnknownDocument));
	}

	#[test]
	fn renaming_a_document_whose_file_has_gone_is_reported() {
		let parent = tempfile::tempdir().unwrap();
		let (root, id) = with_chapter_one_gone(&parent);

		let err = rename_document(root, id, "Ithaca Falls".to_owned()).unwrap_err();
		assert!(matches!(err, Error::DocumentMissing));
	}

	#[test]
	fn renaming_refuses_a_relative_path() {
		let err = rename_document(
			PathBuf::from("some/where"),
			Uuid::new_v4(),
			"Ithaca Falls".to_owned(),
		)
		.unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn a_target_is_kept_and_read_back() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		let view = set_document_target(root.clone(), id, Some(1_500)).unwrap();

		assert_eq!(view.target, Some(1_500));
		assert_eq!(first_document(&root).target, Some(1_500));
		let listed = list_documents(root.clone()).unwrap();
		assert_eq!(listed[0].documents[0].target, Some(1_500));
		let overview = cards(root, "Manuscript");
		assert_eq!(overview[0].document.target, Some(1_500));
	}

	#[test]
	fn a_target_is_cleared_by_none_and_by_zero() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		set_document_target(root.clone(), id, Some(1_500)).unwrap();
		assert_eq!(
			set_document_target(root.clone(), id, None).unwrap().target,
			None
		);

		set_document_target(root.clone(), id, Some(1_500)).unwrap();
		assert_eq!(
			set_document_target(root.clone(), id, Some(0))
				.unwrap()
				.target,
			None
		);
		assert_eq!(first_document(&root).target, None);
	}

	#[test]
	fn a_document_without_a_target_writes_no_such_field() {
		let document = Document::new("Manuscript", "Chapter 1.md");
		let json = serde_json::to_value(&document).unwrap();
		assert!(json.get("target").is_none());
	}

	#[test]
	fn a_manifest_written_before_targets_still_loads() {
		let json = serde_json::json!({
			"id": Uuid::new_v4().to_string(),
			"path": "Manuscript/Chapter 1.md",
		});
		let document: Document = serde_json::from_value(json).unwrap();
		assert_eq!(document.target, None);
	}

	#[test]
	fn a_target_survives_a_rename() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;
		set_document_target(root.clone(), id, Some(900)).unwrap();

		let renamed = rename_document(root, id, "Ithaca Falls".to_owned()).unwrap();

		assert_eq!(renamed.target, Some(900));
	}

	#[test]
	fn targeting_an_unknown_document_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = set_document_target(root, Uuid::new_v4(), Some(1_500)).unwrap_err();

		assert!(matches!(err, Error::UnknownDocument));
	}

	#[test]
	fn targeting_refuses_a_relative_path() {
		let err = set_document_target(PathBuf::from("some/where"), Uuid::new_v4(), Some(1_500))
			.unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	/// What is in the trash, as file names relative to `.trash/<section>`.
	fn trashed(root: &Path, section: &str) -> Vec<String> {
		let mut names: Vec<String> = fs::read_dir(root.join(TRASH_DIR).join(section))
			.unwrap()
			.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
			.collect();
		names.sort();
		names
	}

	#[test]
	fn a_deleted_document_keeps_its_text_in_the_trash() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;
		write_document(root.clone(), id, "Sing to me of the man, Muse.".to_owned()).unwrap();

		trash(&root, id, fixed_time()).unwrap();

		assert_eq!(
			trashed(&root, "Manuscript"),
			["20231114-221320 Chapter 1.md"]
		);
		assert_eq!(
			fs::read_to_string(
				root.join(TRASH_DIR)
					.join("Manuscript")
					.join("20231114-221320 Chapter 1.md")
			)
			.unwrap(),
			"Sing to me of the man, Muse."
		);
	}

	#[test]
	fn a_deleted_document_leaves_its_section_and_the_manifest() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		trash(&root, id, fixed_time()).unwrap();

		assert!(!root.join("Manuscript").join("Chapter 1.md").exists());
		assert!(
			!read_manifest(&root)
				.unwrap()
				.documents()
				.iter()
				.any(|d| d.id == id)
		);
		let manuscript = sections(&read_manifest(&root).unwrap()).remove(0);
		assert!(manuscript.documents.is_empty());
	}

	#[test]
	fn the_trash_is_invisible_to_a_refresh() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		trash(&root, id, fixed_time()).unwrap();
		refresh(&root).unwrap();

		assert_eq!(scan_paths(&root).len(), 4);
		assert_eq!(read_manifest(&root).unwrap().documents().len(), 4);
		assert!(trashed(&root, "Manuscript").len() == 1, "it is still there");
	}

	#[test]
	fn deleting_two_documents_of_the_same_name_keeps_both() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		for text in ["the first one", "the second one"] {
			let made =
				create_document(root.clone(), "Notes".to_owned(), "Ideas".to_owned()).unwrap();
			write_document(root.clone(), made.id, text.to_owned()).unwrap();
			// The same instant both times, which is what forces the collision.
			trash(&root, made.id, fixed_time()).unwrap();
		}

		assert_eq!(
			trashed(&root, "Notes"),
			["20231114-221320 Ideas.md", "20231114-221320-1 Ideas.md"]
		);
	}

	#[test]
	fn documents_deleted_from_different_sections_do_not_meet() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let notes =
			create_document(root.clone(), "Notes".to_owned(), "Chapter 1".to_owned()).unwrap();
		let manuscript = first_document(&root).id;

		trash(&root, manuscript, fixed_time()).unwrap();
		trash(&root, notes.id, fixed_time()).unwrap();

		assert_eq!(
			trashed(&root, "Manuscript"),
			["20231114-221320 Chapter 1.md"]
		);
		assert_eq!(trashed(&root, "Notes"), ["20231114-221320 Chapter 1.md"]);
	}

	#[test]
	fn deleting_an_unknown_document_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = delete_document(root.clone(), Uuid::new_v4()).unwrap_err();

		assert!(matches!(err, Error::UnknownDocument));
		assert!(!root.join(TRASH_DIR).exists());
	}

	#[test]
	fn deleting_a_document_whose_file_has_gone_is_reported() {
		let parent = tempfile::tempdir().unwrap();
		let (root, id) = with_chapter_one_gone(&parent);

		let err = delete_document(root, id).unwrap_err();
		assert!(matches!(err, Error::DocumentMissing));
	}

	#[cfg(unix)]
	#[test]
	fn a_trash_that_leads_out_of_the_project_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let elsewhere = parent.path().join("elsewhere");
		fs::create_dir(&elsewhere).unwrap();
		std::os::unix::fs::symlink(&elsewhere, root.join(TRASH_DIR)).unwrap();
		let id = first_document(&root).id;

		let err = trash(&root, id, fixed_time()).unwrap_err();

		assert!(matches!(err, Error::OutsideProject));
		assert!(root.join("Manuscript").join("Chapter 1.md").exists());
	}

	#[test]
	fn deleting_refuses_a_relative_path() {
		let err = delete_document(PathBuf::from("some/where"), Uuid::new_v4()).unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn a_stamp_is_sortable_and_holds_no_separators() {
		assert_eq!(stamp(fixed_time()), "20231114-221320");
		assert!(!stamp(fixed_time()).contains(' '));
	}

	#[test]
	fn a_stamp_reads_back_as_the_moment_and_the_name() {
		let (at, was) = unstamp("20231114-221320 Chapter 1.md").unwrap();
		assert_eq!(at, fixed_time());
		assert_eq!(was, "Chapter 1.md");

		// The suffix a second deletion in the same second carries.
		let (at, was) = unstamp("20231114-221320-1 Chapter 1.md").unwrap();
		assert_eq!(at, fixed_time());
		assert_eq!(was, "Chapter 1.md");
	}

	#[test]
	fn a_name_that_is_not_a_stamp_is_not_read_as_one() {
		for name in [
			"Chapter 1.md",
			"20231114 Chapter 1.md",
			"2023111-4221320 Chapter 1.md",
			"20231145-221320 Chapter 1.md",
			"20231114-991320 Chapter 1.md",
			"abcdefgh-221320 Chapter 1.md",
			"20231114-221320Chapter 1.md",
			"20231114-221320- Chapter 1.md",
			"20231114-221320-x Chapter 1.md",
		] {
			assert!(unstamp(name).is_none(), "{name:?}");
		}
	}

	/// A project with one document deleted out of Manuscript.
	fn with_chapter_one_deleted(parent: &tempfile::TempDir) -> PathBuf {
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;
		write_document(root.clone(), id, "Sing to me of the man, Muse.".to_owned()).unwrap();
		trash(&root, id, fixed_time()).unwrap();
		root
	}

	#[test]
	fn the_trash_lists_what_was_deleted_and_when() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_chapter_one_deleted(&parent);

		let listed = list_trash(root).unwrap();

		assert_eq!(listed.len(), 1);
		assert_eq!(listed[0].title, "Chapter 1");
		assert_eq!(listed[0].folder, "Manuscript");
		assert_eq!(listed[0].path, "Manuscript/20231114-221320 Chapter 1.md");
		assert_eq!(listed[0].deleted, Some(fixed_time()));
	}

	#[test]
	fn an_empty_trash_lists_nothing() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		assert!(list_trash(root).unwrap().is_empty());
	}

	#[test]
	fn the_trash_shows_the_most_recent_first_and_the_undated_last() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let later = fixed_time() + time::Duration::days(1);

		let first = first_document(&root).id;
		trash(&root, first, fixed_time()).unwrap();
		let second = create_document(root.clone(), "Notes".to_owned(), "Ideas".to_owned()).unwrap();
		trash(&root, second.id, later).unwrap();
		// Something a writer dropped in by hand.
		fs::write(root.join(TRASH_DIR).join("Notes").join("Stray.md"), "").unwrap();

		let listed = list_trash(root).unwrap();

		let titles: Vec<_> = listed.iter().map(|e| e.title.as_str()).collect();
		assert_eq!(titles, ["Ideas", "Chapter 1", "Stray"]);
		assert!(listed[2].deleted.is_none());
	}

	#[test]
	fn a_deleted_document_can_be_put_back_where_it_was() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_chapter_one_deleted(&parent);
		let entry = list_trash(root.clone()).unwrap().remove(0);

		let back = restore_from_trash(root.clone(), entry.path).unwrap();

		assert_eq!(back.path, "Manuscript/Chapter 1.md");
		assert_eq!(back.title, "Chapter 1");
		assert_eq!(
			read_document(root.clone(), back.id).unwrap(),
			"Sing to me of the man, Muse."
		);
		assert!(list_trash(root).unwrap().is_empty(), "it left the trash");
	}

	#[test]
	fn putting_one_back_over_a_document_of_that_name_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_chapter_one_deleted(&parent);
		create_document(
			root.clone(),
			"Manuscript".to_owned(),
			"Chapter 1".to_owned(),
		)
		.unwrap();
		let entry = list_trash(root.clone()).unwrap().remove(0);

		let err = restore_from_trash(root.clone(), entry.path).unwrap_err();

		assert!(matches!(err, Error::DocumentExists));
		assert_eq!(list_trash(root).unwrap().len(), 1, "it is still there");
	}

	#[test]
	fn putting_one_back_recreates_a_section_folder_that_has_gone() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_chapter_one_deleted(&parent);
		fs::remove_dir_all(root.join("Manuscript")).unwrap();
		let entry = list_trash(root.clone()).unwrap().remove(0);

		let back = restore_from_trash(root.clone(), entry.path).unwrap();

		assert_eq!(back.folder, "Manuscript");
		assert_eq!(
			read_document(root, back.id).unwrap(),
			"Sing to me of the man, Muse."
		);
	}

	#[test]
	fn a_purged_entry_is_gone_for_good() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_chapter_one_deleted(&parent);
		let entry = list_trash(root.clone()).unwrap().remove(0);

		purge_trash_entry(root.clone(), entry.path).unwrap();

		assert!(list_trash(root.clone()).unwrap().is_empty());
		assert_eq!(scan_paths(&root).len(), 4);
	}

	#[test]
	fn the_trash_commands_refuse_an_entry_that_is_not_there() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let path = "Manuscript/20231114-221320 Chapter 1.md".to_owned();

		assert!(matches!(
			restore_from_trash(root.clone(), path.clone()).unwrap_err(),
			Error::DocumentMissing
		));
		assert!(matches!(
			purge_trash_entry(root, path).unwrap_err(),
			Error::DocumentMissing
		));
	}

	#[test]
	fn the_trash_commands_refuse_a_path_out_of_the_trash() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_chapter_one_deleted(&parent);
		fs::write(parent.path().join("secrets.md"), "not yours").unwrap();

		for path in [
			"secrets.md",
			"../secrets.md",
			"Manuscript/../../secrets.md",
			"Scraps/Offcut.md",
			"Manuscript/notes.txt",
		] {
			assert!(matches!(
				purge_trash_entry(root.clone(), path.to_owned()).unwrap_err(),
				Error::BadDocumentPath | Error::DocumentMissing
			));
		}

		assert!(parent.path().join("secrets.md").exists());
	}

	#[cfg(unix)]
	#[test]
	fn a_trash_entry_that_leads_out_of_the_project_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_chapter_one_deleted(&parent);
		let outside = parent.path().join("secrets.md");
		fs::write(&outside, "not yours").unwrap();
		fs::create_dir_all(root.join(TRASH_DIR).join("Notes")).unwrap();
		std::os::unix::fs::symlink(&outside, root.join(TRASH_DIR).join("Notes").join("Link.md"))
			.unwrap();

		let err = purge_trash_entry(root, "Notes/Link.md".to_owned()).unwrap_err();

		assert!(matches!(err, Error::OutsideProject));
		assert!(outside.exists());
	}

	#[test]
	fn the_trash_commands_refuse_a_relative_path() {
		let relative = PathBuf::from("some/where");
		assert!(matches!(
			list_trash(relative.clone()).unwrap_err(),
			Error::RelativePath
		));
		assert!(matches!(
			restore_from_trash(relative.clone(), "Notes/Notes.md".to_owned()).unwrap_err(),
			Error::RelativePath
		));
		assert!(matches!(
			purge_trash_entry(relative, "Notes/Notes.md".to_owned()).unwrap_err(),
			Error::RelativePath
		));
	}

	/// A novel whose Manuscript holds three chapters in order.
	fn with_three_chapters(parent: &tempfile::TempDir) -> PathBuf {
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		for name in ["Chapter 2", "Chapter 3"] {
			create_document(root.clone(), "Manuscript".to_owned(), name.to_owned()).unwrap();
		}
		root
	}

	/// The titles in one section, in the order the manifest records them.
	fn order_of(root: &Path, folder: &str) -> Vec<String> {
		sections(&read_manifest(root).unwrap())
			.into_iter()
			.find(|s| s.folder == folder)
			.unwrap()
			.documents
			.iter()
			.map(|d| d.title.clone())
			.collect()
	}

	fn chapter(root: &Path, title: &str) -> Uuid {
		read_manifest(root)
			.unwrap()
			.documents()
			.iter()
			.find(|d| d.path == format!("Manuscript/{title}.md"))
			.unwrap()
			.id
	}

	#[test]
	fn a_document_can_be_moved_up_its_section() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);

		reorder_document(root.clone(), chapter(&root, "Chapter 3"), 0).unwrap();

		assert_eq!(
			order_of(&root, "Manuscript"),
			["Chapter 3", "Chapter 1", "Chapter 2"]
		);
	}

	#[test]
	fn a_document_can_be_moved_down_its_section() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);

		reorder_document(root.clone(), chapter(&root, "Chapter 1"), 1).unwrap();

		assert_eq!(
			order_of(&root, "Manuscript"),
			["Chapter 2", "Chapter 1", "Chapter 3"]
		);
	}

	#[test]
	fn an_index_past_the_end_means_the_end() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);

		reorder_document(root.clone(), chapter(&root, "Chapter 1"), 99).unwrap();

		assert_eq!(
			order_of(&root, "Manuscript"),
			["Chapter 2", "Chapter 3", "Chapter 1"]
		);
	}

	#[test]
	fn moving_a_document_where_it_already_is_changes_nothing() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);
		let before = read_manifest(&root).unwrap();

		reorder_document(root.clone(), chapter(&root, "Chapter 2"), 1).unwrap();

		assert_eq!(read_manifest(&root).unwrap(), before);
	}

	#[test]
	fn reordering_one_section_leaves_the_others_alone() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);
		create_document(root.clone(), "Notes".to_owned(), "Ideas".to_owned()).unwrap();
		let notes = order_of(&root, "Notes");

		reorder_document(root.clone(), chapter(&root, "Chapter 3"), 0).unwrap();

		assert_eq!(order_of(&root, "Notes"), notes);
		assert_eq!(read_manifest(&root).unwrap().documents().len(), 8);
	}

	#[test]
	fn a_new_order_survives_a_refresh() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);

		reorder_document(root.clone(), chapter(&root, "Chapter 3"), 0).unwrap();
		refresh(&root).unwrap();

		assert_eq!(
			order_of(&root, "Manuscript"),
			["Chapter 3", "Chapter 1", "Chapter 2"]
		);
	}

	#[test]
	fn reordering_touches_no_files() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);
		let before = scan_paths(&root);

		reorder_document(root.clone(), chapter(&root, "Chapter 3"), 0).unwrap();

		assert_eq!(scan_paths(&root), before);
	}

	#[test]
	fn a_document_whose_file_has_gone_can_still_be_moved() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);
		let id = chapter(&root, "Chapter 3");
		fs::remove_file(root.join("Manuscript").join("Chapter 3.md")).unwrap();

		reorder_document(root.clone(), id, 0).unwrap();

		assert_eq!(
			order_of(&root, "Manuscript"),
			["Chapter 3", "Chapter 1", "Chapter 2"]
		);
	}

	#[test]
	fn reordering_an_unknown_document_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = reorder_document(root, Uuid::new_v4(), 0).unwrap_err();
		assert!(matches!(err, Error::UnknownDocument));
	}

	#[test]
	fn reordering_refuses_a_relative_path() {
		let err = reorder_document(PathBuf::from("some/where"), Uuid::new_v4(), 0).unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn reordering_leaves_the_other_sections_where_they_were() {
		let mut manifest = manifest_with(&[
			"Manuscript/Chapter 1.md",
			"Notes/Notes.md",
			"Manuscript/Chapter 2.md",
		]);
		let second = manifest.documents()[1].id;

		reorder(&mut manifest, second, 0).unwrap();

		assert_eq!(
			paths_of(&manifest),
			[
				"Manuscript/Chapter 2.md",
				"Manuscript/Chapter 1.md",
				"Notes/Notes.md"
			],
			"a document moves among the ones it sits beside and nowhere else"
		);
	}

	#[test]
	fn writing_refuses_a_relative_path() {
		let err =
			write_document(PathBuf::from("some/where"), Uuid::new_v4(), String::new()).unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn every_document_comes_back_with_its_text() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Manuscript/Chapter 1.md"), "Wren went down.").unwrap();
		fs::write(root.join("Notes/Notes.md"), "Ask about Wren.").unwrap();

		let all = read_all_documents(root.clone()).unwrap();

		assert_eq!(
			all.iter()
				.map(|d| d.document.path.as_str())
				.collect::<Vec<_>>(),
			[
				"Manuscript/Chapter 1.md",
				"Outline/Outline.md",
				"Characters/Characters.md",
				"Locations/Locations.md",
				"Notes/Notes.md",
			],
			"manifest order, which is the order the sidebar shows"
		);
		assert_eq!(all[0].text.as_deref(), Some("Wren went down."));
		assert_eq!(all[4].text.as_deref(), Some("Ask about Wren."));
		assert_eq!(all[0].document.title, "Chapter 1");
		assert_eq!(all[0].document.folder, "Manuscript");
	}

	#[test]
	fn a_document_whose_file_has_gone_comes_back_without_text() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Notes/Notes.md"), "still here").unwrap();
		fs::remove_file(root.join("Manuscript/Chapter 1.md")).unwrap();

		let all = read_all_documents(root).unwrap();

		assert_eq!(all.len(), 5, "it is still listed, so search can name it");
		assert!(all[0].text.is_none());
		assert_eq!(
			all[4].text.as_deref(),
			Some("still here"),
			"one unreadable file does not cost the rest"
		);
	}

	#[test]
	fn reading_everything_does_not_read_a_document_outside_the_project() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(parent.path().join("secrets.md"), "not yours").unwrap();
		set_document_path(&root, 0, "../secrets.md");

		let all = read_all_documents(root).unwrap();

		assert!(all[0].text.is_none());
	}

	#[test]
	fn reading_everything_refuses_a_relative_path() {
		let err = read_all_documents(PathBuf::from("some/where")).unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}
}
