//! Every project keeps its versions as a git repository at its own root.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use git2::build::CheckoutBuilder;
use git2::{Commit, ErrorCode, IndexAddOption, Oid, Repository, Signature, Sort, Tree};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::document::{
	TRASH_DIR, body, delete_document, move_node, recreate_document, rename_document, write_document,
};
use crate::history::HISTORY_FILE;
use crate::project::{
	Book, COVER_NAMES, Error, MANIFEST_FILE, Manifest, Result, read_manifest, remove_if_there,
	write_atomic, write_book,
};
use crate::tree::{self, Node};

const FIRST_VERSION: &str = "Versions begin";
const AUTHOR_EMAIL: &str = "aurora@localhost";

/// Marks a commit as one Aurora kept, so it is never named by its subject.
const KIND_TRAILER: &str = "Aurora-Kind";
const MINUTES_TRAILER: &str = "Aurora-Minutes";
const WRITTEN_TRAILER: &str = "Aurora-Written";
const REMOVED_TRAILER: &str = "Aurora-Removed";

/// Why Aurora kept a version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Kind {
	Named,
	Session,
	Closing,
	BeforePuttingBack,
}

impl Kind {
	const ALL: [Kind; 4] = [
		Kind::Named,
		Kind::Session,
		Kind::Closing,
		Kind::BeforePuttingBack,
	];

	fn as_str(self) -> &'static str {
		match self {
			Kind::Named => "named",
			Kind::Session => "session",
			Kind::Closing => "closing",
			Kind::BeforePuttingBack => "beforePuttingBack",
		}
	}

	fn parse(text: &str) -> Option<Kind> {
		Kind::ALL.into_iter().find(|kind| kind.as_str() == text)
	}
}

/// One row of the Versions tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Version {
	pub id: String,
	#[serde(with = "time::serde::rfc3339")]
	#[ts(type = "string")]
	pub at: OffsetDateTime,
	/// None for a commit Aurora did not make.
	pub kind: Option<Kind>,
	pub name: Option<String>,
	pub minutes: Option<u32>,
	pub written: Option<u32>,
	pub removed: Option<u32>,
	pub author: String,
	/// The document's words in this version, when the list is of one document.
	pub words: Option<u32>,
}

/// A version the frontend asks to keep. Every count is in words.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum Keep {
	Named {
		name: String,
	},
	Session {
		minutes: u32,
		written: u32,
		removed: u32,
	},
	Closing {
		written: u32,
		removed: u32,
	},
	/// `label` is the name or time of the version being put back.
	BeforePuttingBack {
		label: String,
	},
}

impl Keep {
	fn kind(&self) -> Kind {
		match self {
			Keep::Named { .. } => Kind::Named,
			Keep::Session { .. } => Kind::Session,
			Keep::Closing { .. } => Kind::Closing,
			Keep::BeforePuttingBack { .. } => Kind::BeforePuttingBack,
		}
	}

	fn name(&self) -> Option<String> {
		match self {
			Keep::Named { name } => Some(name.clone()),
			Keep::BeforePuttingBack { label } => Some(format!("Before putting back “{label}”")),
			Keep::Session { .. } | Keep::Closing { .. } => None,
		}
	}

	/// A subject someone reading `git log` can follow, then the trailers the
	/// Versions tab reads its labels from.
	fn message(&self) -> String {
		let mut trailers = vec![format!("{KIND_TRAILER}: {}", self.kind().as_str())];
		let subject = match self {
			Keep::Named { .. } | Keep::BeforePuttingBack { .. } => self.name().unwrap_or_default(),
			Keep::Session {
				minutes,
				written,
				removed,
			} => {
				trailers.push(format!("{MINUTES_TRAILER}: {minutes}"));
				trailers.push(format!("{WRITTEN_TRAILER}: {written}"));
				trailers.push(format!("{REMOVED_TRAILER}: {removed}"));
				format!(
					"A session of {minutes} minutes, {written} words written and {removed} removed"
				)
			}
			Keep::Closing { written, removed } => {
				trailers.push(format!("{WRITTEN_TRAILER}: {written}"));
				trailers.push(format!("{REMOVED_TRAILER}: {removed}"));
				format!("Left the project, {written} words written and {removed} removed")
			}
		};
		format!("{subject}\n\n{}\n", trailers.join("\n"))
	}
}

/// Set once the writer has been told about an enclosing repository. It lives
/// in the project's own git config, which is local and never pushed.
pub const NESTED_NOTICE_SHOWN: &str = "aurora.nestedNoticeShown";

/// Kept out of every version, so going back never rolls back Stats or the
/// Trash. Written to `.git/info/exclude` so the project folder gains no file.
fn excluded() -> [String; 2] {
	[format!("/{HISTORY_FILE}"), format!("/{TRASH_DIR}/")]
}

/// Opens the repository at `root`, making one when there is none. Returns the
/// folder of a repository the project sits inside, while the writer has not
/// yet been told about it.
pub fn ensure_repository(root: &Path, author: &str) -> Result<Option<PathBuf>> {
	// `open` looks only at `root`: a repository further up belongs to
	// something else, and the project still gets its own.
	let repo = match Repository::open(root) {
		Ok(repo) => repo,
		Err(e) if e.code() == ErrorCode::NotFound => Repository::init(root)?,
		Err(e) => return Err(e.into()),
	};
	exclude(&repo)?;
	if repo.is_empty()? {
		keep_first(&repo, author)?;
	}
	enclosing(root, &repo)
}

fn exclude(repo: &Repository) -> Result<()> {
	let path = repo.path().join("info").join("exclude");
	let mut text = match fs::read_to_string(&path) {
		Ok(text) => text,
		Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
		Err(e) => return Err(e.into()),
	};
	let missing: Vec<String> = excluded()
		.into_iter()
		.filter(|line| !text.lines().any(|existing| existing.trim() == line))
		.collect();
	if missing.is_empty() {
		return Ok(());
	}

	if !text.is_empty() && !text.ends_with('\n') {
		text.push('\n');
	}
	for line in missing {
		text.push_str(&line);
		text.push('\n');
	}
	fs::create_dir_all(repo.path().join("info"))?;
	fs::write(&path, text)?;
	Ok(())
}

fn keep_first(repo: &Repository, author: &str) -> Result<()> {
	let tree = repo.find_tree(stage(repo)?)?;
	let first = Keep::Named {
		name: FIRST_VERSION.to_owned(),
	};
	commit(repo, &signature(author)?, &first, &tree, &[])?;
	Ok(())
}

fn signature(author: &str) -> Result<Signature<'static>> {
	let name = if author.trim().is_empty() {
		"Aurora"
	} else {
		author
	};
	// libgit2 refuses an empty email, and a writer has none to give.
	Ok(Signature::now(name, AUTHOR_EMAIL)?)
}

/// Stages the whole project, deletions included, and returns its tree.
fn stage(repo: &Repository) -> Result<Oid> {
	let mut index = repo.index()?;
	index.add_all(["*"], IndexAddOption::DEFAULT, None)?;
	index.update_all(["*"], None)?;
	index.write()?;
	Ok(index.write_tree()?)
}

fn commit(
	repo: &Repository,
	signature: &Signature,
	keep: &Keep,
	tree: &Tree,
	parents: &[&Commit],
) -> Result<Oid> {
	let id = repo.commit(
		Some("HEAD"),
		signature,
		signature,
		&keep.message(),
		tree,
		parents,
	)?;
	if let Some(name) = keep.name() {
		repo.note(signature, signature, None, id, &name, true)?;
	}
	Ok(id)
}

/// Keeps the project as it is now and returns the version that holds it. With
/// nothing changed that is the latest version, and only a named version does
/// anything: it names the latest one, or keeps an empty version under the new
/// name when the latest already has one, so no name is lost.
pub fn keep(root: &Path, keep: &Keep, author: &str) -> Result<Oid> {
	let repo = Repository::open(root)?;
	let head = repo.head()?.peel_to_commit()?;
	let tree = repo.find_tree(stage(&repo)?)?;
	let signature = signature(author)?;

	if tree.id() == head.tree_id() {
		match keep {
			Keep::Named { name } if name_of(&repo, &head)?.is_none() => {
				repo.note(&signature, &signature, None, head.id(), name, true)?;
				return Ok(head.id());
			}
			Keep::Named { .. } => {}
			_ => return Ok(head.id()),
		}
	}
	commit(&repo, &signature, keep, &tree, &[&head])
}

/// A version's name. A note always wins, and an empty one means no name. A
/// commit Aurora did not make is otherwise named by its subject line.
pub fn name_of(repo: &Repository, commit: &Commit) -> Result<Option<String>> {
	let ours = trailers(commit)?.iter().any(|(key, _)| key == KIND_TRAILER);
	name(repo, commit, ours)
}

fn name(repo: &Repository, commit: &Commit, ours: bool) -> Result<Option<String>> {
	match repo.find_note(None, commit.id()) {
		Ok(note) => {
			let name = note.message()?.trim();
			return Ok((!name.is_empty()).then(|| name.to_owned()));
		}
		Err(e) if e.code() == ErrorCode::NotFound => {}
		Err(e) => return Err(e.into()),
	}
	if ours {
		return Ok(None);
	}
	Ok(commit.summary()?.map(str::to_owned))
}

fn trailers(commit: &Commit) -> Result<Vec<(String, String)>> {
	Ok(git2::message_trailers_strs(commit.message()?)?
		.iter()
		.map(|(key, value)| (key.to_owned(), value.to_owned()))
		.collect())
}

fn describe(repo: &Repository, commit: &Commit) -> Result<Version> {
	let trailers = trailers(commit)?;
	let value = |key: &str| {
		trailers
			.iter()
			.find(|(k, _)| k == key)
			.map(|(_, v)| v.as_str())
	};
	let count = |key: &str| value(key).and_then(|v| v.parse().ok());
	Ok(Version {
		id: commit.id().to_string(),
		at: OffsetDateTime::from_unix_timestamp(commit.time().seconds())
			.unwrap_or(OffsetDateTime::UNIX_EPOCH),
		kind: value(KIND_TRAILER).and_then(Kind::parse),
		name: name(repo, commit, value(KIND_TRAILER).is_some())?,
		minutes: count(MINUTES_TRAILER),
		written: count(WRITTEN_TRAILER),
		removed: count(REMOVED_TRAILER),
		author: commit.author().name().unwrap_or_default().to_owned(),
		words: None,
	})
}

/// The blob holding a document's text in one version, found by its id in
/// that version's manifest so a rename or a move does not lose it. None when
/// the document is not there, or the manifest cannot be read.
///
/// Most versions share their manifest with the one before, so `paths`
/// remembers the document's path by manifest and each is parsed once.
fn text_in(
	repo: &Repository,
	commit: &Commit,
	document: Uuid,
	paths: &mut HashMap<Oid, Option<String>>,
) -> Result<Option<Oid>> {
	let tree = commit.tree()?;
	let Ok(entry) = tree.get_path(Path::new(MANIFEST_FILE)) else {
		return Ok(None);
	};
	if let Entry::Vacant(slot) = paths.entry(entry.id()) {
		let blob = repo.find_blob(entry.id())?;
		let path = serde_json::from_slice::<Manifest>(blob.content())
			.ok()
			.and_then(|manifest| {
				manifest
					.documents()
					.into_iter()
					.find(|d| d.id == document)
					.map(|d| d.path)
			});
		slot.insert(path);
	}
	let Some(path) = &paths[&entry.id()] else {
		return Ok(None);
	};
	Ok(tree.get_path(Path::new(path)).ok().map(|e| e.id()))
}

/// Every version, newest first along first parents. With a document, only
/// the versions in which its text changed.
pub fn list(root: &Path, document: Option<Uuid>) -> Result<Vec<Version>> {
	let repo = Repository::open(root)?;
	let mut walk = repo.revwalk()?;
	walk.set_sorting(Sort::TOPOLOGICAL)?;
	walk.simplify_first_parent()?;
	walk.push_head()?;

	let mut versions = Vec::new();
	let mut paths = HashMap::new();
	for id in walk {
		let commit = repo.find_commit(id?)?;
		let mut version = describe(&repo, &commit)?;
		if let Some(document) = document {
			let before = match commit.parent(0) {
				Ok(parent) => text_in(&repo, &parent, document, &mut paths)?,
				Err(_) => None,
			};
			let now = text_in(&repo, &commit, document, &mut paths)?;
			if now == before {
				continue;
			}
			version.words = match now {
				Some(blob) => Some(words_in(repo.find_blob(blob)?.content())),
				None => Some(0),
			};
		}
		versions.push(version);
	}
	Ok(versions)
}

/// Counted the way the rest of Aurora counts a document: its body only.
fn words_in(bytes: &[u8]) -> u32 {
	let text = String::from_utf8_lossy(bytes);
	body(&text).split_whitespace().count() as u32
}

/// How one document or folder differs between two versions. Matched by id, so
/// a rename or a move is never read as a deletion and an addition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Change {
	pub id: Uuid,
	pub folder: bool,
	/// Its path in the older version, or none when it was added.
	pub before: Option<String>,
	/// Its path in the newer version, or none when it was deleted.
	pub after: Option<String>,
	pub renamed: bool,
	/// Into another folder, not merely under a folder that was renamed.
	pub moved: bool,
	/// A document whose text differs. Never set on a folder.
	pub edited: bool,
	/// In a folder, the children in both versions that changed places. Empty on
	/// a document.
	pub reordered: Vec<Reorder>,
}

/// A child that moved within its folder, by name as the folder now holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Reorder {
	pub name: String,
	/// The child it now comes after, or none when it is first.
	pub after: Option<String>,
}

/// Everything that differs between two versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Changes {
	/// In the newer version's order, then whatever was deleted.
	pub nodes: Vec<Change>,
	/// Any book detail other than the cover.
	pub book: bool,
	pub cover: bool,
}

/// A node where one version has it.
struct Placed {
	folder: bool,
	name: String,
	parent: Option<Uuid>,
	path: String,
	children: Vec<Uuid>,
}

fn id_of(node: &Node) -> Uuid {
	match node {
		Node::Folder { id, .. } | Node::Document { id, .. } => *id,
	}
}

fn place(nodes: &[Node], parent: Option<Uuid>, prefix: &str, placed: &mut Vec<(Uuid, Placed)>) {
	for node in nodes {
		let (name, folder, children) = match node {
			Node::Folder { name, children, .. } => {
				(name, true, children.iter().map(id_of).collect())
			}
			Node::Document { name, .. } => (name, false, Vec::new()),
		};
		let id = id_of(node);
		let path = format!("{prefix}{name}");
		placed.push((
			id,
			Placed {
				folder,
				name: name.clone(),
				parent,
				path: path.clone(),
				children,
			},
		));
		if let Node::Folder { children, .. } = node {
			place(children, Some(id), &format!("{path}/"), placed);
		}
	}
}

/// A version's tree and manifest. With no version, the project as it stands
/// on disk, staged but not kept.
fn side<'r>(repo: &'r Repository, version: Option<&str>) -> Result<(Tree<'r>, Manifest)> {
	let tree = match version {
		Some(id) => repo.find_commit(Oid::from_str(id)?)?.tree()?,
		None => repo.find_tree(stage(repo)?)?,
	};
	let manifest = repo.find_blob(tree.get_path(Path::new(MANIFEST_FILE))?.id())?;
	let manifest = serde_json::from_slice(manifest.content())?;
	Ok((tree, manifest))
}

/// Which children changed places, each with the child it now follows. The
/// longest run still in its old order stays put and everything else moved, so
/// dragging one child names only that child. What was added, deleted or moved
/// to another folder is reported on its own.
fn reordered(then: &[Uuid], now: &[Uuid]) -> Vec<(Uuid, Option<Uuid>)> {
	let was: Vec<Uuid> = then.iter().filter(|id| now.contains(id)).copied().collect();
	let is: Vec<(usize, Uuid)> = now
		.iter()
		.enumerate()
		.filter(|(_, id)| was.contains(id))
		.map(|(at, id)| (at, *id))
		.collect();
	let old: Vec<usize> = is
		.iter()
		.map(|(_, id)| was.iter().position(|other| other == id).unwrap_or(0))
		.collect();

	// ponytail: quadratic longest increasing run, fine for a folder's children.
	let mut length = vec![1; old.len()];
	let mut from = vec![None; old.len()];
	for i in 0..old.len() {
		for j in 0..i {
			if old[j] < old[i] && length[j] + 1 > length[i] {
				length[i] = length[j] + 1;
				from[i] = Some(j);
			}
		}
	}
	let mut stays = vec![false; old.len()];
	let mut at = (0..old.len()).max_by_key(|&i| length[i]);
	while let Some(i) = at {
		stays[i] = true;
		at = from[i];
	}

	is.iter()
		.zip(stays)
		.filter(|(_, stays)| !stays)
		.map(|((at, id), _)| (*id, at.checked_sub(1).map(|before| now[before])))
		.collect()
}

fn blob_at(tree: &Tree, path: &str) -> Option<Oid> {
	tree.get_path(Path::new(path)).ok().map(|entry| entry.id())
}

/// What changed from version `a` to version `b`, where none is the project as
/// it stands on disk.
pub fn changes(root: &Path, a: Option<&str>, b: Option<&str>) -> Result<Changes> {
	let repo = Repository::open(root)?;
	let (old_tree, old) = side(&repo, a)?;
	let (new_tree, new) = side(&repo, b)?;
	let placed = |manifest: &Manifest| {
		let mut placed = Vec::new();
		place(&manifest.nodes, None, "", &mut placed);
		placed
	};
	let (was, is) = (placed(&old), placed(&new));
	let before: HashMap<Uuid, &Placed> = was.iter().map(|(id, node)| (*id, node)).collect();
	let after: HashSet<Uuid> = is.iter().map(|(id, _)| *id).collect();
	let names: HashMap<Uuid, &String> = is.iter().map(|(id, node)| (*id, &node.name)).collect();

	let mut nodes = Vec::new();
	for (id, now) in &is {
		let then = before.get(id);
		let change = Change {
			id: *id,
			folder: now.folder,
			before: then.map(|then| then.path.clone()),
			after: Some(now.path.clone()),
			renamed: then.is_some_and(|then| then.name != now.name),
			moved: then.is_some_and(|then| then.parent != now.parent),
			edited: !now.folder
				&& then.is_some_and(|then| {
					blob_at(&old_tree, &then.path) != blob_at(&new_tree, &now.path)
				}),
			reordered: then.map_or_else(Vec::new, |then| {
				reordered(&then.children, &now.children)
					.into_iter()
					.map(|(id, after)| Reorder {
						name: names[&id].clone(),
						after: after.map(|after| names[&after].clone()),
					})
					.collect()
			}),
		};
		if then.is_none()
			|| change.renamed
			|| change.moved
			|| change.edited
			|| !change.reordered.is_empty()
		{
			nodes.push(change);
		}
	}
	for (id, then) in was.iter().filter(|(id, _)| !after.contains(id)) {
		nodes.push(Change {
			id: *id,
			folder: then.folder,
			before: Some(then.path.clone()),
			after: None,
			renamed: false,
			moved: false,
			edited: false,
			reordered: Vec::new(),
		});
	}

	let details = |book: &Book| Book {
		cover: String::new(),
		..book.clone()
	};
	Ok(Changes {
		nodes,
		book: details(&old.book) != details(&new.book),
		cover: old.book.cover != new.book.cover
			|| blob_at(&old_tree, &old.book.cover) != blob_at(&new_tree, &new.book.cover),
	})
}

/// A document's full text at a version, or none when it is not there.
pub fn text(root: &Path, version: &str, document: Uuid) -> Result<Option<String>> {
	let repo = Repository::open(root)?;
	let commit = repo.find_commit(Oid::from_str(version)?)?;
	let Some(blob) = text_in(&repo, &commit, document, &mut HashMap::new())? else {
		return Ok(None);
	};
	let blob = repo.find_blob(blob)?;
	Ok(Some(String::from_utf8_lossy(blob.content()).into_owned()))
}

/// Writes a name onto a version, replacing any it had. An empty name is
/// stored as an empty note, which is what takes a subject-line name away.
fn set_name(root: &Path, id: &str, name: &str, author: &str) -> Result<()> {
	let signature = signature(author)?;
	let repo = Repository::open(root)?;
	let commit = repo.find_commit(Oid::from_str(id)?)?;
	repo.note(&signature, &signature, None, commit.id(), name.trim(), true)?;
	Ok(())
}

/// The book's author, which every version is kept under.
fn author(root: &Path) -> Result<String> {
	// The path arrives from the frontend, so it is not trusted to be sensible.
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Ok(read_manifest(root)?.book.author)
}

#[tauri::command]
pub fn keep_version(root: PathBuf, keep: Keep) -> Result<String> {
	let author = author(&root)?;
	Ok(self::keep(&root, &keep, &author)?.to_string())
}

#[tauri::command]
pub fn list_versions(root: PathBuf, document: Option<Uuid>) -> Result<Vec<Version>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	list(&root, document)
}

/// Records that the writer has seen the note about an enclosing repository.
#[tauri::command]
pub fn nested_notice_shown(root: PathBuf) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Repository::open(&root)?
		.config()?
		.set_bool(NESTED_NOTICE_SHOWN, true)?;
	Ok(())
}

#[tauri::command]
pub fn rename_version(root: PathBuf, id: String, name: String) -> Result<()> {
	set_name(&root, &id, &name, &author(&root)?)
}

#[tauri::command]
pub fn unname_version(root: PathBuf, id: String) -> Result<()> {
	set_name(&root, &id, "", &author(&root)?)
}

/// Either side left out is the project as it stands now.
#[tauri::command]
pub fn version_changes(root: PathBuf, a: Option<String>, b: Option<String>) -> Result<Changes> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	changes(&root, a.as_deref(), b.as_deref())
}

#[tauri::command]
pub fn version_text(root: PathBuf, version: String, id: Uuid) -> Result<Option<String>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	text(&root, &version, id)
}

/// Keeps the project as it stands, so whatever is put back can be undone.
/// `label` is the name or time of the version being put back, or none when
/// the same visit to the compare view has already kept one.
fn keep_before(root: &Path, label: Option<String>) -> Result<Repository> {
	let author = author(root)?;
	if let Some(label) = label {
		keep(root, &Keep::BeforePuttingBack { label }, &author)?;
	}
	Ok(Repository::open(root)?)
}

/// Where a version had a document: the nearest of its old folders the project
/// still has, and its place there, after the document it used to follow if
/// that is in the same folder now.
fn destination(then: &Manifest, now: &Manifest, id: Uuid) -> Result<(Uuid, usize)> {
	let trail = tree::trail(&then.nodes, id).ok_or(Error::UnknownDocument)?;
	let parent = trail
		.iter()
		.rev()
		.skip(1)
		.map(|node| node.id())
		.find(|folder| tree::children(&now.nodes, *folder).is_some())
		.ok_or(Error::UnknownFolder)?;
	let siblings = tree::parent(&then.nodes, id)
		.and_then(|above| tree::children(&then.nodes, above.id()))
		.unwrap_or_default();
	let children = tree::children(&now.nodes, parent).unwrap_or_default();
	let at = siblings
		.iter()
		.position(|node| node.id() == id)
		.unwrap_or(0);
	let index = siblings[..at]
		.iter()
		.rev()
		.find_map(|sibling| children.iter().position(|c| c.id() == sibling.id()))
		.map_or(0, |before| before + 1);
	Ok((parent, index))
}

/// Puts one document back as a version had it: its text, its name and its
/// folder. One the version did not have goes to the Trash, and one it had
/// that has since been deleted comes back under its old id.
#[tauri::command]
pub fn put_back_document(
	root: PathBuf,
	version: String,
	id: Uuid,
	label: Option<String>,
) -> Result<()> {
	let repo = keep_before(&root, label)?;
	let (files, then) = side(&repo, Some(&version))?;
	let now = read_manifest(&root)?;
	let Some(node) = tree::find(&then.nodes, id).filter(|n| matches!(n, Node::Document { .. }))
	else {
		return match tree::find(&now.nodes, id) {
			Some(Node::Document { .. }) => delete_document(root, id),
			_ => Err(Error::UnknownDocument),
		};
	};

	let path = tree::path(&then.nodes, id).ok_or(Error::UnknownDocument)?;
	let blob = blob_at(&files, &path).ok_or(Error::DocumentMissing)?;
	let text = repo.find_blob(blob)?.content().to_vec();
	let (parent, index) = destination(&then, &now, id)?;
	match tree::find(&now.nodes, id) {
		None => recreate_document(&root, parent, index, node.clone(), &text),
		Some(Node::Folder { .. }) => Err(Error::UnknownDocument),
		Some(Node::Document { name, .. }) => {
			if name != node.name() {
				rename_document(root.clone(), id, node.name().to_owned())?;
			}
			if tree::parent(&now.nodes, id).map(Node::id) != Some(parent) {
				move_node(root.clone(), id, parent, index)?;
			}
			let text = String::from_utf8(text).map_err(|_| Error::NotText)?;
			write_document(root, id, text)
		}
	}
}

/// Puts back the book's details and its cover as a version had them.
#[tauri::command]
pub fn put_back_book(root: PathBuf, version: String, label: Option<String>) -> Result<()> {
	let repo = keep_before(&root, label)?;
	let (files, then) = side(&repo, Some(&version))?;
	for name in COVER_NAMES {
		match blob_at(&files, name).filter(|_| then.book.cover == name) {
			Some(blob) => write_atomic(&root.join(name), repo.find_blob(blob)?.content())?,
			None => remove_if_there(&root.join(name))?,
		}
	}
	write_book(root, then.book)
}

/// Puts the whole project back as a version had it. Stats and the Trash are
/// never part of a version, so they stay as they are.
#[tauri::command]
pub fn put_back_everything(root: PathBuf, version: String, label: Option<String>) -> Result<()> {
	let repo = keep_before(&root, label)?;
	let (files, then) = side(&repo, Some(&version))?;
	let folders = |manifest: &Manifest| {
		let mut placed = Vec::new();
		place(&manifest.nodes, None, "", &mut placed);
		placed
			.into_iter()
			.filter(|(_, node)| node.folder)
			.map(|(_, node)| node.path)
			// The manifest is a file the writer can edit.
			.filter(|path| {
				Path::new(path)
					.components()
					.all(|c| matches!(c, Component::Normal(_)))
			})
			.collect::<Vec<_>>()
	};
	let was = folders(&read_manifest(&root)?);

	// The version just kept holds every file, so a forced checkout removes
	// what the older version did not have and leaves ignored files alone.
	repo.checkout_tree(files.as_object(), Some(CheckoutBuilder::new().force()))?;

	// Git keeps files, not folders, so empty ones are made and taken away here.
	let is = folders(&then);
	let mut gone: Vec<&String> = was.iter().filter(|path| !is.contains(path)).collect();
	gone.sort_by_key(|path| std::cmp::Reverse(path.len()));
	for path in gone {
		// Only an empty folder goes, so a failure leaves nothing lost.
		let _ = fs::remove_dir(root.join(path));
	}
	for path in &is {
		fs::create_dir_all(root.join(path))?;
	}
	Ok(())
}

fn enclosing(root: &Path, repo: &Repository) -> Result<Option<PathBuf>> {
	if repo
		.config()?
		.get_bool(NESTED_NOTICE_SHOWN)
		.unwrap_or(false)
	{
		return Ok(None);
	}
	let Some(parent) = root.parent() else {
		return Ok(None);
	};
	// Only ever a notice, so a repository that cannot be read counts as none.
	Ok(Repository::discover(parent)
		.ok()
		.and_then(|outer| outer.workdir().map(Path::to_path_buf)))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::document::{
		create_document, create_folder, delete_document, delete_folder, list_trash, move_node,
		read_document, rename_document, rename_folder, write_document,
	};
	use crate::project::{Format, create, set_cover, write_book};
	use crate::tree::Node;

	fn project() -> tempfile::TempDir {
		let dir = tempfile::tempdir().unwrap();
		fs::create_dir(dir.path().join("Manuscript")).unwrap();
		fs::write(dir.path().join("Manuscript/one.md"), "Call me Ishmael.").unwrap();
		fs::write(dir.path().join("aurora.json"), "{}").unwrap();
		dir
	}

	fn versions(repo: &Repository) -> usize {
		let mut walk = repo.revwalk().unwrap();
		walk.push_head().unwrap();
		walk.count()
	}

	#[test]
	fn a_fresh_folder_begins_with_one_version() {
		let dir = project();
		assert_eq!(ensure_repository(dir.path(), "").unwrap(), None);

		let repo = Repository::open(dir.path()).unwrap();
		assert_eq!(versions(&repo), 1);
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		assert_eq!(head.summary().unwrap(), Some(FIRST_VERSION));
		assert_eq!(head.author().name().unwrap(), "Aurora");
		assert_eq!(head.author().email().unwrap(), AUTHOR_EMAIL);
		let tree = head.tree().unwrap();
		assert!(tree.get_path(Path::new("Manuscript/one.md")).is_ok());
		assert!(tree.get_path(Path::new("aurora.json")).is_ok());
		assert_eq!(
			repo.find_note(None, head.id()).unwrap().message().unwrap(),
			FIRST_VERSION
		);
	}

	#[test]
	fn the_first_version_is_by_the_books_author() {
		let dir = project();
		ensure_repository(dir.path(), "Herman Melville").unwrap();

		let repo = Repository::open(dir.path()).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		assert_eq!(head.author().name().unwrap(), "Herman Melville");
	}

	#[test]
	fn opening_again_keeps_no_new_version() {
		let dir = project();
		ensure_repository(dir.path(), "").unwrap();
		ensure_repository(dir.path(), "").unwrap();

		let repo = Repository::open(dir.path()).unwrap();
		assert_eq!(versions(&repo), 1);
		let exclude = fs::read_to_string(repo.path().join("info/exclude")).unwrap();
		assert_eq!(exclude.matches("/history.json").count(), 1);
	}

	#[test]
	fn an_existing_repository_is_adopted_untouched() {
		let dir = project();
		let repo = Repository::init(dir.path()).unwrap();
		let mut index = repo.index().unwrap();
		index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
		let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
		let signature = Signature::now("Writer", "writer@example.com").unwrap();
		let theirs = repo
			.commit(Some("HEAD"), &signature, &signature, "Mine", &tree, &[])
			.unwrap();

		ensure_repository(dir.path(), "").unwrap();

		assert_eq!(repo.head().unwrap().target(), Some(theirs));
		assert_eq!(versions(&repo), 1);
	}

	#[test]
	fn excluded_files_are_never_kept() {
		let dir = project();
		fs::write(dir.path().join(HISTORY_FILE), "{}").unwrap();
		fs::create_dir(dir.path().join(TRASH_DIR)).unwrap();
		fs::write(dir.path().join(TRASH_DIR).join("gone.md"), "gone").unwrap();

		ensure_repository(dir.path(), "").unwrap();

		let repo = Repository::open(dir.path()).unwrap();
		let tree = repo.head().unwrap().peel_to_tree().unwrap();
		assert!(tree.get_path(Path::new(HISTORY_FILE)).is_err());
		assert!(tree.get_path(Path::new(TRASH_DIR)).is_err());
		assert!(tree.get_path(Path::new("aurora.json")).is_ok());
	}

	#[test]
	fn a_project_inside_another_repository_says_so_until_told() {
		let outer = tempfile::tempdir().unwrap();
		Repository::init(outer.path()).unwrap();
		let root = outer.path().join("Ithaca");
		fs::create_dir(&root).unwrap();
		fs::write(root.join("aurora.json"), "{}").unwrap();

		let found = ensure_repository(&root, "").unwrap().unwrap();
		// Compared by name: the platforms disagree about how to spell the path.
		assert_eq!(found.file_name(), outer.path().file_name());
		assert!(root.join(".git").is_dir());

		Repository::open(&root)
			.unwrap()
			.config()
			.unwrap()
			.set_bool(NESTED_NOTICE_SHOWN, true)
			.unwrap();
		assert_eq!(ensure_repository(&root, "").unwrap(), None);
	}

	/// A project with its first version, and a change waiting to be kept.
	fn changed() -> (tempfile::TempDir, Repository) {
		let dir = project();
		ensure_repository(dir.path(), "").unwrap();
		fs::write(
			dir.path().join("Manuscript/one.md"),
			"Call me Ishmael. Some",
		)
		.unwrap();
		let repo = Repository::open(dir.path()).unwrap();
		(dir, repo)
	}

	fn pair(key: &str, value: &str) -> (String, String) {
		(key.to_owned(), value.to_owned())
	}

	#[test]
	fn a_session_keeps_its_counts_and_no_name() {
		let (dir, repo) = changed();
		let session = Keep::Session {
			minutes: 25,
			written: 312,
			removed: 40,
		};
		let id = keep(dir.path(), &session, "").unwrap();

		let commit = repo.find_commit(id).unwrap();
		assert_eq!(commit.parent_count(), 1);
		assert_eq!(
			trailers(&commit).unwrap(),
			[
				pair("Aurora-Kind", "session"),
				pair("Aurora-Minutes", "25"),
				pair("Aurora-Written", "312"),
				pair("Aurora-Removed", "40"),
			]
		);
		assert_eq!(name_of(&repo, &commit).unwrap(), None);
	}

	#[test]
	fn closing_keeps_its_counts_without_minutes() {
		let (dir, repo) = changed();
		let closing = Keep::Closing {
			written: 5,
			removed: 0,
		};
		let id = keep(dir.path(), &closing, "").unwrap();

		assert_eq!(
			trailers(&repo.find_commit(id).unwrap()).unwrap(),
			[
				pair("Aurora-Kind", "closing"),
				pair("Aurora-Written", "5"),
				pair("Aurora-Removed", "0"),
			]
		);
	}

	#[test]
	fn a_named_version_is_named() {
		let (dir, repo) = changed();
		let named = Keep::Named {
			name: "Before the storm".to_owned(),
		};
		let id = keep(dir.path(), &named, "").unwrap();

		let commit = repo.find_commit(id).unwrap();
		assert_eq!(trailers(&commit).unwrap(), [pair("Aurora-Kind", "named")]);
		assert_eq!(
			name_of(&repo, &commit).unwrap().as_deref(),
			Some("Before the storm")
		);
	}

	#[test]
	fn putting_back_names_what_it_saved_the_writer_from() {
		let (dir, repo) = changed();
		let before = Keep::BeforePuttingBack {
			label: "Before the storm".to_owned(),
		};
		let id = keep(dir.path(), &before, "").unwrap();

		let commit = repo.find_commit(id).unwrap();
		assert_eq!(
			trailers(&commit).unwrap(),
			[pair("Aurora-Kind", "beforePuttingBack")]
		);
		assert_eq!(
			name_of(&repo, &commit).unwrap().as_deref(),
			Some("Before putting back “Before the storm”")
		);
	}

	#[test]
	fn a_deleted_document_leaves_the_version() {
		let (dir, repo) = changed();
		fs::remove_file(dir.path().join("Manuscript/one.md")).unwrap();
		let closing = Keep::Closing {
			written: 0,
			removed: 3,
		};
		let id = keep(dir.path(), &closing, "").unwrap();

		let tree = repo.find_commit(id).unwrap().tree().unwrap();
		assert!(tree.get_path(Path::new("Manuscript/one.md")).is_err());
	}

	#[test]
	fn nothing_changed_keeps_nothing() {
		let dir = project();
		ensure_repository(dir.path(), "").unwrap();
		let repo = Repository::open(dir.path()).unwrap();
		let head = repo.head().unwrap().target().unwrap();

		let session = Keep::Session {
			minutes: 1,
			written: 0,
			removed: 0,
		};
		assert_eq!(keep(dir.path(), &session, "").unwrap(), head);
		assert_eq!(versions(&repo), 1);
	}

	#[test]
	fn naming_with_nothing_changed_names_the_latest() {
		let (dir, repo) = changed();
		let session = Keep::Session {
			minutes: 1,
			written: 1,
			removed: 0,
		};
		let latest = keep(dir.path(), &session, "").unwrap();

		let named = Keep::Named {
			name: "Draft one".to_owned(),
		};
		assert_eq!(keep(dir.path(), &named, "").unwrap(), latest);
		assert_eq!(versions(&repo), 2);
		let commit = repo.find_commit(latest).unwrap();
		assert_eq!(
			name_of(&repo, &commit).unwrap().as_deref(),
			Some("Draft one")
		);
	}

	#[test]
	fn naming_an_already_named_latest_keeps_an_empty_version() {
		let dir = project();
		ensure_repository(dir.path(), "").unwrap();
		let repo = Repository::open(dir.path()).unwrap();
		let first = repo.head().unwrap().peel_to_commit().unwrap();

		let named = Keep::Named {
			name: "Draft one".to_owned(),
		};
		let id = keep(dir.path(), &named, "").unwrap();

		let commit = repo.find_commit(id).unwrap();
		assert_ne!(id, first.id());
		assert_eq!(commit.tree_id(), first.tree_id());
		assert_eq!(
			name_of(&repo, &commit).unwrap().as_deref(),
			Some("Draft one")
		);
		assert_eq!(
			name_of(&repo, &first).unwrap().as_deref(),
			Some(FIRST_VERSION)
		);
	}

	#[test]
	fn a_version_can_be_renamed_and_unnamed() {
		let dir = project();
		ensure_repository(dir.path(), "").unwrap();
		let repo = Repository::open(dir.path()).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		let id = head.id().to_string();

		set_name(dir.path(), &id, "The very start", "").unwrap();
		assert_eq!(
			name_of(&repo, &head).unwrap().as_deref(),
			Some("The very start")
		);
		set_name(dir.path(), &id, "", "").unwrap();
		assert_eq!(name_of(&repo, &head).unwrap(), None);
	}

	#[test]
	fn versions_come_newest_first_with_their_names_and_counts() {
		let (dir, _repo) = changed();
		let session = Keep::Session {
			minutes: 25,
			written: 3,
			removed: 0,
		};
		keep(dir.path(), &session, "").unwrap();
		fs::write(dir.path().join("Manuscript/two.md"), "More").unwrap();
		let named = Keep::Named {
			name: "Draft one".to_owned(),
		};
		keep(dir.path(), &named, "Herman Melville").unwrap();

		let listed = list(dir.path(), None).unwrap();
		let rows: Vec<_> = listed
			.iter()
			.map(|v| (v.kind, v.name.as_deref(), v.minutes, v.written))
			.collect();
		assert_eq!(
			rows,
			[
				(Some(Kind::Named), Some("Draft one"), None, None),
				(Some(Kind::Session), None, Some(25), Some(3)),
				(Some(Kind::Named), Some(FIRST_VERSION), None, None),
			]
		);
		assert_eq!(listed[0].author, "Herman Melville");
		assert_eq!(listed[2].author, "Aurora");
	}

	#[test]
	fn a_document_lists_only_the_versions_that_changed_its_text() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(
			parent.path(),
			"Ithaca",
			Format::Novel,
			OffsetDateTime::UNIX_EPOCH,
		)
		.unwrap();
		ensure_repository(&root, "").unwrap();
		let manifest = read_manifest(&root).unwrap();
		let seeded = &manifest.documents()[0];
		let document = seeded.id;
		let elsewhere = manifest
			.nodes
			.iter()
			.find_map(|node| match node {
				Node::Folder { id, name, .. } if !seeded.path.starts_with(&format!("{name}/")) => {
					Some(*id)
				}
				_ => None,
			})
			.unwrap();
		let session = || Keep::Session {
			minutes: 1,
			written: 1,
			removed: 0,
		};
		let mut expected = vec![repo_head(&root)];

		write_document(root.clone(), document, "One".to_owned()).unwrap();
		expected.push(keep(&root, &session(), "").unwrap());

		let other = create_document(root.clone(), elsewhere, "Other".to_owned())
			.unwrap()
			.id;
		let other_added = keep(&root, &session(), "").unwrap();

		rename_document(root.clone(), document, "Renamed".to_owned()).unwrap();
		keep(&root, &session(), "").unwrap();

		write_document(root.clone(), document, "One two".to_owned()).unwrap();
		expected.push(keep(&root, &session(), "").unwrap());

		move_node(root.clone(), document, elsewhere, 0).unwrap();
		write_document(root.clone(), document, "One two three".to_owned()).unwrap();
		expected.push(keep(&root, &session(), "").unwrap());

		let ids = |document| -> Vec<Oid> {
			list(&root, Some(document))
				.unwrap()
				.iter()
				.map(|v| Oid::from_str(&v.id).unwrap())
				.collect()
		};
		expected.reverse();
		assert_eq!(ids(document), expected);
		let words: Vec<_> = list(&root, Some(document))
			.unwrap()
			.iter()
			.map(|v| v.words)
			.collect();
		assert_eq!(words, [Some(3), Some(2), Some(1), Some(0)]);
		assert_eq!(ids(other), [other_added]);
	}

	fn repo_head(root: &Path) -> Oid {
		Repository::open(root)
			.unwrap()
			.head()
			.unwrap()
			.target()
			.unwrap()
	}

	#[test]
	fn a_foreign_commit_is_named_by_its_subject_until_unnamed() {
		let dir = project();
		let repo = Repository::init(dir.path()).unwrap();
		let mut index = repo.index().unwrap();
		index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
		let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
		let signature = Signature::now("Writer", "writer@example.com").unwrap();
		let id = repo
			.commit(
				Some("HEAD"),
				&signature,
				&signature,
				"Chapter one done\n\nLong notes.",
				&tree,
				&[],
			)
			.unwrap();
		let commit = repo.find_commit(id).unwrap();

		assert_eq!(
			name_of(&repo, &commit).unwrap().as_deref(),
			Some("Chapter one done")
		);
		set_name(dir.path(), &id.to_string(), "", "").unwrap();
		assert_eq!(name_of(&repo, &commit).unwrap(), None);
	}

	/// A novel with versions, its first document, and two top-level folders
	/// that do not hold that document.
	fn novel() -> (tempfile::TempDir, PathBuf, Uuid, Uuid, Uuid) {
		let parent = tempfile::tempdir().unwrap();
		let root = create(
			parent.path(),
			"Ithaca",
			Format::Novel,
			OffsetDateTime::UNIX_EPOCH,
		)
		.unwrap();
		ensure_repository(&root, "").unwrap();
		let manifest = read_manifest(&root).unwrap();
		let seeded = manifest.documents()[0].clone();
		let others: Vec<Uuid> = manifest
			.nodes
			.iter()
			.filter_map(|node| match node {
				Node::Folder { id, name, .. } if !seeded.path.starts_with(&format!("{name}/")) => {
					Some(*id)
				}
				_ => None,
			})
			.collect();
		(parent, root, seeded.id, others[0], others[1])
	}

	fn kept(root: &Path) -> String {
		let session = Keep::Session {
			minutes: 1,
			written: 1,
			removed: 0,
		};
		keep(root, &session, "").unwrap().to_string()
	}

	fn change_of(changes: &Changes, id: Uuid) -> Option<&Change> {
		changes.nodes.iter().find(|change| change.id == id)
	}

	#[test]
	fn documents_are_matched_by_id_through_every_kind_of_change() {
		let (_parent, root, seeded, here, there) = novel();
		write_document(root.clone(), seeded, "One".to_owned()).unwrap();
		let add = |name: &str| {
			create_document(root.clone(), here, name.to_owned())
				.unwrap()
				.id
		};
		let (moving, doomed, untouched) = (add("Moving"), add("Doomed"), add("Untouched"));
		let start = kept(&root);

		rename_document(root.clone(), seeded, "Renamed".to_owned()).unwrap();
		write_document(root.clone(), seeded, "One two".to_owned()).unwrap();
		move_node(root.clone(), moving, there, 0).unwrap();
		delete_document(root.clone(), doomed).unwrap();
		let added = add("Added");
		let end = kept(&root);

		let changes = changes(&root, Some(&start), Some(&end)).unwrap();
		assert_eq!(changes.nodes.len(), 4);
		assert!(change_of(&changes, untouched).is_none());

		let renamed = change_of(&changes, seeded).unwrap();
		assert!(renamed.renamed && renamed.edited && !renamed.moved);
		assert!(renamed.after.as_deref().unwrap().ends_with("Renamed.md"));

		let moved = change_of(&changes, moving).unwrap();
		assert!(moved.moved && !moved.renamed && !moved.edited);

		let deleted = change_of(&changes, doomed).unwrap();
		assert!(deleted.before.is_some() && deleted.after.is_none());

		let added = change_of(&changes, added).unwrap();
		assert!(added.before.is_none() && added.after.is_some() && !added.folder);
		assert!(!changes.book && !changes.cover);
	}

	#[test]
	fn a_renamed_folder_does_not_move_what_is_inside_it() {
		let (_parent, root, _, here, there) = novel();
		let folder = |parent: Uuid, name: &str| {
			create_folder(root.clone(), parent, name.to_owned(), None)
				.unwrap()
				.id()
		};
		let (old, moving, doomed) = (
			folder(here, "Old"),
			folder(here, "Moving"),
			folder(here, "Doomed"),
		);
		let inside = create_document(root.clone(), old, "Inside".to_owned())
			.unwrap()
			.id;
		let start = kept(&root);

		rename_folder(root.clone(), old, "New".to_owned()).unwrap();
		move_node(root.clone(), moving, there, 0).unwrap();
		delete_folder(root.clone(), doomed).unwrap();
		let fresh = folder(here, "Fresh");
		let end = kept(&root);

		let changes = changes(&root, Some(&start), Some(&end)).unwrap();
		assert!(change_of(&changes, inside).is_none());
		let renamed = change_of(&changes, old).unwrap();
		assert!(renamed.folder && renamed.renamed && !renamed.moved);
		assert!(change_of(&changes, moving).unwrap().moved);
		assert!(change_of(&changes, doomed).unwrap().after.is_none());
		assert!(change_of(&changes, fresh).unwrap().before.is_none());
	}

	#[test]
	fn a_folder_reordered_is_one_change_and_an_addition_is_not_a_reorder() {
		let (_parent, root, _, here, there) = novel();
		let add = |parent: Uuid, name: &str| {
			create_document(root.clone(), parent, name.to_owned())
				.unwrap()
				.id
		};
		let (_, _, last) = (add(here, "One"), add(here, "Two"), add(here, "Three"));
		let start = kept(&root);

		move_node(root.clone(), last, here, 0).unwrap();
		add(there, "New");
		let end = kept(&root);

		let changes = changes(&root, Some(&start), Some(&end)).unwrap();
		let folder = change_of(&changes, here).unwrap();
		assert!(!folder.renamed && !folder.moved);
		assert_eq!(folder.reordered.len(), 1);
		assert!(folder.reordered[0].name.starts_with("Three"));
		assert_eq!(folder.reordered[0].after, None);
		assert!(change_of(&changes, last).is_none());
		assert!(change_of(&changes, there).is_none());

		// To the end, now after the last of the others.
		let held = read_manifest(&root)
			.unwrap()
			.nodes
			.iter()
			.find_map(|node| match node {
				Node::Folder { id, children, .. } if *id == here => Some(children.len()),
				_ => None,
			})
			.unwrap();
		move_node(root.clone(), last, here, held - 1).unwrap();
		let later = kept(&root);
		let changes = super::changes(&root, Some(&end), Some(&later)).unwrap();
		let moved = &change_of(&changes, here).unwrap().reordered;
		assert_eq!(moved.len(), 1);
		assert!(moved[0].name.starts_with("Three"));
		assert!(moved[0].after.as_deref().unwrap().starts_with("Two"));
	}

	#[test]
	fn book_details_and_the_cover_are_told_apart() {
		let (_parent, root, ..) = novel();
		let start = kept(&root);
		let mut book = read_manifest(&root).unwrap().book;
		book.title = "Odyssey".to_owned();
		write_book(root.clone(), book).unwrap();
		let retitled = kept(&root);

		let image = root.parent().unwrap().join("cover.png");
		fs::write(&image, b"\x89PNG\r\n\x1a\none").unwrap();
		set_cover(root.clone(), image.clone()).unwrap();
		let covered = kept(&root);
		fs::write(&image, b"\x89PNG\r\n\x1a\ntwo").unwrap();
		set_cover(root.clone(), image).unwrap();
		let recovered = kept(&root);

		let book = changes(&root, Some(&start), Some(&retitled)).unwrap();
		assert!(book.book && !book.cover && book.nodes.is_empty());
		let cover = changes(&root, Some(&retitled), Some(&covered)).unwrap();
		assert!(cover.cover && !cover.book);
		// The same file name, with other bytes in it.
		assert!(
			changes(&root, Some(&covered), Some(&recovered))
				.unwrap()
				.cover
		);
	}

	#[test]
	fn now_is_the_folder_on_disk_and_keeps_nothing() {
		let (_parent, root, seeded, ..) = novel();
		let head = repo_head(&root);
		write_document(root.clone(), seeded, "Unkept".to_owned()).unwrap();

		let changes = changes(&root, Some(&head.to_string()), None).unwrap();
		assert!(change_of(&changes, seeded).unwrap().edited);
		assert_eq!(repo_head(&root), head);
	}

	#[test]
	fn a_version_gives_back_a_documents_text_wherever_it_went() {
		let (_parent, root, seeded, _, there) = novel();
		write_document(root.clone(), seeded, "First".to_owned()).unwrap();
		let first = kept(&root);
		rename_document(root.clone(), seeded, "Renamed".to_owned()).unwrap();
		move_node(root.clone(), seeded, there, 0).unwrap();
		write_document(root.clone(), seeded, "Second".to_owned()).unwrap();
		let second = kept(&root);

		let body_of = |version: &str| {
			text(&root, version, seeded)
				.unwrap()
				.map(|t| body(&t).to_owned())
		};
		assert_eq!(body_of(&first).as_deref(), Some("First"));
		assert_eq!(body_of(&second).as_deref(), Some("Second"));
		assert_eq!(text(&root, &first, Uuid::new_v4()).unwrap(), None);
	}

	fn body_now(root: &Path, id: Uuid) -> String {
		body(&read_document(root.to_path_buf(), id).unwrap()).to_owned()
	}

	#[test]
	fn a_document_comes_back_with_its_text_name_and_folder() {
		let (_parent, root, seeded, _, there) = novel();
		write_document(root.clone(), seeded, "First".to_owned()).unwrap();
		let first = kept(&root);
		rename_document(root.clone(), seeded, "Renamed".to_owned()).unwrap();
		move_node(root.clone(), seeded, there, 0).unwrap();
		write_document(root.clone(), seeded, "Second".to_owned()).unwrap();

		put_back_document(
			root.clone(),
			first.clone(),
			seeded,
			Some("First".to_owned()),
		)
		.unwrap();

		assert!(changes(&root, Some(&first), None).unwrap().nodes.is_empty());
		assert_eq!(body_now(&root, seeded), "First");
		let before = list(&root, None).unwrap().remove(0);
		assert_eq!(before.kind, Some(Kind::BeforePuttingBack));
		assert_eq!(before.name.as_deref(), Some("Before putting back “First”"));
		let then = text(&root, &before.id, seeded).unwrap().unwrap();
		assert_eq!(body(&then), "Second");
	}

	#[test]
	fn a_deleted_document_returns_under_its_id_and_a_newer_one_is_trashed() {
		let (_parent, root, _, here, _) = novel();
		let add = |name: &str| {
			create_document(root.clone(), here, name.to_owned())
				.unwrap()
				.id
		};
		let (_, doomed, _) = (add("One"), add("Doomed"), add("Three"));
		write_document(root.clone(), doomed, "Kept".to_owned()).unwrap();
		let start = kept(&root);
		delete_document(root.clone(), doomed).unwrap();
		let newer = add("Newer");

		put_back_document(
			root.clone(),
			start.clone(),
			doomed,
			Some("start".to_owned()),
		)
		.unwrap();
		let shared = list(&root, None).unwrap().len();
		// The same visit, which has kept its version already.
		put_back_document(root.clone(), start.clone(), newer, None).unwrap();

		// Nothing differs, so the id and the place among the others are back.
		assert!(changes(&root, Some(&start), None).unwrap().nodes.is_empty());
		assert_eq!(body_now(&root, doomed), "Kept");
		assert_eq!(list_trash(root.clone()).unwrap().len(), 2);
		assert_eq!(list(&root, None).unwrap().len(), shared);
	}

	#[test]
	fn the_book_and_its_cover_come_back() {
		let (_parent, root, ..) = novel();
		let png = root.parent().unwrap().join("cover.png");
		fs::write(&png, b"\x89PNG\r\n\x1a\none").unwrap();
		set_cover(root.clone(), png).unwrap();
		let start = kept(&root);
		let mut book = read_manifest(&root).unwrap().book;
		book.title = "Odyssey".to_owned();
		write_book(root.clone(), book).unwrap();
		let jpeg = root.parent().unwrap().join("cover.jpg");
		fs::write(&jpeg, b"\xff\xd8\xff").unwrap();
		set_cover(root.clone(), jpeg).unwrap();

		put_back_book(root.clone(), start.clone(), Some("start".to_owned())).unwrap();

		let changes = changes(&root, Some(&start), None).unwrap();
		assert!(!changes.book && !changes.cover);
		assert!(root.join("cover.png").is_file());
		assert!(!root.join("cover.jpg").exists());
		assert_eq!(
			list(&root, None).unwrap()[0].kind,
			Some(Kind::BeforePuttingBack)
		);
	}

	#[test]
	fn everything_comes_back_but_stats_and_the_trash() {
		let (_parent, root, seeded, here, there) = novel();
		let folder = |parent: Uuid, name: &str| {
			create_folder(root.clone(), parent, name.to_owned(), None)
				.unwrap()
				.id()
		};
		let path_of = |id: Uuid| tree::path(&read_manifest(&root).unwrap().nodes, id).unwrap();
		let empty = folder(here, "Empty");
		let empty_path = path_of(empty);
		write_document(root.clone(), seeded, "First".to_owned()).unwrap();
		let start = kept(&root);

		write_document(root.clone(), seeded, "Second".to_owned()).unwrap();
		rename_document(root.clone(), seeded, "Renamed".to_owned()).unwrap();
		delete_folder(root.clone(), empty).unwrap();
		let later = folder(there, "Later");
		create_document(root.clone(), later, "Inside".to_owned()).unwrap();
		let later_path = path_of(later);
		fs::write(root.join(HISTORY_FILE), "[]").unwrap();

		put_back_everything(root.clone(), start.clone(), Some("start".to_owned())).unwrap();

		assert!(changes(&root, Some(&start), None).unwrap().nodes.is_empty());
		assert_eq!(body_now(&root, seeded), "First");
		assert!(root.join(&empty_path).is_dir());
		assert!(!root.join(&later_path).exists());
		assert_eq!(fs::read_to_string(root.join(HISTORY_FILE)).unwrap(), "[]");
		assert_eq!(list_trash(root.clone()).unwrap().len(), 1);
		let before = list(&root, None).unwrap().remove(0);
		assert_eq!(before.kind, Some(Kind::BeforePuttingBack));
		assert!(
			changes(&root, Some(&before.id), None)
				.unwrap()
				.nodes
				.iter()
				.any(|change| change.id == later && change.after.is_none())
		);
	}
}
