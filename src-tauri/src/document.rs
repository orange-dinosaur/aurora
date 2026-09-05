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
	Error, MANUSCRIPT, Manifest, Result, read_manifest, validate_name, write_atomic, write_manifest,
};
use crate::tree::{self, Fields, FolderKind, Node};

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
	/// The document's front matter block, fences and all, for the other end to
	/// read its fields out of. Empty when the file has none. Rust does not look
	/// inside it: what a field is, is defined once, in `src/frontmatter.ts`.
	pub front: String,
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
			front: String::new(),
			modified: None,
		}
	}
}

/// One of a folder's children, as the overview draws it. A document gets the
/// card it has always had; a folder says what it is, how much it holds and how
/// much has been written under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "node", rename_all = "camelCase")]
pub enum ChildSummary {
	Folder {
		id: Uuid,
		name: String,
		kind: Option<FolderKind>,
		/// How many nodes it holds directly, folders and documents alike.
		children: usize,
		/// The words in every document below it, however deep.
		words: usize,
	},
	Document(DocumentSummary),
}

/// One entry in the project's tree as the front end reads it: [`tree::Node`]
/// with the things a node's place decides already worked out. A document
/// carries the same view every other command sends, so nothing outside Rust
/// ever takes a path apart.
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
		/// The words in every document below it, however deep.
		words: usize,
	},
	Document(DocumentView),
}

impl NodeView {
	/// The node's id, whichever kind it is.
	pub fn id(&self) -> Uuid {
		match self {
			NodeView::Folder { id, .. } => *id,
			NodeView::Document(document) => document.id,
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

/// What one file looked like when its words were last counted.
#[derive(Clone, Copy)]
struct Counted {
	modified: SystemTime,
	len: u64,
	words: usize,
}

/// Every file this process has counted, by canonical path, held until it
/// quits. A tree read stats every document and opens only the ones that moved.
///
/// The commands run on Tauri's thread pool, so the map needs a lock. A `Mutex`
/// rather than an `RwLock`: a hit reads and a miss writes, each holds the map
/// for about as long as a hash lookup, and readers do not queue long enough
/// for the second lock to earn its keep. A panic while counting must not take
/// word counts away for the rest of the session, so a poisoned lock is taken
/// as it stands.
static COUNTS: LazyLock<Mutex<HashMap<PathBuf, Counted>>> =
	LazyLock::new(|| Mutex::new(HashMap::new()));

fn counted(path: &Path) -> Option<Counted> {
	COUNTS
		.lock()
		.unwrap_or_else(|poisoned| poisoned.into_inner())
		.get(path)
		.copied()
}

fn remember(path: &Path, counted: Counted) {
	COUNTS
		.lock()
		.unwrap_or_else(|poisoned| poisoned.into_inner())
		.insert(path.to_path_buf(), counted);
}

/// The words in one document, from its path under a canonical root. A file
/// that cannot be read counts as nothing rather than failing the whole tree,
/// and so does one the manifest points outside the project: `aurora.json` is a
/// file a writer can edit, so a name in it is not to be trusted.
fn words_in(root: &Path, relative: &str) -> usize {
	let Ok(path) = root.join(relative).canonicalize() else {
		return 0;
	};
	if !path.starts_with(root) {
		return 0;
	}

	// A stat costs a fraction of opening a file and splitting it, and a file
	// whose time and size have both stayed put has not been written since it
	// was counted. No timestamp at all means no caching: counting again is
	// only slow, while trusting a stamp that cannot go stale would be wrong.
	let stamp = fs::metadata(&path)
		.ok()
		.and_then(|data| data.modified().ok().map(|when| (when, data.len())));
	let held = stamp.and_then(|(modified, len)| {
		counted(&path).filter(|held| held.modified == modified && held.len == len)
	});

	if let Some(held) = held {
		return held.words;
	}

	let Ok(bytes) = fs::read(&path) else {
		return 0;
	};
	let text = String::from_utf8(bytes).unwrap_or_default();
	let words = body(&text).split_whitespace().count();

	if let Some((modified, len)) = stamp {
		remember(
			&path,
			Counted {
				modified,
				len,
				words,
			},
		);
	}

	words
}

/// The words under `nodes`, however deep they sit. For a card that has to say
/// what a folder amounts to without building the views of everything in it.
fn words_under(root: &Path, nodes: &[Node], prefix: &str) -> usize {
	nodes
		.iter()
		.map(|node| match node {
			Node::Folder { name, children, .. } => {
				words_under(root, children, &format!("{prefix}{name}/"))
			}
			Node::Document { name, .. } => words_in(root, &format!("{prefix}{name}")),
		})
		.sum()
}

/// The tree under `prefix`, ready to send, and the words in the whole of it.
/// The prefix is how far down the walk has come, which is what turns a node's
/// name into its path. The total comes back alongside the views because a
/// folder's count is its children's added up, and adding them up on the way
/// out is what keeps each file to a single read.
fn views(root: &Path, nodes: &[Node], prefix: &str) -> (Vec<NodeView>, usize) {
	let mut built = Vec::with_capacity(nodes.len());
	let mut total = 0;

	for node in nodes {
		match node {
			// The tree is about shape and counts. What the writer said about a
			// folder is asked for on its own, by the one panel that shows it.
			Node::Folder {
				id,
				name,
				kind,
				children,
				..
			} => {
				let (children, words) = views(root, children, &format!("{prefix}{name}/"));
				total += words;
				built.push(NodeView::Folder {
					id: *id,
					name: name.clone(),
					kind: *kind,
					children,
					words,
				});
			}
			Node::Document { id, name, target } => {
				let document = Document {
					id: *id,
					path: format!("{prefix}{name}"),
					target: *target,
				};
				total += words_in(root, &document.path);
				built.push(NodeView::Document((&document).into()));
			}
		}
	}

	(built, total)
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
			target: None,
			fields: Fields::new(),
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
				target: None,
				fields: Fields::new(),
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

/// The folders a document sits in, from its section down: `Manuscript`, `Part
/// One`, `Chapter 3`. This is the one thing a view says about where a document
/// is rather than what it is, and taking the path apart happens here so that
/// nothing outside Rust has to.
fn trail_of(path: &str) -> Vec<String> {
	let mut names: Vec<String> = path.split('/').map(str::to_owned).collect();
	names.pop();
	names
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
			target,
			fields,
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

		// The fields come from the manifest and never from disk: a folder is a
		// directory out there, and a directory says nothing about itself.
		sections.push(Node::Folder {
			id: *id,
			name: name.clone(),
			kind: *kind,
			target: *target,
			fields: fields.clone(),
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
					target,
					fields,
					children,
				},
				Node::Folder {
					children: below, ..
				},
			) => Node::Folder {
				id: *id,
				name: name.clone(),
				kind: *kind,
				target: *target,
				fields: fields.clone(),
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

/// A document cut into its front matter block and the prose under it. The fence
/// is spelled the same as the one `FRONT_MATTER` matches in
/// `src/frontmatter.ts`, which is the end that reads and writes what is inside
/// the block; the two have to agree on where a document starts. The block comes
/// back with its fences and nothing after them, which is the shape the parser
/// there is given.
fn split(text: &str) -> (&str, &str) {
	let Some(rest) = text.strip_prefix("---\n") else {
		return ("", text);
	};
	let Some(close) = rest.find("\n---") else {
		return ("", text);
	};

	let after = rest[close + 4..].trim_start_matches([' ', '\t']);
	let prose = after.strip_prefix('\n').unwrap_or(after);

	// The opening fence and the closing one are four bytes each.
	(&text[..close + 8], prose)
}

/// A document's prose, with any front matter block lifted off the top.
fn body(text: &str) -> &str {
	split(text).1
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

	let (front, prose) = split(&text);

	DocumentSummary {
		document: document.into(),
		words: prose.split_whitespace().count(),
		excerpt: excerpt(prose),
		front: front.to_owned(),
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
	project_path(manifest, base, path, true)
}

/// The same walk for a directory, whose last part is a folder name rather than
/// a file and so is not Markdown.
fn folder_path(manifest: &Manifest, base: &Path, path: &str) -> Result<PathBuf> {
	project_path(manifest, base, path, false)
}

fn project_path(manifest: &Manifest, base: &Path, path: &str, markdown: bool) -> Result<PathBuf> {
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
		if !ordinary(part) || (markdown && parts.peek().is_none() && !is_markdown(part)) {
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

/// Starts a new, empty document inside a folder. The name is the title, and a
/// document may sit at any level: `parent_id` is a section as readily as a
/// chapter three folders down.
#[tauri::command]
pub fn create_document(root: PathBuf, parent_id: Uuid, name: String) -> Result<DocumentView> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let name = without_extension(&name);
	validate_name(name)?;

	let manifest = read_manifest(&root)?;
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
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	validate_name(&name)?;

	let mut manifest = read_manifest(&root)?;
	let chain = tree::trail(&manifest.nodes, parent_id).ok_or(Error::UnknownFolder)?;
	let (inside, parent) = folder_at(&manifest, parent_id)?;

	// The section at the head of the chain is what decides whether kinds mean
	// anything here at all.
	if !tree::may_hold(chain[0].name() == MANUSCRIPT, parent, kind) {
		return Err(Error::FolderNotAllowed);
	}
	if tree::children(&manifest.nodes, parent_id)
		.unwrap_or_default()
		.iter()
		.any(|node| node.name() == name)
	{
		return Err(Error::AlreadyExists);
	}

	let directory = folder_path(&manifest, &root, &format!("{inside}/{name}"))?;
	// Where the folder above actually leads is what decides whether this stays
	// inside the project: a section folder can be a symlink the writer made.
	let above = directory.parent().ok_or(Error::BadDocumentPath)?;
	if !above.canonicalize()?.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}
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
#[tauri::command]
pub fn document_tree(root: PathBuf) -> Result<Vec<NodeView>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Ok(views(&canonical(&root), &read_manifest(&root)?.nodes, "").0)
}

/// Looks at the project's folders again for anything added, removed or renamed
/// outside Aurora, and brings the manifest back in step with what it finds.
/// Nothing comes back: whoever asked reads the tree again afterwards.
#[tauri::command]
pub fn refresh_documents(root: PathBuf) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	refresh(&root)?;
	Ok(())
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

/// Gives a folder a new name, which is to say a new directory name. Everything
/// under it keeps its id and its place, because nothing anywhere stores a path:
/// a descendant is found by walking down to it, and the walk is the same walk.
///
/// A section is refused. The Manuscript is recognised by its name and the kind
/// rules stand on that, so the top level of a project holds still.
#[tauri::command]
pub fn rename_folder(root: PathBuf, id: Uuid, name: String) -> Result<NodeView> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	validate_name(&name)?;

	let mut manifest = read_manifest(&root)?;
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
	if !over.canonicalize()?.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}
	// The manifest has already said no sibling is called this. Anything at the
	// new name is something Aurora does not know about, and is not to be
	// written over.
	if to.exists() {
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

/// Sets the word target a folder is written towards, or clears it with `None`.
/// The same field a document has, on the other kind of node, so a chapter and
/// the whole Manuscript are aimed the same way.
#[tauri::command]
pub fn set_folder_target(root: PathBuf, id: Uuid, target: Option<u32>) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let target = target.filter(|words| *words > 0);

	let mut manifest = read_manifest(&root)?;
	match tree::find_mut(&mut manifest.nodes, id) {
		Some(Node::Folder { target: aim, .. }) => *aim = target,
		_ => return Err(Error::UnknownFolder),
	}

	write_manifest(&root, &mut manifest)?;
	Ok(())
}

/// How a folder stands against what it is aiming at: everything written under
/// it, and the target on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderProgress {
	/// The words in every document below it, however deep.
	pub words: usize,
	pub target: Option<u32>,
}

/// A folder's progress, asked for on its own the way its fields are: only the
/// panel wants it, and the tree that already carries the count is read for the
/// sidebar rather than for this.
#[tauri::command]
pub fn folder_progress(root: PathBuf, id: Uuid) -> Result<FolderProgress> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

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
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

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
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let mut manifest = read_manifest(&root)?;
	match tree::find_mut(&mut manifest.nodes, id) {
		Some(Node::Folder { fields: held, .. }) => *held = fields,
		_ => return Err(Error::UnknownFolder),
	}

	write_manifest(&root, &mut manifest)?;
	Ok(())
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
	/// Nothing when the entry is a document on its own. When it is a whole
	/// folder, how many documents went into the trash inside it.
	pub inside: Option<u32>,
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

/// Moves a node into the project's trash and drops it from the manifest.
///
/// **The trash mirrors the project.** What is deleted lands at the path it sat
/// at, with the moment of the deletion in front of its own name. That is the
/// whole record of where it came from: nothing is written down beside it, and a
/// writer looking in `.trash` sees the shape they deleted from. A folder goes
/// in as one directory, so what was inside it stays inside it.
///
/// The name keeps its stamp so that deleting two things called the same thing
/// does not lose the first.
fn trash(root: &Path, id: Uuid, at: OffsetDateTime) -> Result<()> {
	let mut manifest = read_manifest(root)?;

	let folder = match tree::find(&manifest.nodes, id) {
		Some(Node::Folder { .. }) => true,
		Some(Node::Document { .. }) => false,
		None => return Err(Error::UnknownDocument),
	};
	// A section is the project's shape rather than something in it.
	if tree::parent(&manifest.nodes, id).is_none() {
		return Err(Error::SectionFixed);
	}

	let was = tree::path(&manifest.nodes, id).ok_or(Error::UnknownDocument)?;
	let (inside, name) = was.rsplit_once('/').ok_or(Error::BadDocumentPath)?;
	let from = if folder {
		folder_path(&manifest, root, &was)?
	} else {
		resolve(&manifest, root, id)?
	};

	// The folders above it are made in the trash as they are needed, so the
	// mirror only ever holds the paths something was actually deleted from.
	let into = root.join(TRASH_DIR).join(inside);
	fs::create_dir_all(&into)?;
	// The trash is an ordinary folder a writer can replace with a symlink, so
	// where it actually leads is what decides whether this move stays inside
	// the project.
	if !into.canonicalize()?.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}

	let at = stamp(at);
	let mut to = into.join(format!("{at} {name}"));
	// Two deletions within the same second would otherwise write over each
	// other.
	let mut again = 1;
	while to.exists() {
		to = into.join(format!("{at}-{again} {name}"));
		again += 1;
	}

	// The file moves first. If writing the manifest then fails, the next
	// refresh drops the node anyway, since it is no longer in the section.
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

	let manifest = read_manifest(&root)?;
	if !matches!(tree::find(&manifest.nodes, id), Some(Node::Document { .. })) {
		return Err(Error::UnknownDocument);
	}
	trash(&root, id, OffsetDateTime::now_utc())
}

/// Deletes a folder and everything in it. The subtree goes into the trash as
/// one directory and comes back out of it as one.
#[tauri::command]
pub fn delete_folder(root: PathBuf, id: Uuid) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	if !matches!(tree::find(&manifest.nodes, id), Some(Node::Folder { .. })) {
		return Err(Error::UnknownFolder);
	}
	trash(&root, id, OffsetDateTime::now_utc())
}

/// Puts a node at a given place inside a folder, which may be the one it is
/// already in. An index past the end means the end.
///
/// Reordering and moving are one operation because they are one thing: where a
/// node sits is its parent and its place among that parent's children, and
/// changing either is the same edit. Staying put touches no files, so a
/// document whose file has gone can still be reordered; going somewhere else
/// takes the file, or the whole directory, with it.
///
/// The kind rules of [`tree::may_hold`] apply, and two rules of their own: a
/// section stays where it is, and nothing may be moved inside itself.
#[tauri::command]
pub fn move_node(root: PathBuf, id: Uuid, parent_id: Uuid, index: usize) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let mut manifest = read_manifest(&root)?;

	let (folder, kind, name) = match tree::find(&manifest.nodes, id) {
		Some(Node::Folder { kind, name, .. }) => (true, *kind, name.clone()),
		Some(Node::Document { name, .. }) => (false, None, name.clone()),
		None => return Err(Error::UnknownDocument),
	};

	let held = tree::parent(&manifest.nodes, id).map(Node::id);
	// The top level is the project's shape rather than something in it, and the
	// Manuscript is known by its name.
	let Some(held) = held else {
		return Err(Error::SectionFixed);
	};

	let chain = tree::trail(&manifest.nodes, parent_id).ok_or(Error::UnknownFolder)?;
	// A node cannot be put inside itself, and everything it holds is inside it.
	if chain.iter().any(|node| node.id() == id) {
		return Err(Error::MoveInsideItself);
	}
	// The section at the head of the chain is what decides whether kinds mean
	// anything where this is going.
	let in_manuscript = chain[0].name() == MANUSCRIPT;
	let (into, parent) = folder_at(&manifest, parent_id)?;
	if folder && !tree::may_hold(in_manuscript, parent, kind) {
		return Err(Error::FolderNotAllowed);
	}

	// The disk only comes into it when the node changes hands. A name that
	// collides is refused before anything moves.
	if parent_id != held {
		let taken = if folder {
			Error::AlreadyExists
		} else {
			Error::DocumentExists
		};
		if tree::children(&manifest.nodes, parent_id)
			.unwrap_or_default()
			.iter()
			.any(|node| node.name() == name)
		{
			return Err(taken);
		}

		let was = tree::path(&manifest.nodes, id).ok_or(Error::UnknownDocument)?;
		let path = format!("{into}/{name}");
		let (from, to) = if folder {
			(
				folder_path(&manifest, &root, &was)?,
				folder_path(&manifest, &root, &path)?,
			)
		} else {
			(
				document_path(&manifest, &root, &was)?,
				document_path(&manifest, &root, &path)?,
			)
		};

		// Where the folder above actually leads is what decides whether this
		// stays inside the project, the same as when something is made.
		let over = to.parent().ok_or(Error::BadDocumentPath)?;
		if !over.canonicalize()?.starts_with(root.canonicalize()?) {
			return Err(Error::OutsideProject);
		}
		if to.exists() {
			return Err(taken);
		}
		// Reordering a document whose file has gone is fine; carrying it to
		// another folder is not, because there is nothing to carry.
		if !folder && !from.exists() {
			return Err(Error::DocumentMissing);
		}

		fs::rename(&from, &to)?;
	}

	let moving = tree::remove(&mut manifest.nodes, id).ok_or(Error::UnknownDocument)?;
	let Some(Node::Folder { children, .. }) = tree::find_mut(&mut manifest.nodes, parent_id) else {
		unreachable!("the destination was a folder a moment ago")
	};
	children.insert(index.min(children.len()), moving);
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

/// How many documents a trashed folder holds, at any depth. What the writer
/// wants to know before putting it back is how much of their work is in there.
fn documents_inside(dir: &Path) -> Result<u32> {
	let mut total = 0;
	for entry in fs::read_dir(dir)? {
		let entry = entry?;
		let name = entry.file_name();
		let Some(name) = name.to_str() else {
			continue;
		};
		let kind = entry.file_type()?;
		if kind.is_dir() {
			total += documents_inside(&entry.path())?;
		} else if kind.is_file() && is_markdown(name) {
			total += 1;
		}
	}
	Ok(total)
}

/// One level of the trash, and every level under it that a deletion made on its
/// way in. A stamped name is what was actually deleted, so it becomes an entry
/// and the walk does not go inside it: a chapter of fourteen scenes is one row,
/// not fifteen.
fn gather_trash(dir: &Path, at: &str, section: &str, found: &mut Vec<TrashEntry>) -> Result<()> {
	let entries = match fs::read_dir(dir) {
		Ok(entries) => entries,
		Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
		Err(e) => return Err(e.into()),
	};

	for entry in entries {
		let entry = entry?;
		let name = entry.file_name();
		let Some(name) = name.to_str() else {
			continue;
		};
		if name.starts_with('.') {
			continue;
		}

		// A symlink is neither a document nor a folder, here as anywhere else.
		let kind = entry.file_type()?;
		let directory = kind.is_dir();
		if !directory && !(kind.is_file() && is_markdown(name)) {
			continue;
		}

		let path = format!("{at}/{name}");
		match unstamp(name) {
			Some((deleted, was)) => found.push(TrashEntry {
				folder: section.to_owned(),
				title: without_extension(was).to_owned(),
				deleted: Some(deleted),
				inside: directory
					.then(|| documents_inside(&entry.path()))
					.transpose()?,
				path,
			}),
			// An unstamped directory is one the mirror needed, so what was
			// deleted is further down.
			None if directory => gather_trash(&entry.path(), &path, section, found)?,
			// A file somebody put there by hand is still shown, so it can at
			// least be got rid of.
			None => found.push(TrashEntry {
				folder: section.to_owned(),
				title: without_extension(name).to_owned(),
				deleted: None,
				inside: None,
				path,
			}),
		}
	}

	Ok(())
}

/// Everything in the project's trash, most recently deleted first.
#[tauri::command]
pub fn list_trash(root: PathBuf) -> Result<Vec<TrashEntry>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	let trash = root.join(TRASH_DIR);
	let mut entries = Vec::new();
	for section in manifest.folders() {
		gather_trash(&trash.join(&section), &section, &section, &mut entries)?;
	}

	// Newest first, with anything undated behind the rest.
	entries.sort_by(|a, b| b.deleted.cmp(&a.deleted).then_with(|| a.path.cmp(&b.path)));
	Ok(entries)
}

/// Something in the trash, once the project is satisfied it is really in there.
/// A trashed folder is a directory and a trashed document is a file, so the
/// path is walked without holding its last part to the `.md` rule.
fn trash_entry(manifest: &Manifest, root: &Path, path: &str) -> Result<PathBuf> {
	let file = folder_path(manifest, &root.join(TRASH_DIR), path)?;
	if !file.exists() {
		return Err(Error::DocumentMissing);
	}
	// The trash is an ordinary folder a writer can replace with a symlink.
	if !file.canonicalize()?.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}
	Ok(file)
}

/// The deepest of the folders on this path that the project still has. The
/// section at its head is the last resort, and something the writer deleted
/// three folders down comes back as near to where it was as is left.
fn nearest_folder(manifest: &Manifest, path: &str) -> String {
	let parts: Vec<&str> = path.split('/').collect();

	for depth in (1..=parts.len()).rev() {
		let above = parts[..depth].join("/");
		let there = tree::walk(&manifest.nodes).any(|node| {
			matches!(node, Node::Folder { .. })
				&& tree::path(&manifest.nodes, node.id()).as_deref() == Some(above.as_str())
		});
		if there {
			return above;
		}
	}

	parts[0].to_owned()
}

/// Puts a deleted document or folder back where it came from, under the name it
/// had. The trash mirrors the project, so where it came from is the path it is
/// sitting at with the stamp taken off. A folder it was inside may have been
/// deleted too, and then it goes to the nearest one still standing rather than
/// failing. Anything already using that name is not written over.
#[tauri::command]
pub fn restore_from_trash(root: PathBuf, path: String) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	let from = trash_entry(&manifest, &root, &path)?;

	let (inside, file) = path.rsplit_once('/').ok_or(Error::BadDocumentPath)?;
	let was = unstamp(file).map_or(file, |(_, was)| was);
	let back = format!("{}/{was}", nearest_folder(&manifest, inside));

	let directory = from.is_dir();
	let to = if directory {
		folder_path(&manifest, &root, &back)?
	} else {
		document_path(&manifest, &root, &back)?
	};
	if to.exists() {
		return Err(if directory {
			Error::AlreadyExists
		} else {
			Error::DocumentExists
		});
	}

	let folder = to.parent().ok_or(Error::BadDocumentPath)?;
	fs::create_dir_all(folder)?;
	if !folder.canonicalize()?.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}

	fs::rename(&from, &to)?;

	// What came back has no ids any more, so the manifest adopts it the way it
	// adopts anything that appears in a section: at the end of the folder it
	// landed in, and inside a restored folder in the order the disk lists it.
	refresh(&root)?;
	Ok(())
}

/// Throws one thing in the trash away for good, a folder with everything that
/// went into it.
#[tauri::command]
pub fn purge_trash_entry(root: PathBuf, path: String) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	let file = trash_entry(&manifest, &root, &path)?;
	if file.is_dir() {
		fs::remove_dir_all(file)?;
	} else {
		fs::remove_file(file)?;
	}
	Ok(())
}

/// What a folder holds, one card at a time, in the order the folder keeps
/// them. Reads the manifest rather than the folder, the way `document_tree`
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

	let counting_root = canonical(&root);

	Ok(tree::children(&manifest.nodes, id)
		.unwrap_or_default()
		.iter()
		.map(|node| match node {
			// A folder card says what it is and how much it holds. What the
			// writer said about it is the panel's business, not the card's.
			Node::Folder {
				id,
				name,
				kind,
				children,
				..
			} => ChildSummary::Folder {
				id: *id,
				name: name.clone(),
				kind: *kind,
				children: children.len(),
				words: words_under(&counting_root, children, &format!("{prefix}/{name}/")),
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
	use crate::tree::{self, Value, tree_from_flat};
	use time::OffsetDateTime;

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
				Node::Document { id, name, target } => Some(DocumentView::from(&Document {
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

		let NodeView::Document(chapter) = &children[0] else {
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
}
