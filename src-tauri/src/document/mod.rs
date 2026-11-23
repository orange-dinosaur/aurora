use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use time::{Date, Month, OffsetDateTime};
use uuid::Uuid;

use crate::project::{
	Error, MANUSCRIPT, Manifest, Result, adopt_matter, editable, matter, read_manifest,
	remove_if_there, validate_name, write_atomic, write_json, write_manifest,
};
use crate::takes;
use crate::tree::{self, Fields, FolderKind, Node};

mod count;
mod front;
mod paths;
mod scan;
mod trash;

pub(crate) use count::manuscript_words;
use count::*;
pub use front::*;
pub(crate) use paths::*;
pub use scan::*;
pub use trash::*;

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
	pub(crate) fn title(&self) -> String {
		Path::new(&self.path)
			.file_stem()
			.and_then(|stem| stem.to_str())
			.unwrap_or(&self.path)
			.to_owned()
	}
}

/// A document as the sidebar shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DocumentView {
	pub id: Uuid,
	pub path: String,
	/// The folders it sits in, from its section down. A list rather than one
	/// joined name, because the search panel draws the whole of it and the tab
	/// strip only the last.
	pub trail: Vec<String>,
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
			trail: trail_of(&document.path),
			title: document.title(),
			target: document.target,
		}
	}
}

/// Where a deleted document goes. It sits inside the project so that a move
/// into it is a rename rather than a copy, and `scan` never looks at it: it
/// only ever reads the section folders the manifest names.
pub const TRASH_DIR: &str = ".trash";

/// Where the trash keeps a record of each node it holds, beside the entries.
const NODES_DIR: &str = ".nodes";

/// How much of a document's opening the overview carries. Long enough to
/// recognise a chapter by, short enough that a whole section stays small.
const EXCERPT_CHARS: usize = 240;

/// A document as the section overview shows it: the sidebar's view plus enough
/// of the file to tell one chapter from another without opening it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DocumentSummary {
	#[serde(flatten)]
	pub document: DocumentView,
	pub words: usize,
	pub excerpt: String,
	/// The document's front matter block, fences and all, for the other end to
	/// read its fields out of. Empty when the file has none. What a field is
	/// stays defined once, in `src/frontmatter.ts`; the only thing Rust picks
	/// out of a block itself is `inBook`, which the compile walk needs.
	pub front: String,
	#[serde(with = "time::serde::rfc3339::option")]
	#[ts(type = "string | null")]
	pub modified: Option<OffsetDateTime>,
}

impl DocumentSummary {
	/// A card for a document whose file cannot be read.
	fn blank(document: &Document) -> Self {
		Self {
			document: document.into(),
			words: 0,
			excerpt: String::new(),
			front: String::new(),
			modified: None,
		}
	}
}

/// One of a folder's children, as the overview draws it. A document gets the
/// card it has always had; a folder says what it is, how much it holds and how
/// much has been written under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "node", rename_all = "camelCase")]
#[ts(export)]
pub enum ChildSummary {
	Folder {
		id: Uuid,
		name: String,
		kind: Option<FolderKind>,
		/// How many nodes it holds directly, folders and documents alike.
		children: usize,
		/// The words in every document below it, however deep.
		words: usize,
		/// The words the writer means it to reach, `null` when unset.
		target: Option<u32>,
	},
	Document(DocumentSummary),
}

/// One entry in the project's tree as the front end reads it: [`tree::Node`]
/// with the things a node's place decides already worked out. A document
/// carries the same view every other command sends, so nothing outside Rust
/// ever takes a path apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "node", rename_all = "camelCase")]
#[ts(export)]
pub enum NodeView {
	Folder {
		id: Uuid,
		name: String,
		/// Always present, `null` outside the Manuscript, for the same reason
		/// a document's target is.
		kind: Option<FolderKind>,
		children: Vec<NodeView>,
		/// The words in every document below it, however deep, less anything in
		/// the Manuscript's front or back matter.
		words: usize,
		/// The words the writer means it to reach, `null` when they have not
		/// said. Carried here so every row can draw its progress from one read.
		target: Option<u32>,
		/// Whether an export takes it. The sidebar draws what is out of the
		/// book faint, so this rides along with the tree rather than being
		/// asked for a node at a time the way a folder's other fields are.
		///
		/// Spelled out because `rename_all` on an enum renames its variants
		/// and not the fields inside them.
		#[serde(rename = "inBook")]
		in_book: bool,
	},
	Document {
		#[serde(flatten)]
		document: DocumentView,
		/// The words in its body, counted the way a folder's are.
		words: usize,
		#[serde(rename = "inBook")]
		in_book: bool,
	},
}

impl NodeView {
	/// The node's id, whichever kind it is.
	pub fn id(&self) -> Uuid {
		match self {
			NodeView::Folder { id, .. } => *id,
			NodeView::Document { document, .. } => document.id,
		}
	}
}

/// The project root as a real location, which is what the counts below compare
/// a file against. A root that will not resolve is passed through as it came:
/// nothing under it then matches, so the counts come back as zero rather than
/// the call failing.
fn canonical(root: &Path) -> PathBuf {
	root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

/// A document and the whole of its text, for reading the project in one go.
/// The text is `None` when the file could not be read, which the front end
/// shows rather than swallows: a document nobody could look at must not read
/// as a document with nothing in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct DocumentText {
	#[serde(flatten)]
	pub document: DocumentView,
	pub text: Option<String>,
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

	let (front, prose) = split(&text);

	DocumentSummary {
		document: document.into(),
		words: words_of(prose),
		excerpt: excerpt(prose),
		front: front.to_owned(),
		modified,
	}
}

/// The text of one document.
#[tauri::command]
pub fn read_document(root: PathBuf, id: Uuid) -> Result<String> {
	absolute(&root)?;

	let manifest = read_manifest(&root)?;
	let path = resolve(&manifest, &root, id)?;
	String::from_utf8(fs::read(path)?).map_err(|_| Error::NotText)
}

/// Replaces one document's text. The document has to be one the project knows
/// about and its file has to still be there; writing back a file that has
/// vanished is a different thing to ask for.
#[tauri::command]
pub fn write_document(root: PathBuf, id: Uuid, text: String) -> Result<()> {
	absolute(&root)?;

	let manifest = read_manifest(&root)?;
	let path = resolve(&manifest, &root, id)?;
	write_atomic(&path, text.as_bytes())
}

/// A document's text and the file's modified time, in milliseconds, as the
/// tab that holds it saw them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct Opened {
	pub text: String,
	#[ts(type = "number")]
	pub seen: i64,
}

fn modified(path: &Path) -> Result<i64> {
	let since = fs::metadata(path)?
		.modified()?
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap_or_default();
	Ok(i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
}

/// Reads a document for a tab, with the time its file was last written. Read
/// the time first: a change between the two reads then shows up as a refused
/// save rather than going unseen.
#[tauri::command]
pub fn open_document(root: PathBuf, id: Uuid) -> Result<Opened> {
	absolute(&root)?;

	let manifest = read_manifest(&root)?;
	let path = resolve(&manifest, &root, id)?;
	let seen = modified(&path)?;
	let text = String::from_utf8(fs::read(&path)?).map_err(|_| Error::NotText)?;
	Ok(Opened { text, seen })
}

/// Replaces one document's text like `write_document`, and hands back the
/// file's new modified time. With a `seen` that is not the file's current
/// time, something else has written it since the tab read it, and nothing is
/// written.
#[tauri::command]
pub fn save_document(root: PathBuf, id: Uuid, text: String, seen: Option<i64>) -> Result<i64> {
	absolute(&root)?;

	let manifest = read_manifest(&root)?;
	let path = resolve(&manifest, &root, id)?;
	if let Some(seen) = seen
		&& seen != modified(&path)?
	{
		return Err(Error::ChangedOnDisk);
	}
	write_atomic(&path, text.as_bytes())?;
	modified(&path)
}

/// Puts a document that has gone missing back on disk and brings the manifest
/// with it. The path comes from the tab that still holds the text rather than
/// from the manifest, since a refresh may already have dropped the document.
/// The document it returns is the one to carry on editing: the same id if the
/// manifest still knew the path, a fresh one if it had to be adopted again.
#[tauri::command]
pub fn restore_document(root: PathBuf, path: String, text: String) -> Result<DocumentView> {
	let manifest = editable(&root)?;
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
	// A section folder can be a symlink the writer made, so where it actually
	// leads is what decides whether this write stays inside the project.
	make_inside(root, folder)?;

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

/// Starts a new, empty document inside a folder. The name is the title, and a
/// document may sit at any level: `parent_id` is a section as readily as a
/// chapter three folders down.
#[tauri::command]
pub fn create_document(root: PathBuf, parent_id: Uuid, name: String) -> Result<DocumentView> {
	let manifest = editable(&root)?;
	let name = without_extension(&name);
	validate_name(name)?;

	let inside = folder_at(&manifest, parent_id)?.0;

	add_document(&root, &manifest, &format!("{inside}/{name}.md"), "")
}

/// The path of the folder with this id, and its kind. An id naming a document,
/// or nothing at all, is not somewhere to put anything.
fn folder_at(manifest: &Manifest, id: Uuid) -> Result<(String, Option<FolderKind>)> {
	match tree::find(&manifest.nodes, id) {
		Some(Node::Folder { kind, .. }) => {
			Ok((tree::path(&manifest.nodes, id).unwrap_or_default(), *kind))
		}
		_ => Err(Error::UnknownFolder),
	}
}

/// Makes a folder inside another one, on disk and in the manifest. What it may
/// be is [`tree::may_hold`]: the front end only offers the kinds that fit, and
/// this is what makes that true rather than polite.
#[tauri::command]
pub fn create_folder(
	root: PathBuf,
	parent_id: Uuid,
	name: String,
	kind: Option<FolderKind>,
) -> Result<NodeView> {
	let mut manifest = editable(&root)?;
	validate_name(&name)?;

	let chain = tree::trail(&manifest.nodes, parent_id).ok_or(Error::UnknownFolder)?;
	let (inside, parent) = folder_at(&manifest, parent_id)?;

	// The section at the head of the chain is what decides whether kinds mean
	// anything here at all.
	if !tree::may_hold(chain[0].name() == MANUSCRIPT, parent, kind) {
		return Err(Error::FolderNotAllowed);
	}
	let siblings = tree::children(&manifest.nodes, parent_id).unwrap_or_default();
	if siblings.iter().any(|node| node.name() == name) {
		return Err(Error::AlreadyExists);
	}
	// One front matter and one back matter to a Manuscript, whatever they end
	// up called. The + stops offering a kind once it is there, so this is only
	// what a second attempt meets.
	if matter(kind)
		&& siblings
			.iter()
			.any(|node| matches!(node, Node::Folder { kind: held, .. } if *held == kind))
	{
		return Err(Error::FolderNotAllowed);
	}

	let directory = folder_path(&manifest, &root, &format!("{inside}/{name}"))?;
	// Where the folder above actually leads is what decides whether this stays
	// inside the project: a section folder can be a symlink the writer made.
	let above = directory.parent().ok_or(Error::BadDocumentPath)?;
	under(&root, above)?;
	fs::create_dir(&directory)?;

	let made = Node::Folder {
		id: Uuid::new_v4(),
		name,
		kind,
		target: None,
		fields: Fields::new(),
		children: Vec::new(),
	};
	let view = views(
		&canonical(&root),
		std::slice::from_ref(&made),
		&format!("{inside}/"),
	)
	.0
	.pop()
	.expect("one node in, one out");

	let Some(Node::Folder { children, .. }) = tree::find_mut(&mut manifest.nodes, parent_id) else {
		unreachable!("the parent was a folder a moment ago")
	};
	children.push(made);
	write_manifest(&root, &mut manifest)?;

	Ok(view)
}

/// The project as a tree, straight from the manifest. Every listing of the
/// project is this one, however much of it the caller draws.
#[tauri::command(async)]
pub fn document_tree(root: PathBuf) -> Result<Vec<NodeView>> {
	absolute(&root)?;
	Ok(views(&canonical(&root), &read_manifest(&root)?.nodes, "").0)
}

/// Looks at the project's folders again for anything added, removed or renamed
/// outside Aurora, and brings the manifest back in step with what it finds.
/// Nothing comes back: whoever asked reads the tree again afterwards.
#[tauri::command]
pub fn refresh_documents(root: PathBuf) -> Result<()> {
	absolute(&root)?;
	refresh(&root)?;
	Ok(())
}

/// Whether going from `from` to `to` changes nothing but the case of the name.
fn case_only(from: &Path, to: &Path) -> bool {
	from != to && from.to_string_lossy().to_lowercase() == to.to_string_lossy().to_lowercase()
}

/// Whether something other than `from` is already at `to`. On a filesystem that
/// ignores case, a name differing from `from` only by case finds `from` itself,
/// so there it counts only when the folder lists it spelled exactly as asked.
fn taken(from: &Path, to: &Path) -> Result<bool> {
	if !to.exists() {
		return Ok(false);
	}
	if !case_only(from, to) {
		return Ok(true);
	}
	let (Some(dir), Some(name)) = (to.parent(), to.file_name()) else {
		return Ok(true);
	};
	for entry in fs::read_dir(dir)? {
		if entry?.file_name().as_os_str() == name {
			return Ok(true);
		}
	}
	Ok(false)
}

/// The view of one document, as the manifest has it now.
fn view_of(manifest: &Manifest, id: Uuid) -> Result<DocumentView> {
	manifest
		.documents()
		.iter()
		.find(|d| d.id == id)
		.map(DocumentView::from)
		.ok_or(Error::UnknownDocument)
}

/// Gives a document a new title, which is to say a new file name. It stays in
/// its section, and keeps its id and its place in the order.
#[tauri::command]
pub fn rename_document(root: PathBuf, id: Uuid, name: String) -> Result<DocumentView> {
	let mut manifest = editable(&root)?;
	let name = without_extension(&name);
	validate_name(name)?;

	// The safety check. The path used below is built the way `to` is, since a
	// canonical one (`\\?\` on Windows) is not the same shape and would hide a
	// change of case.
	resolve(&manifest, &root, id)?;

	let was = tree::path(&manifest.nodes, id).ok_or(Error::UnknownDocument)?;
	let from = document_path(&manifest, &root, &was)?;
	let file = format!("{name}.md");
	// A document is renamed where it stands, so its path is the one it has
	// with the last part swapped.
	let path = match was.rfind('/') {
		Some(slash) => format!("{}/{file}", &was[..slash]),
		None => return Err(Error::BadDocumentPath),
	};

	// Being renamed to what it is already called is not a collision with
	// itself.
	if path == was {
		return view_of(&manifest, id);
	}

	let to = document_path(&manifest, &root, &path)?;
	if taken(&from, &to)? {
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
	view_of(&manifest, id)
}

/// Gives a folder a new name, which is to say a new directory name. Everything
/// under it keeps its id and its place, because nothing anywhere stores a path:
/// a descendant is found by walking down to it, and the walk is the same walk.
///
/// A section is refused. The Manuscript is recognised by its name and the kind
/// rules stand on that, so the top level of a project holds still.
#[tauri::command]
pub fn rename_folder(root: PathBuf, id: Uuid, name: String) -> Result<NodeView> {
	let mut manifest = editable(&root)?;
	validate_name(&name)?;

	if !matches!(tree::find(&manifest.nodes, id), Some(Node::Folder { .. })) {
		return Err(Error::UnknownFolder);
	}

	let Some(above) = tree::parent(&manifest.nodes, id) else {
		return Err(Error::SectionFixed);
	};
	let Node::Folder { children, .. } = above else {
		unreachable!("whatever holds a node is a folder")
	};
	// Being called what it is already called is not a collision with itself.
	if children
		.iter()
		.any(|node| node.id() != id && node.name() == name)
	{
		return Err(Error::AlreadyExists);
	}

	let was = tree::path(&manifest.nodes, id).ok_or(Error::UnknownFolder)?;
	// A folder is renamed where it stands, so its path is the one it has with
	// the last part swapped.
	let inside = match was.rfind('/') {
		Some(slash) => was[..slash].to_owned(),
		None => return Err(Error::BadDocumentPath),
	};
	let path = format!("{inside}/{name}");

	// The prefix a view is built under is where the folder sits, which the
	// rename does not change.
	let view = |manifest: &Manifest| {
		let node = tree::find(&manifest.nodes, id).ok_or(Error::UnknownFolder)?;
		views(
			&canonical(&root),
			std::slice::from_ref(node),
			&format!("{inside}/"),
		)
		.0
		.pop()
		.ok_or(Error::UnknownFolder)
	};

	if path == was {
		return view(&manifest);
	}

	let from = folder_path(&manifest, &root, &was)?;
	let to = folder_path(&manifest, &root, &path)?;

	// Where the folder above actually leads is what decides whether this stays
	// inside the project, the same as when the folder was made.
	let over = to.parent().ok_or(Error::BadDocumentPath)?;
	under(&root, over)?;
	// The manifest has already said no sibling is called this. Anything at the
	// new name is something Aurora does not know about, and is not to be
	// written over.
	if taken(&from, &to)? {
		return Err(Error::AlreadyExists);
	}

	// The directory moves first. If writing the manifest then fails, the next
	// refresh adopts the renamed folder and what is inside it rather than
	// losing any of it.
	fs::rename(&from, &to)?;

	match tree::find_mut(&mut manifest.nodes, id) {
		Some(Node::Folder { name: called, .. }) => *called = name,
		_ => return Err(Error::UnknownFolder),
	}
	write_manifest(&root, &mut manifest)?;
	view(&manifest)
}

/// Sets the word target a document is written towards, or clears it with
/// `None`. Nothing on disk changes but the manifest: the target is the writer's
/// intention, not part of the text.
#[tauri::command]
pub fn set_document_target(root: PathBuf, id: Uuid, target: Option<u32>) -> Result<DocumentView> {
	let mut manifest = editable(&root)?;

	// Aiming at no words is the same as not aiming, and saying so here keeps
	// the two apart everywhere above.
	let target = target.filter(|words| *words > 0);

	match tree::find_mut(&mut manifest.nodes, id) {
		Some(Node::Document { target: aim, .. }) => *aim = target,
		_ => return Err(Error::UnknownDocument),
	}

	write_manifest(&root, &mut manifest)?;
	view_of(&manifest, id)
}

/// Sets the word target a folder is written towards, or clears it with `None`.
/// The same field a document has, on the other kind of node, so a chapter and
/// the whole Manuscript are aimed the same way.
#[tauri::command]
pub fn set_folder_target(root: PathBuf, id: Uuid, target: Option<u32>) -> Result<()> {
	let mut manifest = editable(&root)?;

	let target = target.filter(|words| *words > 0);

	match tree::find_mut(&mut manifest.nodes, id) {
		Some(Node::Folder { target: aim, .. }) => *aim = target,
		_ => return Err(Error::UnknownFolder),
	}

	write_manifest(&root, &mut manifest)?;
	Ok(())
}

/// How a folder stands against what it is aiming at: everything written under
/// it, and the target on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FolderProgress {
	/// The words in every document below it, however deep, less anything in the
	/// Manuscript's front or back matter.
	pub words: usize,
	pub target: Option<u32>,
}

/// A folder's progress, asked for on its own the way its fields are: only the
/// panel wants it, and the tree that already carries the count is read for the
/// sidebar rather than for this.
#[tauri::command]
pub fn folder_progress(root: PathBuf, id: Uuid) -> Result<FolderProgress> {
	absolute(&root)?;

	let manifest = read_manifest(&root)?;
	let Some(Node::Folder {
		target, children, ..
	}) = tree::find(&manifest.nodes, id)
	else {
		return Err(Error::UnknownFolder);
	};

	let prefix = tree::path(&manifest.nodes, id).unwrap_or_default();
	Ok(FolderProgress {
		words: words_under(&canonical(&root), children, &format!("{prefix}/")),
		target: *target,
	})
}

/// What the writer has said about one folder, empty when they have said
/// nothing. Asked for on its own rather than carried on every view of a folder:
/// only the panel wants it, and reading it costs a manifest and no files.
#[tauri::command]
pub fn folder_fields(root: PathBuf, id: Uuid) -> Result<Fields> {
	absolute(&root)?;

	match tree::find(&read_manifest(&root)?.nodes, id) {
		Some(Node::Folder { fields, .. }) => Ok(fields.clone()),
		_ => Err(Error::UnknownFolder),
	}
}

/// Replaces everything the writer has said about a folder. A folder has no file
/// of its own to keep a front matter block in, so the manifest holds its fields
/// instead; the whole set arrives at once because the panel that sends them
/// holds the whole set.
#[tauri::command]
pub fn set_folder_fields(root: PathBuf, id: Uuid, fields: Fields) -> Result<()> {
	let mut manifest = editable(&root)?;
	match tree::find_mut(&mut manifest.nodes, id) {
		Some(Node::Folder { fields: held, .. }) => *held = fields,
		_ => return Err(Error::UnknownFolder),
	}

	write_manifest(&root, &mut manifest)?;
	Ok(())
}

/// One document in the project's trash, as the Trash view shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
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
	#[ts(type = "string | null")]
	pub deleted: Option<OffsetDateTime>,
	/// Nothing when the entry is a document on its own. When it is a whole
	/// folder, how many documents went into the trash inside it.
	pub inside: Option<u32>,
}

/// What a folder holds, one card at a time, in the order the folder keeps
/// them. Reads the manifest rather than the folder, the way `document_tree`
/// does.
#[tauri::command(async)]
pub fn folder_overview(root: PathBuf, id: Uuid) -> Result<Vec<ChildSummary>> {
	absolute(&root)?;

	let manifest = read_manifest(&root)?;
	// The folder's own path, which is the prefix of everything it holds. Ask
	// for it first: an id that names a document, or nothing at all, is not a
	// folder to look inside.
	let prefix = match tree::find(&manifest.nodes, id) {
		Some(Node::Folder { .. }) => tree::path(&manifest.nodes, id).unwrap_or_default(),
		_ => return Err(Error::UnknownFolder),
	};

	let counting_root = canonical(&root);

	Ok(tree::children(&manifest.nodes, id)
		.unwrap_or_default()
		.iter()
		.map(|node| match node {
			// A folder card says what it is, how much it holds and what it is
			// aiming at. What the writer said about it is the panel's
			// business, not the card's.
			Node::Folder {
				id,
				name,
				kind,
				children,
				target,
				..
			} => ChildSummary::Folder {
				id: *id,
				name: name.clone(),
				kind: *kind,
				children: children.len(),
				words: words_under(&counting_root, children, &format!("{prefix}/{name}/")),
				target: *target,
			},
			Node::Document {
				id, name, target, ..
			} => {
				let document = Document {
					id: *id,
					path: format!("{prefix}/{name}"),
					target: *target,
				};
				ChildSummary::Document(match within(&counting_root, &document.path) {
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
#[tauri::command(async)]
pub fn read_all_documents(root: PathBuf) -> Result<Vec<DocumentText>> {
	absolute(&root)?;

	let manifest = read_manifest(&root)?;
	let base = canonical(&root);

	Ok(manifest
		.documents()
		.iter()
		.map(|document| DocumentText {
			document: document.into(),
			// A file that has vanished, is not UTF-8, or that the manifest
			// points outside the project, costs that one document rather than
			// the whole search.
			text: within(&base, &document.path)
				.and_then(|path| String::from_utf8(fs::read(path)?).map_err(|_| Error::NotText))
				.ok(),
		})
		.collect())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::project::{Format, NameError, create};
	use crate::tree::{self, Value, tree_from_flat};
	use time::OffsetDateTime;

	fn trash(root: &Path, id: Uuid, at: OffsetDateTime) -> Result<()> {
		super::trash(root, read_manifest(root)?, id, at)
	}

	fn fixed_time() -> OffsetDateTime {
		OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
	}

	#[test]
	fn a_document_id_survives_the_round_trip() {
		let document = Document::new("Manuscript", "Scene 1.md");
		let json = serde_json::to_value(&document).unwrap();
		assert_eq!(json["path"], "Manuscript/Scene 1.md");
		assert_eq!(json["id"], document.id.to_string());
		assert_eq!(serde_json::from_value::<Document>(json).unwrap(), document);
	}

	/// The documents one section holds, in the order the manifest records them.
	/// The commands send the whole tree; the tests that predate it only ever
	/// look at one flat level of it.
	fn documents_in(manifest: &Manifest, section: &str) -> Vec<DocumentView> {
		let Some(Node::Folder { children, .. }) =
			manifest.nodes.iter().find(|node| node.name() == section)
		else {
			panic!("the project has no section called {section}");
		};

		children
			.iter()
			.filter_map(|node| match node {
				Node::Document {
					id, name, target, ..
				} => Some(DocumentView::from(&Document {
					id: *id,
					path: format!("{section}/{name}"),
					target: *target,
				})),
				Node::Folder { .. } => None,
			})
			.collect()
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
				"Manuscript/Scene 1.md",
				"Outline/Outline.md",
				"Notes/Notes.md"
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
				"Manuscript/Chapter 2.md",
				"Manuscript/Chapter 3.md",
				"Manuscript/Scene 1.md",
			]
		);
	}

	/// The same table is in `words.test.ts`: the status line and the section
	/// cards must count markdown alike.
	#[test]
	fn markdown_markers_are_not_words() {
		let table = [
			("# Title", 1),
			("###### Deep heading", 2),
			("####### seven hashes", 3),
			("# Title\n\n* * *\n\nHello world", 3),
			("---", 0),
			("***", 0),
			("_ _ _", 0),
			("> quoted line", 2),
			("> > nested quote", 2),
			("- one\n- two", 2),
			("* one\n+ two", 2),
			("1. one\n2. two", 2),
			("**bold** and _italic_", 3),
			("~~gone~~ now", 2),
			("`code` here", 2),
			("a \\*b c", 3),
			("Elena\\.", 1),
			("2024. A year", 2),
			("2024.5 is a number", 4),
		];

		for (markdown, count) in table {
			assert_eq!(words_of(markdown), count, "{markdown:?}");
		}
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
			["Manuscript/Part One/Chapter 2.md", "Manuscript/Scene 1.md"],
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
		let Some([Node::Folder { name, children, .. }, _]) =
			tree::children(&found, manuscript.id())
		else {
			panic!("the new directory and the seed scene, in that order")
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
		fs::write(hidden.join("Scene 1.md"), "").unwrap();

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
		assert_eq!(scan_paths(&root).len(), 2);
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
		let mut manifest = manifest_with(&["Manuscript/Scene 1.md", "Notes/Notes.md"]);
		let before = manifest.documents();

		assert!(!reconcile(
			&mut manifest,
			&found(&["Manuscript/Scene 1.md", "Notes/Notes.md"])
		));
		assert_eq!(manifest.documents(), before);
	}

	#[test]
	fn a_new_file_is_adopted_at_the_end_of_its_section() {
		let mut manifest = manifest_with(&["Manuscript/Scene 1.md", "Notes/Notes.md"]);
		let chapter_one = manifest.documents()[0].id;

		assert!(reconcile(
			&mut manifest,
			&found(&[
				"Manuscript/Scene 1.md",
				"Manuscript/Chapter 2.md",
				"Notes/Notes.md",
			])
		));
		assert_eq!(
			paths_of(&manifest),
			[
				"Manuscript/Scene 1.md",
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
		let mut manifest = manifest_with(&["Manuscript/Scene 1.md", "Notes/Notes.md"]);

		assert!(reconcile(&mut manifest, &found(&["Notes/Notes.md"])));
		assert_eq!(paths_of(&manifest), ["Notes/Notes.md"]);
	}

	#[test]
	fn one_file_renamed_on_disk_keeps_its_id() {
		let mut manifest = manifest_with(&["Manuscript/Scene 1.md", "Manuscript/Scene 2.md"]);
		let ids: Vec<Uuid> = manifest.documents().iter().map(|d| d.id).collect();

		assert!(reconcile(
			&mut manifest,
			&found(&["Manuscript/Scene 1.md", "Manuscript/Opening.md"])
		));

		let after = manifest.documents();
		assert_eq!(
			paths_of(&manifest),
			["Manuscript/Scene 1.md", "Manuscript/Opening.md"]
		);
		assert_eq!(
			after[1].id, ids[1],
			"the renamed one is still the same document"
		);
	}

	#[test]
	fn one_folder_renamed_on_disk_keeps_its_id() {
		let mut manifest = manifest_with(&["Manuscript/Act 1/Scene 1.md"]);
		let document = manifest.documents()[0].id;
		let Node::Folder { children, .. } = &manifest.nodes[0] else {
			unreachable!()
		};
		let act = children[0].id();

		assert!(reconcile(
			&mut manifest,
			&found(&["Manuscript/Act I/Scene 1.md"])
		));

		assert_eq!(paths_of(&manifest), ["Manuscript/Act I/Scene 1.md"]);
		assert_eq!(manifest.documents()[0].id, document);
		let Node::Folder { children, .. } = &manifest.nodes[0] else {
			unreachable!()
		};
		assert_eq!(children[0].id(), act, "the folder keeps its id");
	}

	#[test]
	fn two_missing_and_two_found_are_left_as_additions_and_removals() {
		let mut manifest = manifest_with(&["Manuscript/One.md", "Manuscript/Two.md"]);
		let ids: Vec<Uuid> = manifest.documents().iter().map(|d| d.id).collect();

		assert!(reconcile(
			&mut manifest,
			&found(&["Manuscript/Three.md", "Manuscript/Four.md"])
		));

		for document in manifest.documents() {
			assert!(!ids.contains(&document.id));
		}
	}

	#[test]
	fn the_recorded_order_wins_over_the_order_on_disk() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 2.md", "Manuscript/Scene 1.md"]);

		assert!(!reconcile(
			&mut manifest,
			&found(&["Manuscript/Scene 1.md", "Manuscript/Chapter 2.md"])
		));
		assert_eq!(
			paths_of(&manifest),
			["Manuscript/Chapter 2.md", "Manuscript/Scene 1.md"]
		);
	}

	#[test]
	fn a_manifest_without_documents_adopts_everything() {
		let mut manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());

		assert!(reconcile(
			&mut manifest,
			&found(&["Manuscript/Scene 1.md", "Notes/Notes.md"])
		));
		assert_eq!(
			paths_of(&manifest),
			["Manuscript/Scene 1.md", "Notes/Notes.md"]
		);
		let ids: HashSet<_> = manifest.documents().iter().map(|d| d.id).collect();
		assert_eq!(ids.len(), 2, "each adopted file gets its own id");
	}

	#[test]
	fn a_path_recorded_twice_is_collapsed() {
		let mut manifest = manifest_with(&["Manuscript/Scene 1.md", "Manuscript/Scene 1.md"]);
		let first = manifest.documents()[0].id;

		assert!(reconcile(&mut manifest, &found(&["Manuscript/Scene 1.md"])));
		assert_eq!(paths_of(&manifest), ["Manuscript/Scene 1.md"]);
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
		let mut manifest = manifest_with(&["Manuscript/Scene 1.md", "Notes/Notes.md"]);
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
	fn the_tree_holds_the_project_under_its_sections() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manifest = read_manifest(&root).unwrap();

		let folders: Vec<_> = manifest.nodes.iter().map(Node::name).collect();
		assert_eq!(
			folders,
			["Manuscript", "Outline", "Characters", "Locations", "Notes"]
		);

		let manuscript = documents_in(&manifest, "Manuscript");
		assert_eq!(manuscript.len(), 1);
		assert_eq!(manuscript[0].title, "Scene 1");
		assert_eq!(manuscript[0].trail, ["Manuscript"]);
		assert_eq!(manuscript[0].path, "Manuscript/Scene 1.md");
	}

	#[test]
	fn an_empty_section_is_still_there() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::remove_file(root.join("Notes").join("Notes.md")).unwrap();
		refresh(&root).unwrap();

		assert!(documents_in(&read_manifest(&root).unwrap(), "Notes").is_empty());
	}

	#[test]
	fn listing_does_not_look_at_the_folder_but_refreshing_does() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Notes").join("Ideas.md"), "").unwrap();

		assert_eq!(
			documents_in(&read_manifest(&root).unwrap(), "Notes").len(),
			1,
			"the manifest has not been re-read"
		);

		let refreshed = documents_in(&refresh(&root).unwrap(), "Notes");
		assert_eq!(refreshed.len(), 2);
		assert_eq!(refreshed[1].title, "Ideas");
	}

	#[test]
	fn the_listing_commands_refuse_a_relative_path() {
		let relative = PathBuf::from("some/where");
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
		let tree = views(Path::new("/nowhere"), &manifest.nodes, "").0;

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

		let NodeView::Document {
			document: chapter, ..
		} = &children[0]
		else {
			panic!("the chapter sits inside the part");
		};
		assert_eq!(chapter.path, "Manuscript/Part One/Chapter 1.md");
		assert_eq!(chapter.title, "Chapter 1");
		assert_eq!(
			chapter.trail,
			["Manuscript", "Part One"],
			"a document's trail is every folder above it, from its section down"
		);
	}

	#[test]
	fn a_node_says_which_kind_it_is_on_the_wire() {
		let manifest = manifest_with(&["Notes/Notes.md"]);
		let tree = views(Path::new("/nowhere"), &manifest.nodes, "").0;
		let json = serde_json::to_value(tree).unwrap();

		let notes = &json.as_array().unwrap()[4];
		assert_eq!(notes["node"], "folder");
		assert_eq!(notes["kind"], serde_json::Value::Null);
		assert_eq!(notes["children"][0]["node"], "document");
		assert_eq!(notes["children"][0]["title"], "Notes");
		// The flag sits beside the document's own fields rather than under a
		// key of its own, which is the shape the sidebar reads.
		assert_eq!(notes["inBook"], true);
		assert_eq!(notes["children"][0]["inBook"], true);
	}

	#[test]
	fn the_tree_says_what_the_writer_has_taken_out_of_the_book() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manuscript = root.join("Manuscript");
		fs::write(manuscript.join("Kept.md"), "Sing to me.").unwrap();
		fs::write(
			manuscript.join("Cut.md"),
			"---\ninBook: false\n---\nSing to me.",
		)
		.unwrap();
		refresh_documents(root.clone()).unwrap();
		set_folder_fields(
			root.clone(),
			folder_id(&root, "Notes"),
			Fields::from([(IN_BOOK.to_owned(), tree::Value::Flag(false))]),
		)
		.unwrap();

		let tree = document_tree(root).unwrap();

		assert!(
			in_the_book(&tree, "Kept"),
			"a scene that says nothing is in"
		);
		assert!(!in_the_book(&tree, "Cut"));
		assert!(!in_the_book(&tree, "Notes"), "a folder carries it too");
		assert!(in_the_book(&tree, "Manuscript"));
	}

	/// Whether the node of this name, wherever it sits in the tree, is in the
	/// book.
	fn in_the_book(nodes: &[NodeView], name: &str) -> bool {
		found_in_book(nodes, name).expect("the node is somewhere in the tree")
	}

	fn found_in_book(nodes: &[NodeView], name: &str) -> Option<bool> {
		nodes.iter().find_map(|node| match node {
			NodeView::Folder {
				name: found,
				children,
				in_book,
				..
			} => (found == name)
				.then_some(*in_book)
				.or_else(|| found_in_book(children, name)),
			NodeView::Document {
				document, in_book, ..
			} => (document.title == name).then_some(*in_book),
		})
	}

	#[test]
	fn a_title_is_the_file_name_without_its_extension() {
		let document = Document::new("Manuscript", "Scene 1.md");
		assert_eq!(document.title(), "Scene 1");
		let view = DocumentView::from(&document);
		assert_eq!(view.title, "Scene 1");
		assert_eq!(view.trail, ["Manuscript"]);
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
				takes: Default::default(),
				chosen: None,
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
			root.join("Manuscript").join("Scene 1.md"),
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
		fs::remove_file(root.join("Manuscript").join("Scene 1.md")).unwrap();

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
		fs::write(root.join("Manuscript").join("Scene 1.md"), [0xff, 0xfe]).unwrap();

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

	/// Puts the file's modified time a minute back, so a later write is
	/// always later whatever the clock's grain.
	fn age(root: &Path, id: Uuid) {
		let path = resolve(&read_manifest(root).unwrap(), root, id).unwrap();
		let past = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
		fs::File::options()
			.write(true)
			.open(path)
			.unwrap()
			.set_modified(past)
			.unwrap();
	}

	#[test]
	fn a_save_with_the_time_it_read_goes_through() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;
		age(&root, id);

		let opened = open_document(root.clone(), id).unwrap();
		let seen = save_document(root.clone(), id, "Mine.".to_owned(), Some(opened.seen)).unwrap();

		assert!(seen > opened.seen);
		assert_eq!(read_document(root.clone(), id).unwrap(), "Mine.");
		save_document(root, id, "Again.".to_owned(), Some(seen)).unwrap();
	}

	#[test]
	fn a_save_over_a_changed_file_is_refused_and_leaves_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;
		age(&root, id);
		let opened = open_document(root.clone(), id).unwrap();

		write_document(root.clone(), id, "Theirs.".to_owned()).unwrap();
		let err =
			save_document(root.clone(), id, "Mine.".to_owned(), Some(opened.seen)).unwrap_err();

		assert!(matches!(err, Error::ChangedOnDisk));
		assert_eq!(read_document(root.clone(), id).unwrap(), "Theirs.");
		save_document(root.clone(), id, "Mine.".to_owned(), None).unwrap();
		assert_eq!(read_document(root, id).unwrap(), "Mine.");
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
		assert_eq!(left, ["Scene 1.md"]);
		assert_eq!(scan_paths(&root).len(), 3);
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
		fs::remove_file(root.join("Manuscript").join("Scene 1.md")).unwrap();

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
		fs::remove_file(root.join("Manuscript").join("Scene 1.md")).unwrap();
		(root, id)
	}

	#[test]
	fn a_vanished_document_can_be_put_back() {
		let parent = tempfile::tempdir().unwrap();
		let (root, id) = with_chapter_one_gone(&parent);

		let restored = restore_document(
			root.clone(),
			"Manuscript/Scene 1.md".to_owned(),
			"Sing to me of the man, Muse.".to_owned(),
		)
		.unwrap();

		assert_eq!(restored.id, id, "the manifest still knew the path");
		assert_eq!(restored.title, "Scene 1");
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
			"Manuscript/Scene 1.md".to_owned(),
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
			"Manuscript/Scene 1.md".to_owned(),
			"Back again.".to_owned(),
		)
		.unwrap();

		assert_eq!(restored.trail, ["Manuscript"]);
		assert_eq!(read_document(root, restored.id).unwrap(), "Back again.");
	}

	#[test]
	fn a_document_that_came_back_on_its_own_is_not_written_over() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(
			root.join("Manuscript").join("Scene 1.md"),
			"what the sync client brought back",
		)
		.unwrap();

		let err = restore_document(
			root.clone(),
			"Manuscript/Scene 1.md".to_owned(),
			"my copy".to_owned(),
		)
		.unwrap_err();

		assert!(matches!(err, Error::DocumentExists));
		assert_eq!(
			fs::read_to_string(root.join("Manuscript").join("Scene 1.md")).unwrap(),
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
			"Manuscript/.hidden.md",
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

		let err = restore_document(root.clone(), "Notes/Notes.md".to_owned(), "text".to_owned())
			.unwrap_err();
		assert!(matches!(err, Error::OutsideProject));
		assert!(!elsewhere.join("Notes.md").exists());

		let err =
			restore_document(root, "Notes/Sub/Notes.md".to_owned(), "text".to_owned()).unwrap_err();
		assert!(matches!(err, Error::OutsideProject));
		assert!(
			!elsewhere.join("Sub").exists(),
			"no folder is made at the end of the link"
		);
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

	/// `create_document` addressed by section name, which is how most of these
	/// tests were written before a parent was an id.
	fn create_in(root: PathBuf, section: &str, name: &str) -> Result<DocumentView> {
		let id = section_id(&root, section);
		create_document(root, id, name.to_owned())
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
			root.join("Manuscript").join("Scene 1.md"),
			"Sing to me of the man, Muse.",
		)
		.unwrap();

		let overview = cards(root, "Manuscript");

		assert_eq!(overview.len(), 1);
		assert_eq!(overview[0].document.title, "Scene 1");
		assert_eq!(overview[0].excerpt, "Sing to me of the man, Muse.");
		assert_eq!(overview[0].words, 7);
		assert!(overview[0].modified.is_some());
	}

	#[test]
	fn an_overview_counts_the_prose_and_not_the_front_matter() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(
			root.join("Manuscript").join("Scene 1.md"),
			"---\ntags:\n  - homecoming\n---\n\nSing to me of the man, Muse.",
		)
		.unwrap();

		let overview = cards(root, "Manuscript");

		assert_eq!(overview[0].excerpt, "Sing to me of the man, Muse.");
		assert_eq!(overview[0].words, 7);
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
		assert_eq!(titles, ["Scene 1", "Chapter 2", "Chapter 3"]);
	}

	#[test]
	fn an_overview_holds_only_its_own_section() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let overview = cards(root, "Notes");

		assert_eq!(overview.len(), 1);
		assert_eq!(overview[0].document.trail, ["Notes"]);
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
		assert!(matches!(err, Error::UnknownFolder));
	}

	#[test]
	fn an_overview_of_a_document_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let chapter = first_document(&root).id;

		let err = folder_overview(root, chapter).unwrap_err();
		assert!(matches!(err, Error::UnknownFolder));
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
			matches!(&overview[0], ChildSummary::Document(card) if card.document.title == "Scene 1"),
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
	fn a_folder_card_carries_its_target() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::create_dir(root.join("Manuscript").join("Part One")).unwrap();
		refresh(&root).unwrap();
		let part = folder_id(&root, "Manuscript/Part One");
		set_folder_target(root.clone(), part, Some(8_000)).unwrap();

		let overview = folder_overview(root.clone(), section_id(&root, "Manuscript")).unwrap();
		let target = overview.iter().find_map(|card| match card {
			ChildSummary::Folder { id, target, .. } if *id == part => Some(*target),
			_ => None,
		});
		assert_eq!(target, Some(Some(8_000)));
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
	fn a_folder_card_counts_every_word_beneath_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let chapter = root.join("Manuscript").join("Part One").join("Chapter 1");
		fs::create_dir_all(&chapter).unwrap();
		fs::write(chapter.join("Scene 2.md"), "Down to the sea again.").unwrap();
		fs::write(chapter.join("Scene 3.md"), "And the wind.").unwrap();
		fs::create_dir(root.join("Manuscript").join("Part Two")).unwrap();
		refresh(&root).unwrap();

		let id = section_id(&root, "Manuscript");
		let overview = folder_overview(root, id).unwrap();
		let words = |wanted: &str| {
			overview
				.iter()
				.find_map(|child| match child {
					ChildSummary::Folder { name, words, .. } if name == wanted => Some(*words),
					_ => None,
				})
				.expect("the overview holds that folder")
		};

		assert_eq!(
			words("Part One"),
			8,
			"a part's card reaches through the chapter to the scenes"
		);
		assert_eq!(
			words("Part Two"),
			0,
			"an empty folder reports nothing written, not nothing at all"
		);
	}

	#[test]
	fn the_tree_counts_words_at_every_level() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = root.join("Manuscript").join("Part One");
		fs::create_dir(&part).unwrap();
		fs::write(part.join("Chapter 2.md"), "Down to the sea.").unwrap();
		fs::write(root.join("Manuscript").join("Scene 1.md"), "Sing to me.").unwrap();
		refresh(&root).unwrap();

		let tree = document_tree(root).unwrap();
		let NodeView::Folder {
			name,
			children,
			words,
			..
		} = &tree[0]
		else {
			panic!("the Manuscript is a folder");
		};
		assert_eq!(name, "Manuscript");
		assert_eq!(
			*words, 7,
			"a section counts what its folders hold as well as its own documents"
		);

		let part = children
			.iter()
			.find_map(|node| match node {
				NodeView::Folder { name, words, .. } if name == "Part One" => Some(*words),
				_ => None,
			})
			.expect("the part is in the tree");
		assert_eq!(part, 4, "and a folder counts only what is under it");
	}

	#[test]
	fn a_count_follows_the_file_it_came_from() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let scene = root.join("Manuscript").join("Scene 1.md");
		fs::write(&scene, "Sing to me.").unwrap();
		refresh(&root).unwrap();

		let counted = |tree: &[NodeView]| match &tree[0] {
			NodeView::Folder { words, .. } => *words,
			_ => panic!("the Manuscript is a folder"),
		};

		assert_eq!(counted(&document_tree(root.clone()).unwrap()), 3);

		// Written behind the count, which is the whole point of the stat.
		fs::write(&scene, "Sing to me of the man of many turns.").unwrap();

		assert_eq!(
			counted(&document_tree(root).unwrap()),
			9,
			"a file written since it was counted is opened and counted again"
		);
	}

	#[test]
	fn a_summary_serializes_flat_alongside_the_document() {
		let document = Document::new("Manuscript", "Scene 1.md");
		let summary = DocumentSummary {
			document: (&document).into(),
			words: 3,
			excerpt: "Sing to me".to_owned(),
			front: String::new(),
			modified: Some(fixed_time()),
		};

		let json = serde_json::to_value(&summary).unwrap();
		assert_eq!(json["title"], "Scene 1");
		assert_eq!(json["path"], "Manuscript/Scene 1.md");
		assert_eq!(json["words"], 3);
		assert_eq!(json["modified"], "2023-11-14T22:13:20Z");
		assert!(json.get("document").is_none());
	}

	#[test]
	fn a_card_says_which_kind_it_is_on_the_wire() {
		let document = Document::new("Manuscript", "Scene 1.md");
		let json = serde_json::to_value(ChildSummary::Document(DocumentSummary::blank(&document)))
			.unwrap();
		assert_eq!(json["node"], "document");
		assert_eq!(json["title"], "Scene 1", "still flat under the tag");

		let json = serde_json::to_value(ChildSummary::Folder {
			id: Uuid::new_v4(),
			name: "Part One".to_owned(),
			kind: Some(FolderKind::Part),
			children: 2,
			words: 1200,
			target: None,
		})
		.unwrap();
		assert_eq!(json["node"], "folder");
		assert_eq!(json["kind"], "part");
		assert_eq!(json["children"], 2);
		assert_eq!(json["words"], 1200);
	}

	#[test]
	fn an_unreadable_summary_reports_no_time_at_all() {
		let document = Document::new("Manuscript", "Scene 1.md");
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
	fn a_front_matter_block_is_not_part_of_the_prose() {
		assert_eq!(body("---\ntags: []\n---\nSing to me."), "Sing to me.");
		assert_eq!(body("---\ntags: []\n---  \nSing to me."), "Sing to me.");
		assert_eq!(body("---\ntags: []\n---"), "");
	}

	#[test]
	fn a_front_matter_block_comes_back_with_its_fences() {
		assert_eq!(
			split("---\ntags: []\n---\nSing to me.").0,
			"---\ntags: []\n---"
		);
		assert_eq!(split("---\ntags: []\n---  \n").0, "---\ntags: []\n---");
		assert_eq!(split("Sing to me.").0, "");
		assert_eq!(split("---\ntags: []\n").0, "");
	}

	#[test]
	fn a_scene_break_at_the_top_is_not_a_front_matter_block() {
		let text = "---\n\ntext\n\n---\n\nmore";
		assert_eq!(split(text), ("", text));
		assert_eq!(body(text), text);
		assert_eq!(
			split("---\n# kept\ntags:\n  - a\n- b\n\n---\nProse").0,
			"---\n# kept\ntags:\n  - a\n- b\n\n---",
			"keys, lists and comments still make a block"
		);
	}

	#[test]
	fn a_fence_is_only_a_line_of_three_dashes() {
		// Neither of these closes the block early, and neither reads as front
		// matter, so the whole text stays prose rather than losing its tail.
		for text in [
			"---\ntags: []\n----\nsynopsis: x\n---\nProse",
			"---\ntags: []\n---note\n---\nProse",
		] {
			assert_eq!(split(text), ("", text));
		}
		assert_eq!(
			split("---\ntags: []\n---\t \nProse"),
			("---\ntags: []\n---", "Prose")
		);
		assert_eq!(
			split("---\r\ntags: []\r\n---\r\nProse"),
			("---\r\ntags: []\r\n---", "Prose")
		);
		assert_eq!(split("---\ntags: []\n---"), ("---\ntags: []\n---", ""));
	}

	#[test]
	fn a_document_is_in_the_book_unless_it_says_otherwise() {
		assert!(in_book("Sing to me."), "a file with no block is in");
		assert!(in_book("---\ntags: []\n---\nSing to me."));
		assert!(in_book("---\ninBook: true\n---\nSing to me."));
		assert!(!in_book("---\ninBook: false\n---\nSing to me."));
		assert!(!in_book("---\ntags: []\ninBook: no\n---\nSing to me."));
		assert!(!in_book("---\ninBook: \"false\"\n---\nSing to me."));
	}

	#[test]
	fn only_the_key_at_the_margin_takes_a_document_out() {
		assert!(
			in_book("---\nrelationships:\n  inBook: false\n---\nSing to me."),
			"a pair under another field belongs to that field"
		);
		assert!(
			in_book("---\ninBookish: false\n---\nSing to me."),
			"a longer name that starts the same way is another field"
		);
		assert!(
			in_book("Sing to me.\n\ninBook: false"),
			"the prose is prose, whatever it says"
		);
	}

	#[test]
	fn a_folder_carries_the_same_flag() {
		assert!(folder_in_book(&Fields::new()));
		assert!(folder_in_book(&Fields::from([(
			IN_BOOK.to_owned(),
			tree::Value::Text("true".to_owned())
		)])));
		assert!(!folder_in_book(&Fields::from([(
			IN_BOOK.to_owned(),
			tree::Value::Text("false".to_owned())
		)])));
		assert!(!folder_in_book(&Fields::from([(
			IN_BOOK.to_owned(),
			tree::Value::Flag(false)
		)])));
	}

	#[test]
	fn a_folder_keeps_a_switch_as_a_yes_or_no() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = folder_id(&root, "Manuscript");
		let said = Fields::from([(IN_BOOK.to_owned(), tree::Value::Flag(false))]);

		set_folder_fields(root.clone(), id, said.clone()).unwrap();

		assert_eq!(folder_fields(root.clone(), id).unwrap(), said);
		assert!(!folder_in_book(&folder_fields(root, id).unwrap()));
	}

	#[test]
	fn text_without_a_closed_block_is_all_prose() {
		assert_eq!(body("Sing to me."), "Sing to me.");
		assert_eq!(body("---\ntags: []\n"), "---\ntags: []\n");
		assert_eq!(body(""), "");
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

		let created = create_in(root.clone(), "Manuscript", "Chapter 2").unwrap();

		assert_eq!(created.title, "Chapter 2");
		assert_eq!(created.trail, ["Manuscript"]);
		assert_eq!(created.path, "Manuscript/Chapter 2.md");
		assert_eq!(read_document(root.clone(), created.id).unwrap(), "");

		let manuscript = documents_in(&read_manifest(&root).unwrap(), "Manuscript");
		let titles: Vec<_> = manuscript.iter().map(|d| &d.title).collect();
		assert_eq!(titles, ["Scene 1", "Chapter 2"]);
	}

	#[test]
	fn a_new_document_can_be_written_to_straight_away() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let created = create_in(root.clone(), "Notes", "Ideas").unwrap();
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
			root.join("Manuscript").join("Scene 1.md"),
			"Sing to me of the man, Muse.",
		)
		.unwrap();

		let err = create_in(root.clone(), "Manuscript", "Scene 1").unwrap_err();

		assert!(matches!(err, Error::DocumentExists));
		assert_eq!(
			fs::read_to_string(root.join("Manuscript").join("Scene 1.md")).unwrap(),
			"Sing to me of the man, Muse.",
			"the document that was already there is untouched"
		);
	}

	#[test]
	fn the_same_name_in_another_section_is_fine() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		create_in(root.clone(), "Notes", "Scene 1").unwrap();

		assert!(root.join("Notes").join("Scene 1.md").exists());
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
			let err = create_in(root.clone(), "Manuscript", name).unwrap_err();
			assert!(
				matches!(err, Error::InvalidName(_)),
				"{name:?} should not be a document name"
			);
		}

		assert_eq!(scan_paths(&root).len(), 3);
	}

	#[test]
	fn creating_somewhere_the_project_does_not_have_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = create_document(root.clone(), Uuid::new_v4(), "Offcut".to_owned()).unwrap_err();

		assert!(matches!(err, Error::UnknownFolder));
		assert_eq!(scan_paths(&root).len(), 3, "nothing was written");
	}

	/// The id of a folder at a path, for the tests that build a hierarchy.
	fn folder_id(root: &Path, path: &str) -> Uuid {
		let manifest = read_manifest(root).unwrap();
		tree::walk(&manifest.nodes)
			.find(|node| tree::path(&manifest.nodes, node.id()).as_deref() == Some(path))
			.expect("the project has that folder")
			.id()
	}

	#[test]
	fn a_part_and_a_chapter_inside_it_reach_the_folder_and_the_manifest() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let part = create_folder(
			root.clone(),
			section_id(&root, "Manuscript"),
			"Part One".to_owned(),
			Some(FolderKind::Part),
		)
		.unwrap();
		let NodeView::Folder { id, kind, .. } = part else {
			panic!("a folder was made");
		};
		assert_eq!(kind, Some(FolderKind::Part));

		create_folder(
			root.clone(),
			id,
			"Chapter 2".to_owned(),
			Some(FolderKind::Chapter),
		)
		.unwrap();

		assert!(root.join("Manuscript/Part One/Chapter 2").is_dir());
		let manifest = read_manifest(&root).unwrap();
		let chapter = folder_id(&root, "Manuscript/Part One/Chapter 2");
		assert_eq!(
			tree::path(&manifest.nodes, chapter).as_deref(),
			Some("Manuscript/Part One/Chapter 2")
		);
		assert_eq!(
			tree::parent(&manifest.nodes, chapter).map(Node::id),
			Some(id),
			"it sits inside the part, not beside it"
		);
	}

	#[test]
	fn a_document_can_be_made_inside_a_chapter() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let chapter = create_folder(
			root.clone(),
			section_id(&root, "Manuscript"),
			"Chapter 2".to_owned(),
			Some(FolderKind::Chapter),
		)
		.unwrap();
		let NodeView::Folder { id, .. } = chapter else {
			panic!("a folder was made");
		};

		let scene = create_document(root.clone(), id, "Scene 1".to_owned()).unwrap();

		assert_eq!(scene.path, "Manuscript/Chapter 2/Scene 1.md");
		assert!(root.join("Manuscript/Chapter 2/Scene 1.md").is_file());
	}

	#[test]
	fn a_created_folder_keeps_its_kind_and_its_id_through_a_refresh() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let made = create_folder(
			root.clone(),
			section_id(&root, "Manuscript"),
			"Part One".to_owned(),
			Some(FolderKind::Part),
		)
		.unwrap();
		let NodeView::Folder { id, .. } = made else {
			panic!("a folder was made");
		};

		let manifest = refresh(&root).unwrap();

		let Some(Node::Folder { kind, .. }) = tree::find(&manifest.nodes, id) else {
			panic!("the scan did not adopt it as something new");
		};
		assert_eq!(*kind, Some(FolderKind::Part));
	}

	/// Every combination the kind rules refuse, through the command that
	/// enforces them, with nothing left on disk afterwards.
	#[test]
	fn a_part_outside_the_manuscript_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = create_folder(
			root.clone(),
			section_id(&root, "Notes"),
			"Part One".to_owned(),
			Some(FolderKind::Part),
		)
		.unwrap_err();

		assert!(matches!(err, Error::FolderNotAllowed));
		assert!(!root.join("Notes/Part One").exists());
	}

	#[test]
	fn a_chapter_outside_the_manuscript_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = create_folder(
			root.clone(),
			section_id(&root, "Notes"),
			"Chapter 2".to_owned(),
			Some(FolderKind::Chapter),
		)
		.unwrap_err();

		assert!(matches!(err, Error::FolderNotAllowed));
	}

	#[test]
	fn a_folder_with_no_kind_inside_the_manuscript_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = create_folder(
			root.clone(),
			section_id(&root, "Manuscript"),
			"Scraps".to_owned(),
			None,
		)
		.unwrap_err();

		assert!(matches!(err, Error::FolderNotAllowed));
	}

	#[test]
	fn a_part_inside_a_part_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = a_part(&root);

		let err = create_folder(
			root.clone(),
			part,
			"Part Two".to_owned(),
			Some(FolderKind::Part),
		)
		.unwrap_err();

		assert!(matches!(err, Error::FolderNotAllowed));
	}

	#[test]
	fn a_folder_with_no_kind_inside_a_part_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = a_part(&root);

		let err = create_folder(root.clone(), part, "Scraps".to_owned(), None).unwrap_err();

		assert!(matches!(err, Error::FolderNotAllowed));
	}

	#[test]
	fn nothing_at_all_goes_inside_a_chapter() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = a_part(&root);
		let chapter = create_folder(
			root.clone(),
			part,
			"Chapter 2".to_owned(),
			Some(FolderKind::Chapter),
		)
		.unwrap()
		.id();

		for kind in [Some(FolderKind::Part), Some(FolderKind::Chapter), None] {
			let err = create_folder(root.clone(), chapter, "Inside".to_owned(), kind).unwrap_err();
			assert!(matches!(err, Error::FolderNotAllowed), "{kind:?}");
		}
		assert!(!root.join("Manuscript/Part One/Chapter 2/Inside").exists());
	}

	#[test]
	fn a_folder_beside_one_of_the_same_name_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		a_part(&root);

		let err = create_folder(
			root.clone(),
			section_id(&root, "Manuscript"),
			"Part One".to_owned(),
			Some(FolderKind::Part),
		)
		.unwrap_err();

		assert!(matches!(err, Error::AlreadyExists));
	}

	#[test]
	fn a_folder_inside_a_folder_is_allowed_outside_the_manuscript() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let research = create_folder(
			root.clone(),
			section_id(&root, "Notes"),
			"Research".to_owned(),
			None,
		)
		.unwrap()
		.id();

		create_folder(root.clone(), research, "Ships".to_owned(), None).unwrap();

		assert!(root.join("Notes/Research/Ships").is_dir());
	}

	#[test]
	fn a_folder_somewhere_the_project_does_not_have_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let chapter = first_document(&root).id;

		assert!(matches!(
			create_folder(root.clone(), Uuid::new_v4(), "Part One".to_owned(), None).unwrap_err(),
			Error::UnknownFolder
		));
		let inside_a_document =
			create_folder(root.clone(), chapter, "Part One".to_owned(), None).unwrap_err();
		assert!(
			matches!(inside_a_document, Error::UnknownFolder),
			"a document is not somewhere to put a folder"
		);
		assert!(matches!(
			create_folder(
				PathBuf::from("some/where"),
				Uuid::new_v4(),
				"Part One".to_owned(),
				None
			)
			.unwrap_err(),
			Error::RelativePath
		));
	}

	#[test]
	fn a_folder_name_is_held_to_the_same_rules_as_a_document() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = create_folder(
			root.clone(),
			section_id(&root, "Notes"),
			"Rese/arch".to_owned(),
			None,
		)
		.unwrap_err();

		assert!(matches!(
			err,
			Error::InvalidName(NameError::IllegalCharacter('/'))
		));
	}

	/// A part in the Manuscript, which most of the refusals need one of.
	fn a_part(root: &Path) -> Uuid {
		create_folder(
			root.to_path_buf(),
			section_id(root, "Manuscript"),
			"Part One".to_owned(),
			Some(FolderKind::Part),
		)
		.unwrap()
		.id()
	}

	#[test]
	fn creating_inside_a_document_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let chapter = first_document(&root).id;

		let err = create_document(root.clone(), chapter, "Offcut".to_owned()).unwrap_err();

		assert!(matches!(err, Error::UnknownFolder));
	}

	#[test]
	fn creating_recreates_a_section_folder_that_has_gone() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::remove_dir_all(root.join("Manuscript")).unwrap();

		let created = create_in(root.clone(), "Manuscript", "Chapter 2").unwrap();

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

		let err = create_in(root, "Notes", "Ideas").unwrap_err();

		assert!(matches!(err, Error::OutsideProject));
		assert!(!elsewhere.join("Ideas.md").exists());
	}

	#[test]
	fn creating_refuses_a_relative_path() {
		let err = create_document(
			PathBuf::from("some/where"),
			Uuid::new_v4(),
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
			let created = create_in(root.clone(), "Manuscript", typed).unwrap();
			assert_eq!(created.title, title, "typed {typed:?}");
			assert_eq!(created.path, format!("Manuscript/{title}.md"));
		}
	}

	#[test]
	fn a_name_that_is_nothing_but_the_extension_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = create_in(root.clone(), "Notes", ".md").unwrap_err();

		assert!(matches!(err, Error::InvalidName(NameError::Empty)));
		assert_eq!(scan_paths(&root).len(), 3);
	}

	#[test]
	fn a_renamed_document_keeps_its_id_its_text_and_its_place() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		create_in(root.clone(), "Manuscript", "Chapter 2").unwrap();
		let id = first_document(&root).id;
		write_document(root.clone(), id, "Sing to me of the man, Muse.".to_owned()).unwrap();

		let renamed = rename_document(root.clone(), id, "Ithaca Falls".to_owned()).unwrap();

		assert_eq!(renamed.id, id);
		assert_eq!(renamed.title, "Ithaca Falls");
		assert_eq!(renamed.trail, ["Manuscript"]);
		assert_eq!(renamed.path, "Manuscript/Ithaca Falls.md");
		assert_eq!(
			read_document(root.clone(), id).unwrap(),
			"Sing to me of the man, Muse."
		);

		let manuscript = documents_in(&read_manifest(&root).unwrap(), "Manuscript");
		let titles: Vec<_> = manuscript.iter().map(|d| &d.title).collect();
		assert_eq!(titles, ["Ithaca Falls", "Chapter 2"], "it did not move");
	}

	#[test]
	fn renaming_takes_the_old_file_with_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		rename_document(root.clone(), id, "Ithaca Falls".to_owned()).unwrap();

		assert!(!root.join("Manuscript").join("Scene 1.md").exists());
		assert_eq!(scan_paths(&root)[0], "Manuscript/Ithaca Falls.md");
	}

	#[test]
	fn renaming_to_the_name_it_already_has_changes_nothing() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		let renamed = rename_document(root.clone(), id, "Scene 1".to_owned()).unwrap();

		assert_eq!(renamed.id, id);
		assert_eq!(renamed.path, "Manuscript/Scene 1.md");
		assert!(root.join("Manuscript").join("Scene 1.md").exists());
	}

	#[test]
	fn renaming_onto_another_document_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let other = create_in(root.clone(), "Manuscript", "Chapter 2").unwrap();
		write_document(root.clone(), other.id, "the second chapter".to_owned()).unwrap();
		let id = first_document(&root).id;

		let err = rename_document(root.clone(), id, "Chapter 2".to_owned()).unwrap_err();

		assert!(matches!(err, Error::DocumentExists));
		assert_eq!(
			read_document(root.clone(), other.id).unwrap(),
			"the second chapter",
			"the document that was already there is untouched"
		);
		assert_eq!(first_document(&root).path, "Manuscript/Scene 1.md");
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

		assert_eq!(first_document(&root).path, "Manuscript/Scene 1.md");
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

	/// A part holding a chapter, the chapter holding a scene, and a scene loose
	/// in the part beside it. Renaming the part has to leave all three where
	/// they were.
	fn with_a_part(root: &Path) -> Uuid {
		let part = create_folder(
			root.to_path_buf(),
			section_id(root, "Manuscript"),
			"Part One".to_owned(),
			Some(FolderKind::Part),
		)
		.unwrap()
		.id();
		let chapter = create_folder(
			root.to_path_buf(),
			part,
			"Chapter 2".to_owned(),
			Some(FolderKind::Chapter),
		)
		.unwrap()
		.id();
		create_document(root.to_path_buf(), chapter, "Landfall".to_owned()).unwrap();
		create_document(root.to_path_buf(), part, "Prologue".to_owned()).unwrap();
		part
	}

	/// The id of the document sitting at this path, for the tests whose document
	/// is not the first one in the project.
	fn document_id(root: &Path, path: &str) -> Uuid {
		read_manifest(root)
			.unwrap()
			.documents()
			.iter()
			.find(|document| document.path == path)
			.expect("the project has that document")
			.id
	}

	#[test]
	fn a_renamed_folder_keeps_every_descendant_where_it_was() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		let chapter = folder_id(&root, "Manuscript/Part One/Chapter 2");
		let scene = document_id(&root, "Manuscript/Part One/Chapter 2/Landfall.md");
		write_document(root.clone(), scene, "The ship came in at dusk.".to_owned()).unwrap();

		let renamed = rename_folder(root.clone(), part, "Part Two".to_owned()).unwrap();

		let NodeView::Folder { id, name, kind, .. } = &renamed else {
			panic!("a folder was renamed");
		};
		assert_eq!(*id, part, "the folder is the same folder");
		assert_eq!(name, "Part Two");
		assert_eq!(*kind, Some(FolderKind::Part));

		let manifest = read_manifest(&root).unwrap();
		assert_eq!(
			tree::path(&manifest.nodes, chapter).as_deref(),
			Some("Manuscript/Part Two/Chapter 2"),
			"the chapter came along and kept its id"
		);
		assert_eq!(
			tree::path(&manifest.nodes, scene).as_deref(),
			Some("Manuscript/Part Two/Chapter 2/Landfall.md")
		);
		assert_eq!(
			read_document(root.clone(), scene).unwrap(),
			"The ship came in at dusk.",
			"the scene is still readable through its id"
		);
		assert_eq!(
			tree::path(&manifest.nodes, part).as_deref(),
			Some("Manuscript/Part Two")
		);
	}

	#[test]
	fn renaming_takes_the_directory_and_all_of_it_with_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);

		rename_folder(root.clone(), part, "Part Two".to_owned()).unwrap();

		assert!(!root.join("Manuscript/Part One").exists());
		assert!(
			root.join("Manuscript/Part Two/Chapter 2/Landfall.md")
				.is_file()
		);
		assert!(root.join("Manuscript/Part Two/Prologue.md").is_file());
	}

	#[test]
	fn a_renamed_folder_still_answers_to_its_id_after_a_refresh() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		let chapter = folder_id(&root, "Manuscript/Part One/Chapter 2");

		rename_folder(root.clone(), part, "Part Two".to_owned()).unwrap();
		let manifest = refresh(&root).unwrap();

		assert_eq!(
			tree::path(&manifest.nodes, chapter).as_deref(),
			Some("Manuscript/Part Two/Chapter 2"),
			"looking at the folder again found what the manifest already said"
		);
	}

	#[test]
	fn renaming_a_folder_onto_a_sibling_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		create_folder(
			root.clone(),
			section_id(&root, "Manuscript"),
			"Part Two".to_owned(),
			Some(FolderKind::Part),
		)
		.unwrap();

		let err = rename_folder(root.clone(), part, "Part Two".to_owned()).unwrap_err();

		assert!(matches!(err, Error::AlreadyExists));
		assert!(root.join("Manuscript/Part One/Chapter 2").is_dir());
		assert_eq!(
			tree::path(&read_manifest(&root).unwrap().nodes, part).as_deref(),
			Some("Manuscript/Part One")
		);
	}

	#[test]
	fn a_folder_of_that_name_elsewhere_is_not_a_collision() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		create_folder(
			root.clone(),
			section_id(&root, "Notes"),
			"Part Two".to_owned(),
			None,
		)
		.unwrap();

		rename_folder(root.clone(), part, "Part Two".to_owned()).unwrap();

		assert!(root.join("Manuscript/Part Two").is_dir());
		assert!(root.join("Notes/Part Two").is_dir());
	}

	#[test]
	fn renaming_a_folder_to_the_name_it_already_has_changes_nothing() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);

		let renamed = rename_folder(root.clone(), part, "Part One".to_owned()).unwrap();

		assert_eq!(renamed.id(), part);
		assert!(root.join("Manuscript/Part One/Chapter 2").is_dir());
	}

	#[test]
	fn a_section_cannot_be_renamed() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manuscript = section_id(&root, "Manuscript");

		let err = rename_folder(root.clone(), manuscript, "Novel".to_owned()).unwrap_err();

		assert!(matches!(err, Error::SectionFixed));
		assert!(root.join("Manuscript").is_dir());
		assert!(!root.join("Novel").exists());
	}

	#[test]
	fn renaming_a_folder_to_a_name_the_filesystem_would_not_take_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);

		for name in ["", "  ", "Part/Two", "Part?", ".hidden", "NUL"] {
			let err = rename_folder(root.clone(), part, name.to_owned()).unwrap_err();
			assert!(
				matches!(err, Error::InvalidName(_)),
				"{name:?} should not be a folder name"
			);
		}

		assert!(root.join("Manuscript/Part One").is_dir());
	}

	#[test]
	fn renaming_a_document_as_a_folder_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let scene = first_document(&root).id;

		assert!(matches!(
			rename_folder(root.clone(), scene, "Part One".to_owned()).unwrap_err(),
			Error::UnknownFolder
		));
		assert!(matches!(
			rename_folder(root.clone(), Uuid::new_v4(), "Part One".to_owned()).unwrap_err(),
			Error::UnknownFolder
		));
	}

	#[test]
	fn renaming_a_folder_refuses_a_relative_path() {
		let err = rename_folder(
			PathBuf::from("some/where"),
			Uuid::new_v4(),
			"Part Two".to_owned(),
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
		let listed = documents_in(&read_manifest(&root).unwrap(), "Manuscript");
		assert_eq!(listed[0].target, Some(1_500));
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
		let document = Document::new("Manuscript", "Scene 1.md");
		let json = serde_json::to_value(&document).unwrap();
		assert!(json.get("target").is_none());
	}

	#[test]
	fn a_manifest_written_before_targets_still_loads() {
		let json = serde_json::json!({
			"id": Uuid::new_v4().to_string(),
			"path": "Manuscript/Scene 1.md",
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
	fn a_folder_is_written_towards_a_target_of_its_own() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manuscript = folder_id(&root, "Manuscript");
		// The section is seeded with an empty document, so everything counted
		// here is what this writes.
		fs::write(
			root.join("Manuscript").join("Chapter 1.md"),
			"Elena andava a scuola\n",
		)
		.unwrap();
		refresh_documents(root.clone()).unwrap();

		set_folder_target(root.clone(), manuscript, Some(90_000)).unwrap();

		let progress = folder_progress(root.clone(), manuscript).unwrap();
		assert_eq!(progress.target, Some(90_000));
		assert_eq!(progress.words, 4);
		// A refresh reads the directories again, and a directory says nothing
		// about itself, so this is the pass that would lose it.
		refresh_documents(root.clone()).unwrap();
		assert_eq!(
			folder_progress(root, manuscript).unwrap().target,
			Some(90_000)
		);
	}

	#[test]
	fn the_tree_carries_a_folder_target_and_a_document_count() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manuscript = folder_id(&root, "Manuscript");
		fs::write(
			root.join("Manuscript").join("Chapter 1.md"),
			"Elena andava a scuola\n",
		)
		.unwrap();
		refresh_documents(root.clone()).unwrap();
		set_folder_target(root.clone(), manuscript, Some(90_000)).unwrap();

		let tree = document_tree(root).unwrap();
		let Some(NodeView::Folder {
			target, children, ..
		}) = tree.iter().find(|node| node.id() == manuscript)
		else {
			panic!("no Manuscript in the tree");
		};
		assert_eq!(*target, Some(90_000));
		let words = children.iter().find_map(|node| match node {
			NodeView::Document {
				document, words, ..
			} if document.path == "Manuscript/Chapter 1.md" => Some(*words),
			_ => None,
		});
		assert_eq!(words, Some(4));
	}

	#[test]
	fn a_folder_counts_the_documents_below_its_own_folders() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let deep = root.join("Manuscript").join("Part 1").join("Chapter 2");
		fs::create_dir_all(&deep).unwrap();
		fs::write(deep.join("Scene 2.md"), "one two three").unwrap();
		fs::write(root.join("Manuscript").join("Chapter 1.md"), "four five").unwrap();
		refresh_documents(root.clone()).unwrap();

		let manuscript = folder_id(&root, "Manuscript");
		let part = folder_id(&root, "Manuscript/Part 1");

		assert_eq!(folder_progress(root.clone(), manuscript).unwrap().words, 5);
		assert_eq!(folder_progress(root, part).unwrap().words, 3);
	}

	/// Writes a folder of that name inside `parent` with one document in it,
	/// and brings the manifest up to date.
	fn matter_folder(root: &Path, parent: &str, name: &str, text: &str) {
		let dir = root.join(parent).join(name);
		fs::create_dir_all(&dir).unwrap();
		fs::write(dir.join("Note.md"), text).unwrap();
		refresh_documents(root.to_path_buf()).unwrap();
	}

	/// The kind of the first folder of that name anywhere in the tree. The
	/// outer `Option` is whether it was found at all.
	fn folder_kind(nodes: &[NodeView], name: &str) -> Option<Option<FolderKind>> {
		nodes.iter().find_map(|node| match node {
			NodeView::Folder {
				name: found,
				kind,
				children,
				..
			} => (found == name)
				.then_some(*kind)
				.or_else(|| folder_kind(children, name)),
			NodeView::Document { .. } => None,
		})
	}

	/// What the first folder of that name holds, for looking inside one section
	/// when another has a folder of the same name.
	fn folder_children(nodes: &[NodeView], name: &str) -> Vec<NodeView> {
		nodes
			.iter()
			.find_map(|node| match node {
				NodeView::Folder {
					name: found,
					children,
					..
				} => (found == name).then(|| children.clone()),
				NodeView::Document { .. } => None,
			})
			.unwrap_or_default()
	}

	/// The words on the first folder of that name anywhere in the tree.
	fn folder_words(nodes: &[NodeView], name: &str) -> Option<usize> {
		nodes.iter().find_map(|node| match node {
			NodeView::Folder {
				name: found,
				words,
				children,
				..
			} => (found == name)
				.then_some(*words)
				.or_else(|| folder_words(children, name)),
			NodeView::Document { .. } => None,
		})
	}

	#[test]
	fn the_manuscript_leaves_its_front_and_back_matter_out_of_its_count() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Manuscript").join("Chapter 1.md"), "one two").unwrap();
		matter_folder(&root, "Manuscript", "Front Matter", "three four five");
		matter_folder(&root, "Manuscript", "Back Matter", "six seven");

		let manuscript = folder_id(&root, "Manuscript");
		let front = folder_id(&root, "Manuscript/Front Matter");
		let back = folder_id(&root, "Manuscript/Back Matter");

		assert_eq!(folder_progress(root.clone(), manuscript).unwrap().words, 2);
		// Asked about directly, each still says what it holds.
		assert_eq!(folder_progress(root.clone(), front).unwrap().words, 3);
		assert_eq!(folder_progress(root, back).unwrap().words, 2);
	}

	#[test]
	fn the_tree_keeps_the_matter_counts_on_their_own_rows() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Manuscript").join("Chapter 1.md"), "one two").unwrap();
		matter_folder(&root, "Manuscript", "Front Matter", "three four five");

		let tree = document_tree(root).unwrap();

		assert_eq!(folder_words(&tree, "Manuscript"), Some(2));
		assert_eq!(folder_words(&tree, "Front Matter"), Some(3));
	}

	#[test]
	fn front_matter_outside_the_manuscript_is_an_ordinary_folder() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		matter_folder(&root, "Notes", "Front Matter", "one two three");

		assert_eq!(
			folder_progress(root.clone(), folder_id(&root, "Notes"))
				.unwrap()
				.words,
			3
		);
		assert_eq!(
			folder_words(&document_tree(root).unwrap(), "Notes"),
			Some(3)
		);
	}

	#[test]
	fn a_folder_called_front_matter_deeper_in_the_manuscript_still_counts() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		matter_folder(&root, "Manuscript/Part 1", "Front Matter", "one two three");
		let manuscript = folder_id(&root, "Manuscript");

		assert_eq!(folder_progress(root, manuscript).unwrap().words, 3);
	}

	#[test]
	fn the_manuscript_takes_a_matter_folder() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manuscript = folder_id(&root, "Manuscript");

		let view = create_folder(
			root,
			manuscript,
			"Front Matter".to_owned(),
			Some(FolderKind::FrontMatter),
		)
		.unwrap();

		let NodeView::Folder { kind, .. } = view else {
			panic!("a folder went in");
		};
		assert_eq!(kind, Some(FolderKind::FrontMatter));
	}

	#[test]
	fn the_manuscript_takes_only_one_of_each_matter() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manuscript = folder_id(&root, "Manuscript");
		let front = Some(FolderKind::FrontMatter);
		create_folder(root.clone(), manuscript, "Front Matter".to_owned(), front).unwrap();

		// A different name, so it is the kind that is refused and not the name.
		let refused = create_folder(root.clone(), manuscript, "Prelims".to_owned(), front);

		assert!(matches!(refused, Err(Error::FolderNotAllowed)));
		// The other one is still free.
		create_folder(
			root,
			manuscript,
			"Back Matter".to_owned(),
			Some(FolderKind::BackMatter),
		)
		.unwrap();
	}

	#[test]
	fn a_part_takes_no_matter_folder() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manuscript = folder_id(&root, "Manuscript");
		let part = create_folder(
			root.clone(),
			manuscript,
			"Part 1".to_owned(),
			Some(FolderKind::Part),
		)
		.unwrap();

		let refused = create_folder(
			root,
			part.id(),
			"Front Matter".to_owned(),
			Some(FolderKind::FrontMatter),
		);

		assert!(matches!(refused, Err(Error::FolderNotAllowed)));
	}

	#[test]
	fn a_matter_folder_made_outside_aurora_is_adopted() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		matter_folder(&root, "Manuscript", "Front Matter", "one two three");
		matter_folder(&root, "Manuscript", "Back Matter", "four five");
		matter_folder(&root, "Notes", "Front Matter", "six");

		let tree = document_tree(root).unwrap();

		assert_eq!(
			folder_kind(&tree, "Front Matter"),
			Some(Some(FolderKind::FrontMatter))
		);
		assert_eq!(
			folder_kind(&tree, "Back Matter"),
			Some(Some(FolderKind::BackMatter))
		);
		let notes = folder_children(&tree, "Notes");
		assert_eq!(folder_kind(&notes, "Front Matter"), Some(None));
	}

	#[test]
	fn a_chapter_the_writer_called_front_matter_stays_a_chapter() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manuscript = folder_id(&root, "Manuscript");
		create_folder(
			root.clone(),
			manuscript,
			"Front Matter".to_owned(),
			Some(FolderKind::Chapter),
		)
		.unwrap();
		fs::write(
			root.join("Manuscript/Front Matter").join("Scene.md"),
			"one two three",
		)
		.unwrap();
		refresh_documents(root.clone()).unwrap();

		let tree = document_tree(root).unwrap();

		assert_eq!(
			folder_kind(&tree, "Front Matter"),
			Some(Some(FolderKind::Chapter))
		);
		assert_eq!(folder_words(&tree, "Manuscript"), Some(3));
	}

	#[test]
	fn a_renamed_matter_folder_is_still_matter() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Manuscript").join("Chapter 1.md"), "one two").unwrap();
		matter_folder(&root, "Manuscript", "Front Matter", "three four five");
		let front = folder_id(&root, "Manuscript/Front Matter");
		rename_folder(root.clone(), front, "Prelims".to_owned()).unwrap();

		let manuscript = folder_id(&root, "Manuscript");

		assert_eq!(folder_progress(root, manuscript).unwrap().words, 2);
	}

	#[test]
	fn aiming_a_folder_at_no_words_is_not_aiming() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = folder_id(&root, "Notes");

		set_folder_target(root.clone(), id, Some(0)).unwrap();

		assert_eq!(folder_progress(root, id).unwrap().target, None);
	}

	#[test]
	fn aiming_an_unknown_folder_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manifest = read_manifest(&root).unwrap();
		let scene = documents_in(&manifest, "Manuscript")[0].id;

		assert!(matches!(
			set_folder_target(root.clone(), Uuid::new_v4(), Some(100)).unwrap_err(),
			Error::UnknownFolder
		));
		// A document has a target of its own, set by its own command.
		assert!(matches!(
			folder_progress(root, scene).unwrap_err(),
			Error::UnknownFolder
		));
	}

	#[test]
	fn a_folder_keeps_what_the_writer_said_about_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = folder_id(&root, "Manuscript");

		let said = Fields::from([
			(
				"synopsis".to_owned(),
				Value::Text("The long way home.".to_owned()),
			),
			(
				"tags".to_owned(),
				Value::List(vec!["draft".to_owned(), "Elena".to_owned()]),
			),
		]);
		set_folder_fields(root.clone(), id, said.clone()).unwrap();

		assert_eq!(folder_fields(root.clone(), id).unwrap(), said);
		// A refresh reads the directories again, and a directory says nothing
		// about itself, so this is the pass that would lose them.
		refresh_documents(root.clone()).unwrap();
		assert_eq!(folder_fields(root, id).unwrap(), said);
	}

	#[test]
	fn one_folder_does_not_answer_for_another() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manuscript = folder_id(&root, "Manuscript");
		let notes = folder_id(&root, "Notes");

		set_folder_fields(
			root.clone(),
			manuscript,
			Fields::from([("synopsis".to_owned(), Value::Text("Hers.".to_owned()))]),
		)
		.unwrap();

		assert!(folder_fields(root, notes).unwrap().is_empty());
	}

	#[test]
	fn saying_something_about_an_unknown_folder_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = set_folder_fields(root, Uuid::new_v4(), Fields::new()).unwrap_err();

		assert!(matches!(err, Error::UnknownFolder));
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

		assert_eq!(trashed(&root, "Manuscript"), ["20231114-221320 Scene 1.md"]);
		assert_eq!(
			fs::read_to_string(
				root.join(TRASH_DIR)
					.join("Manuscript")
					.join("20231114-221320 Scene 1.md")
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

		assert!(!root.join("Manuscript").join("Scene 1.md").exists());
		assert!(
			!read_manifest(&root)
				.unwrap()
				.documents()
				.iter()
				.any(|d| d.id == id)
		);
		assert!(documents_in(&read_manifest(&root).unwrap(), "Manuscript").is_empty());
	}

	#[test]
	fn the_trash_is_invisible_to_a_refresh() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		trash(&root, id, fixed_time()).unwrap();
		refresh(&root).unwrap();

		assert_eq!(scan_paths(&root).len(), 2);
		assert_eq!(read_manifest(&root).unwrap().documents().len(), 2);
		assert!(trashed(&root, "Manuscript").len() == 1, "it is still there");
	}

	#[test]
	fn deleting_two_documents_of_the_same_name_keeps_both() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		for text in ["the first one", "the second one"] {
			let made = create_in(root.clone(), "Notes", "Ideas").unwrap();
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
		let notes = create_in(root.clone(), "Notes", "Scene 1").unwrap();
		let manuscript = first_document(&root).id;

		trash(&root, manuscript, fixed_time()).unwrap();
		trash(&root, notes.id, fixed_time()).unwrap();

		assert_eq!(trashed(&root, "Manuscript"), ["20231114-221320 Scene 1.md"]);
		assert_eq!(trashed(&root, "Notes"), ["20231114-221320 Scene 1.md"]);
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
		assert!(root.join("Manuscript").join("Scene 1.md").exists());
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
		let (at, was) = unstamp("20231114-221320 Scene 1.md").unwrap();
		assert_eq!(at, fixed_time());
		assert_eq!(was, "Scene 1.md");

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
		assert_eq!(listed[0].title, "Scene 1");
		assert_eq!(listed[0].folder, "Manuscript");
		assert_eq!(listed[0].path, "Manuscript/20231114-221320 Scene 1.md");
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
		let second = create_in(root.clone(), "Notes", "Ideas").unwrap();
		trash(&root, second.id, later).unwrap();
		// Something a writer dropped in by hand.
		fs::write(root.join(TRASH_DIR).join("Notes").join("Stray.md"), "").unwrap();

		let listed = list_trash(root).unwrap();

		let titles: Vec<_> = listed.iter().map(|e| e.title.as_str()).collect();
		assert_eq!(titles, ["Ideas", "Scene 1", "Stray"]);
		assert!(listed[2].deleted.is_none());
	}

	#[test]
	fn a_deleted_document_can_be_put_back_where_it_was() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_chapter_one_deleted(&parent);
		let entry = list_trash(root.clone()).unwrap().remove(0);

		restore_from_trash(root.clone(), entry.path).unwrap();

		let back = first_document(&root);
		assert_eq!(back.path, "Manuscript/Scene 1.md");
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
		create_in(root.clone(), "Manuscript", "Scene 1").unwrap();
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

		restore_from_trash(root.clone(), entry.path).unwrap();

		let back = first_document(&root);
		assert_eq!(back.path, "Manuscript/Scene 1.md");
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
		assert_eq!(scan_paths(&root).len(), 2);
	}

	#[test]
	fn a_deleted_folder_is_one_entry_saying_what_is_inside_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		with_a_part(&root);
		let chapter = folder_id(&root, "Manuscript/Part One/Chapter 2");
		create_document(root.clone(), chapter, "Storm".to_owned()).unwrap();

		trash(&root, chapter, fixed_time()).unwrap();

		let listed = list_trash(root.clone()).unwrap();
		assert_eq!(listed.len(), 1, "a chapter of scenes is one row");
		assert_eq!(listed[0].title, "Chapter 2");
		assert_eq!(listed[0].folder, "Manuscript");
		assert_eq!(listed[0].inside, Some(2));
		assert!(listed[0].deleted.is_some());
		assert!(
			tree::find(&read_manifest(&root).unwrap().nodes, chapter).is_none(),
			"and one node left the manifest"
		);
	}

	#[test]
	fn the_trash_mirrors_the_project_it_deleted_from() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		with_a_part(&root);
		let scene = document_id(&root, "Manuscript/Part One/Chapter 2/Landfall.md");

		trash(&root, scene, fixed_time()).unwrap();

		let listed = list_trash(root.clone()).unwrap();
		assert_eq!(
			listed[0].path, "Manuscript/Part One/Chapter 2/20231114-221320 Landfall.md",
			"it sits at the path it was deleted from"
		);
		assert_eq!(listed[0].title, "Landfall");
		assert_eq!(listed[0].inside, None);
		assert!(
			!root
				.join("Manuscript/Part One/Chapter 2/Landfall.md")
				.exists()
		);
	}

	#[test]
	fn a_chapter_put_back_keeps_its_kind_target_and_scene_order() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		with_a_part(&root);
		let chapter = folder_id(&root, "Manuscript/Part One/Chapter 2");
		let storm = create_document(root.clone(), chapter, "Storm".to_owned())
			.unwrap()
			.id;
		set_folder_target(root.clone(), chapter, Some(500)).unwrap();
		move_node(root.clone(), storm, chapter, 0).unwrap();
		trash(&root, chapter, fixed_time()).unwrap();
		let entry = list_trash(root.clone()).unwrap().remove(0);

		restore_from_trash(root.clone(), entry.path).unwrap();

		let manifest = read_manifest(&root).unwrap();
		let Some(Node::Folder {
			id,
			kind,
			target,
			children,
			..
		}) = tree::find(&manifest.nodes, chapter)
		else {
			panic!("the chapter came back under its own id");
		};
		assert_eq!(*id, chapter);
		assert_eq!(*kind, Some(FolderKind::Chapter));
		assert_eq!(*target, Some(500));
		assert_eq!(
			children[0].name(),
			"Storm.md",
			"the scenes keep their order"
		);
	}

	#[test]
	fn a_scene_put_back_leaves_no_empty_folder_in_the_trash() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		with_a_part(&root);
		let scene = document_id(&root, "Manuscript/Part One/Chapter 2/Landfall.md");
		trash(&root, scene, fixed_time()).unwrap();
		let entry = list_trash(root.clone()).unwrap().remove(0);

		restore_from_trash(root.clone(), entry.path).unwrap();

		let left: Vec<_> = fs::read_dir(root.join(TRASH_DIR))
			.unwrap()
			.map(|entry| entry.unwrap().file_name())
			.collect();
		assert!(
			!left.iter().any(|name| name == "Manuscript"),
			"the folders it sat in are gone from the trash: {left:?}"
		);
		assert!(
			fs::read_dir(root.join(TRASH_DIR).join(NODES_DIR))
				.unwrap()
				.next()
				.is_none(),
			"and so are the records"
		);
	}

	#[test]
	fn putting_a_folder_back_returns_the_whole_of_it_where_it_was() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		let chapter = folder_id(&root, "Manuscript/Part One/Chapter 2");
		create_document(root.clone(), chapter, "Storm".to_owned()).unwrap();
		let landfall = document_id(&root, "Manuscript/Part One/Chapter 2/Landfall.md");
		write_document(root.clone(), landfall, "The ship came in.".to_owned()).unwrap();
		trash(&root, chapter, fixed_time()).unwrap();
		let entry = list_trash(root.clone()).unwrap().remove(0);

		restore_from_trash(root.clone(), entry.path).unwrap();

		let manifest = read_manifest(&root).unwrap();
		assert_eq!(
			paths_of(&manifest)
				.iter()
				.filter(|path| path.contains("Chapter 2"))
				.collect::<Vec<_>>(),
			[
				"Manuscript/Part One/Chapter 2/Landfall.md",
				"Manuscript/Part One/Chapter 2/Storm.md"
			],
			"both scenes came back inside the chapter, inside the part"
		);
		assert_eq!(
			read_document(
				root.clone(),
				document_id(&root, "Manuscript/Part One/Chapter 2/Landfall.md")
			)
			.unwrap(),
			"The ship came in."
		);
		assert!(
			tree::children(&manifest.nodes, part).unwrap().len() == 2,
			"the part holds the chapter and its own loose scene"
		);
		assert!(list_trash(root).unwrap().is_empty(), "it left the trash");
	}

	#[test]
	fn a_folder_whose_parent_has_gone_too_comes_back_to_the_nearest_one_left() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		let chapter = folder_id(&root, "Manuscript/Part One/Chapter 2");

		// The chapter first, then the part it was in.
		trash(&root, chapter, fixed_time()).unwrap();
		trash(&root, part, fixed_time()).unwrap();
		let entry = list_trash(root.clone())
			.unwrap()
			.into_iter()
			.find(|e| e.title == "Chapter 2")
			.expect("the chapter is in the trash on its own");

		restore_from_trash(root.clone(), entry.path).unwrap();

		let manifest = read_manifest(&root).unwrap();
		assert_eq!(
			tree::path(&manifest.nodes, folder_id(&root, "Manuscript/Chapter 2")).as_deref(),
			Some("Manuscript/Chapter 2"),
			"the part is gone, so the Manuscript takes it"
		);
		assert!(root.join("Manuscript/Chapter 2/Landfall.md").is_file());
	}

	#[test]
	fn purging_a_folder_takes_everything_that_went_in_with_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		trash(&root, part, fixed_time()).unwrap();
		let entry = list_trash(root.clone()).unwrap().remove(0);

		purge_trash_entry(root.clone(), entry.path).unwrap();

		assert!(list_trash(root.clone()).unwrap().is_empty());
		assert!(!root.join(TRASH_DIR).join("Manuscript/Part One").exists());
	}

	#[test]
	fn a_section_cannot_be_deleted() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = delete_folder(root.clone(), section_id(&root, "Notes")).unwrap_err();

		assert!(matches!(err, Error::SectionFixed));
		assert!(root.join("Notes").is_dir());
	}

	#[test]
	fn each_delete_command_refuses_the_other_kind() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		let scene = first_document(&root).id;

		assert!(matches!(
			delete_document(root.clone(), part).unwrap_err(),
			Error::UnknownDocument
		));
		assert!(matches!(
			delete_folder(root.clone(), scene).unwrap_err(),
			Error::UnknownFolder
		));
		assert!(root.join("Manuscript/Part One").is_dir());
		assert!(root.join("Manuscript/Scene 1.md").is_file());
	}

	#[test]
	fn two_folders_of_one_name_deleted_together_both_survive() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let notes = section_id(&root, "Notes");
		let first = create_folder(root.clone(), notes, "Research".to_owned(), None)
			.unwrap()
			.id();

		trash(&root, first, fixed_time()).unwrap();
		let again = create_folder(root.clone(), notes, "Research".to_owned(), None)
			.unwrap()
			.id();
		trash(&root, again, fixed_time()).unwrap();

		let listed = list_trash(root).unwrap();
		assert_eq!(listed.len(), 2, "the second did not write over the first");
		assert!(listed.iter().all(|entry| entry.title == "Research"));
	}

	#[test]
	fn the_trash_commands_refuse_an_entry_that_is_not_there() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let path = "Manuscript/20231114-221320 Scene 1.md".to_owned();

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
			create_in(root.clone(), "Manuscript", name).unwrap();
		}
		root
	}

	/// The titles in one section, in the order the manifest records them.
	fn order_of(root: &Path, folder: &str) -> Vec<String> {
		documents_in(&read_manifest(root).unwrap(), folder)
			.iter()
			.map(|d| d.title.clone())
			.collect()
	}

	/// `move_node` inside a node's own folder, which is what reordering is.
	fn reorder_in(root: &Path, id: Uuid, index: usize) -> Result<()> {
		let manifest = read_manifest(root).unwrap();
		let parent = tree::parent(&manifest.nodes, id)
			.map(Node::id)
			.expect("the node sits in a folder");
		move_node(root.to_path_buf(), id, parent, index)
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

		reorder_in(&root, chapter(&root, "Chapter 3"), 0).unwrap();

		assert_eq!(
			order_of(&root, "Manuscript"),
			["Chapter 3", "Scene 1", "Chapter 2"]
		);
	}

	#[test]
	fn a_document_can_be_moved_down_its_section() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);

		reorder_in(&root, chapter(&root, "Scene 1"), 1).unwrap();

		assert_eq!(
			order_of(&root, "Manuscript"),
			["Chapter 2", "Scene 1", "Chapter 3"]
		);
	}

	#[test]
	fn an_index_past_the_end_means_the_end() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);

		reorder_in(&root, chapter(&root, "Scene 1"), 99).unwrap();

		assert_eq!(
			order_of(&root, "Manuscript"),
			["Chapter 2", "Chapter 3", "Scene 1"]
		);
	}

	#[test]
	fn moving_a_document_where_it_already_is_changes_nothing() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);
		let before = read_manifest(&root).unwrap();

		reorder_in(&root, chapter(&root, "Chapter 2"), 1).unwrap();

		assert_eq!(read_manifest(&root).unwrap(), before);
	}

	#[test]
	fn reordering_one_section_leaves_the_others_alone() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);
		create_in(root.clone(), "Notes", "Ideas").unwrap();
		let notes = order_of(&root, "Notes");

		reorder_in(&root, chapter(&root, "Chapter 3"), 0).unwrap();

		assert_eq!(order_of(&root, "Notes"), notes);
		assert_eq!(read_manifest(&root).unwrap().documents().len(), 6);
	}

	#[test]
	fn a_new_order_survives_a_refresh() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);

		reorder_in(&root, chapter(&root, "Chapter 3"), 0).unwrap();
		refresh(&root).unwrap();

		assert_eq!(
			order_of(&root, "Manuscript"),
			["Chapter 3", "Scene 1", "Chapter 2"]
		);
	}

	#[test]
	fn reordering_touches_no_files() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);
		let before = scan_paths(&root);

		reorder_in(&root, chapter(&root, "Chapter 3"), 0).unwrap();

		assert_eq!(scan_paths(&root), before);
	}

	#[test]
	fn a_document_whose_file_has_gone_can_still_be_moved() {
		let parent = tempfile::tempdir().unwrap();
		let root = with_three_chapters(&parent);
		let id = chapter(&root, "Chapter 3");
		fs::remove_file(root.join("Manuscript").join("Chapter 3.md")).unwrap();

		reorder_in(&root, id, 0).unwrap();

		assert_eq!(
			order_of(&root, "Manuscript"),
			["Chapter 3", "Scene 1", "Chapter 2"]
		);
	}

	#[test]
	fn moving_an_unknown_node_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manuscript = section_id(&root, "Manuscript");

		let err = move_node(root.clone(), Uuid::new_v4(), manuscript, 0).unwrap_err();
		assert!(matches!(err, Error::UnknownDocument));

		let scene = first_document(&root).id;
		let err = move_node(root, scene, Uuid::new_v4(), 0).unwrap_err();
		assert!(matches!(err, Error::UnknownFolder));
	}

	#[test]
	fn moving_refuses_a_relative_path() {
		let err = move_node(
			PathBuf::from("some/where"),
			Uuid::new_v4(),
			Uuid::new_v4(),
			0,
		)
		.unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn reordering_leaves_the_other_sections_where_they_were() {
		let mut manifest = manifest_with(&[
			"Manuscript/Scene 1.md",
			"Notes/Notes.md",
			"Manuscript/Chapter 2.md",
		]);
		let second = manifest.documents()[1].id;

		assert!(tree::move_to(&mut manifest.nodes, second, 0));

		assert_eq!(
			paths_of(&manifest),
			[
				"Manuscript/Chapter 2.md",
				"Manuscript/Scene 1.md",
				"Notes/Notes.md"
			],
			"a document moves among the ones it sits beside and nowhere else"
		);
	}

	#[test]
	fn a_scene_moves_between_chapters_and_takes_its_file() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		let chapter = folder_id(&root, "Manuscript/Part One/Chapter 2");
		let scene = document_id(&root, "Manuscript/Part One/Chapter 2/Landfall.md");
		write_document(root.clone(), scene, "The ship came in at dusk.".to_owned()).unwrap();

		move_node(root.clone(), scene, part, 0).unwrap();

		assert_eq!(
			tree::path(&read_manifest(&root).unwrap().nodes, scene).as_deref(),
			Some("Manuscript/Part One/Landfall.md"),
			"it sits in the part now, first"
		);
		assert!(
			!root
				.join("Manuscript/Part One/Chapter 2/Landfall.md")
				.exists()
		);
		assert_eq!(
			read_document(root.clone(), scene).unwrap(),
			"The ship came in at dusk.",
			"the file went with it"
		);
		assert!(
			tree::children(&read_manifest(&root).unwrap().nodes, chapter)
				.unwrap()
				.is_empty()
		);
	}

	#[test]
	fn a_chapter_moves_into_a_part_with_everything_under_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manuscript = section_id(&root, "Manuscript");
		let part = with_a_part(&root);
		let loose = create_folder(
			root.clone(),
			manuscript,
			"Chapter 9".to_owned(),
			Some(FolderKind::Chapter),
		)
		.unwrap()
		.id();
		create_document(root.clone(), loose, "Storm".to_owned()).unwrap();
		let scene = document_id(&root, "Manuscript/Chapter 9/Storm.md");

		move_node(root.clone(), loose, part, 0).unwrap();

		let manifest = read_manifest(&root).unwrap();
		assert_eq!(
			tree::path(&manifest.nodes, loose).as_deref(),
			Some("Manuscript/Part One/Chapter 9")
		);
		assert_eq!(
			tree::path(&manifest.nodes, scene).as_deref(),
			Some("Manuscript/Part One/Chapter 9/Storm.md"),
			"the scene inside it kept its id and came along"
		);
		assert!(
			root.join("Manuscript/Part One/Chapter 9/Storm.md")
				.is_file()
		);
		assert!(!root.join("Manuscript/Chapter 9").exists());
	}

	#[test]
	fn a_document_moves_between_sections() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let note = create_in(root.clone(), "Notes", "Wren").unwrap().id;

		move_node(root.clone(), note, section_id(&root, "Characters"), 0).unwrap();

		assert_eq!(
			tree::path(&read_manifest(&root).unwrap().nodes, note).as_deref(),
			Some("Characters/Wren.md")
		);
		assert!(root.join("Characters/Wren.md").is_file());
		assert!(!root.join("Notes/Wren.md").exists());
	}

	#[test]
	fn a_folder_cannot_be_moved_inside_itself_or_what_it_holds() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		let chapter = folder_id(&root, "Manuscript/Part One/Chapter 2");

		for into in [part, chapter] {
			let err = move_node(root.clone(), part, into, 0).unwrap_err();
			assert!(matches!(err, Error::MoveInsideItself));
		}

		assert!(root.join("Manuscript/Part One/Chapter 2").is_dir());
	}

	#[test]
	fn the_kind_rules_hold_when_a_folder_moves() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		let chapter = folder_id(&root, "Manuscript/Part One/Chapter 2");
		let research = create_folder(
			root.clone(),
			section_id(&root, "Notes"),
			"Research".to_owned(),
			None,
		)
		.unwrap()
		.id();

		// A part belongs directly in the Manuscript and nowhere else, a chapter
		// holds no folders, and a folder with no kind cannot enter the
		// Manuscript at all.
		for (node, into) in [
			(part, section_id(&root, "Notes")),
			(chapter, chapter),
			(research, section_id(&root, "Manuscript")),
			(research, part),
		] {
			let err = move_node(root.clone(), node, into, 0).unwrap_err();
			assert!(
				matches!(err, Error::FolderNotAllowed | Error::MoveInsideItself),
				"{err:?}"
			);
		}
	}

	#[test]
	fn a_section_cannot_be_moved() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = move_node(
			root.clone(),
			section_id(&root, "Notes"),
			section_id(&root, "Manuscript"),
			0,
		)
		.unwrap_err();

		assert!(matches!(err, Error::SectionFixed));
		assert!(root.join("Notes").is_dir());
	}

	#[test]
	fn moving_onto_a_name_already_there_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		let manuscript = section_id(&root, "Manuscript");
		let other = create_document(root.clone(), manuscript, "Prologue".to_owned()).unwrap();
		let loose = document_id(&root, "Manuscript/Prologue.md");
		write_document(root.clone(), loose, "the other one".to_owned()).unwrap();

		let err = move_node(root.clone(), loose, part, 0).unwrap_err();

		assert!(matches!(err, Error::DocumentExists));
		assert_eq!(
			read_document(root, other.id).unwrap(),
			"the other one",
			"neither file was written over"
		);
	}

	#[test]
	fn a_document_whose_file_has_gone_cannot_be_carried_elsewhere() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		let scene = first_document(&root).id;
		fs::remove_file(root.join("Manuscript/Scene 1.md")).unwrap();

		let err = move_node(root.clone(), scene, part, 0).unwrap_err();

		assert!(matches!(err, Error::DocumentMissing));
	}

	#[test]
	fn a_move_survives_a_refresh() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let part = with_a_part(&root);
		let scene = first_document(&root).id;

		move_node(root.clone(), scene, part, 0).unwrap();
		let manifest = refresh(&root).unwrap();

		assert_eq!(
			tree::path(&manifest.nodes, scene).as_deref(),
			Some("Manuscript/Part One/Scene 1.md"),
			"looking at the folder again found what the manifest already said"
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
		fs::write(root.join("Manuscript/Scene 1.md"), "Wren went down.").unwrap();
		fs::write(root.join("Notes/Notes.md"), "Ask about Wren.").unwrap();

		let all = read_all_documents(root.clone()).unwrap();

		assert_eq!(
			all.iter()
				.map(|d| d.document.path.as_str())
				.collect::<Vec<_>>(),
			[
				"Manuscript/Scene 1.md",
				"Outline/Outline.md",
				"Notes/Notes.md"
			],
			"manifest order, which is the order the sidebar shows"
		);
		assert_eq!(all[0].text.as_deref(), Some("Wren went down."));
		assert_eq!(all[2].text.as_deref(), Some("Ask about Wren."));
		assert_eq!(all[0].document.title, "Scene 1");
		assert_eq!(all[0].document.trail, ["Manuscript"]);
	}

	#[test]
	fn a_document_whose_file_has_gone_comes_back_without_text() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Notes/Notes.md"), "still here").unwrap();
		fs::remove_file(root.join("Manuscript/Scene 1.md")).unwrap();

		let all = read_all_documents(root).unwrap();

		assert_eq!(all.len(), 3, "it is still listed, so search can name it");
		assert!(all[0].text.is_none());
		assert_eq!(
			all[2].text.as_deref(),
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

	#[test]
	fn a_change_of_case_alone_is_told_apart() {
		let dir = Path::new("/project/Manuscript");
		let chapter = dir.join("Chapter 1.md");

		assert!(case_only(&chapter, &dir.join("chapter 1.md")));
		assert!(!case_only(&chapter, &chapter));
		assert!(!case_only(&chapter, &dir.join("Chapter 2.md")));
	}

	#[test]
	fn a_name_is_taken_only_by_something_other_than_the_one_renamed() {
		let dir = tempfile::tempdir().unwrap();
		let from = dir.path().join("Chapter 1.md");
		let lower = dir.path().join("chapter 1.md");
		let notes = dir.path().join("Notes.md");
		fs::write(&from, "").unwrap();
		fs::write(&notes, "").unwrap();

		assert!(!taken(&from, &lower).unwrap());
		assert!(taken(&notes, &from).unwrap());

		// Where both spellings can exist side by side, the lower one is not the
		// file being renamed and must not be written over. A file system that
		// ignores case has nothing to collide with: the name is already the
		// one file, which the assertion above covers.
		if !lower.exists() {
			fs::write(&lower, "").unwrap();
			assert!(taken(&from, &lower).unwrap());
		}
	}
}
