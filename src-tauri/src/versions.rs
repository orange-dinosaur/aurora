//! Every project keeps its versions as a git repository at its own root.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use git2::build::CheckoutBuilder;
use git2::{
	BranchType, Commit, ErrorCode, Index, IndexAddOption, IndexEntry, IndexTime, MergeFileInput,
	MergeFileOptions, Oid, Repository, RepositoryInitOptions, ResetType, Signature, Sort,
	StatusOptions, Tree,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::document::{
	TRASH_DIR, body, delete_document, document_path, move_node, recreate_document, rename_document,
	write_document,
};
use crate::history::HISTORY_FILE;
use crate::merge;
use crate::project::{
	Book, COVER_NAMES, Error, Format, MANIFEST_FILE, Manifest, Result, read_manifest,
	remove_if_there, to_json, write_atomic, write_book,
};
use crate::tree::{self, Node};

const AUTHOR_EMAIL: &str = "aurora@localhost";

/// One row of a history of checkpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Checkpoint {
	pub id: String,
	#[serde(with = "time::serde::rfc3339")]
	#[ts(type = "string")]
	pub at: OffsetDateTime,
	pub message: String,
	pub author: String,
	/// The document's words at this checkpoint, when the list is of one document.
	pub words: Option<u32>,
}

/// One version of the project, which git calls a branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Branch {
	pub slug: String,
	/// The name as the writer typed it.
	pub name: String,
	pub current: bool,
	/// Checkpoints no other version reaches, which deleting this one loses.
	pub unmerged: u32,
	/// Main, or a checkout no version names: neither can be renamed or
	/// deleted.
	pub fixed: bool,
}

const MAIN: &str = "main";
/// What Main is in an adopted repository that has no `main`.
const MASTER: &str = "master";

/// Set once the writer has been told about an enclosing repository. It lives
/// in the project's own git config, which is local and never pushed.
pub const NESTED_NOTICE_SHOWN: &str = "aurora.nestedNoticeShown";

/// Kept out of every version, so going back never rolls back Stats or the
/// Trash. Written to `.git/info/exclude` so the project folder gains no file.
fn excluded() -> [String; 2] {
	[format!("/{HISTORY_FILE}"), format!("/{TRASH_DIR}/")]
}

/// Opens the repository at `root`, making one on Main with nothing kept yet
/// when there is none. Returns the folder of a repository the project sits
/// inside, while the writer has not yet been told about it.
pub fn ensure_repository(root: &Path) -> Result<Option<PathBuf>> {
	// `open` looks only at `root`: a repository further up belongs to
	// something else, and the project still gets its own.
	let repo = match Repository::open(root) {
		Ok(repo) => repo,
		Err(e) if e.code() == ErrorCode::NotFound => {
			// Named here so the machine's `init.defaultBranch` has no say.
			Repository::init_opts(root, RepositoryInitOptions::new().initial_head("main"))?
		}
		Err(e) => return Err(e.into()),
	};
	exclude(&repo)?;
	enclosing(root, &repo)
}

/// The commit `HEAD` points at, or none on a branch with nothing kept yet.
fn head_commit(repo: &Repository) -> Result<Option<Commit<'_>>> {
	match repo.head() {
		Ok(head) => Ok(Some(head.peel_to_commit()?)),
		Err(e) if e.code() == ErrorCode::UnbornBranch => Ok(None),
		Err(e) => Err(e.into()),
	}
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

fn signature(author: &str) -> Result<Signature<'static>> {
	let name = if author.trim().is_empty() {
		"Aurora"
	} else {
		author
	};
	// libgit2 refuses an empty email, and a writer has none to give.
	Ok(Signature::now(name, AUTHOR_EMAIL)?)
}

/// The whole project as it stands on disk, deletions included, in memory
/// only: `.git/index` keeps whatever the writer staged.
fn working(repo: &Repository) -> Result<Index> {
	let mut index = repo.index()?;
	index.add_all(["*"], IndexAddOption::DEFAULT, None)?;
	index.update_all(["*"], None)?;
	Ok(index)
}

/// Stages the whole project and returns its tree.
fn stage(repo: &Repository) -> Result<Oid> {
	let mut index = working(repo)?;
	index.write()?;
	Ok(index.write_tree()?)
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

/// `start` and the commits before it along first parents, newest first.
fn first_parents(repo: &Repository, start: Oid) -> Result<git2::Revwalk<'_>> {
	let mut walk = repo.revwalk()?;
	walk.set_sorting(Sort::TOPOLOGICAL)?;
	walk.simplify_first_parent()?;
	walk.push(start)?;
	Ok(walk)
}

/// The document's words in `commit` when its text differs from the first
/// parent's, 0 when it was removed, and none when its text is unchanged.
fn changed_words(
	repo: &Repository,
	commit: &Commit,
	document: Uuid,
	paths: &mut HashMap<Oid, Option<String>>,
) -> Result<Option<u32>> {
	let before = match commit.parent(0) {
		Ok(parent) => text_in(repo, &parent, document, paths)?,
		Err(_) => None,
	};
	let now = text_in(repo, commit, document, paths)?;
	if now == before {
		return Ok(None);
	}
	Ok(Some(match now {
		Some(blob) => words_in(repo.find_blob(blob)?.content()),
		None => 0,
	}))
}

/// Commits what the next checkpoint holds with the writer's message, or the
/// whole project as it is when nothing is staged, and returns the new
/// checkpoint.
fn checkpoint(root: &Path, message: &str, author: &str) -> Result<Oid> {
	if message.trim().is_empty() {
		return Err(Error::EmptyMessage);
	}
	let repo = Repository::open(root)?;
	if repo.index()?.has_conflicts() {
		return Err(Error::ConflictsRemain);
	}
	let staged = compare(&side(&repo, Some("HEAD"))?, &staged_side(&repo)?);
	let tree = match staged.nodes.is_empty() && !staged.book && !staged.cover {
		true => stage(&repo)?,
		false => repo.index()?.write_tree()?,
	};
	let tree = repo.find_tree(tree)?;
	let signature = signature(author)?;
	let head = head_commit(&repo)?;
	let merged = merge_head(&repo)?;
	let parents: Vec<&Commit> = head.iter().chain(merged.iter()).collect();
	let id = repo.commit(
		Some("HEAD"),
		&signature,
		&signature,
		message,
		&tree,
		&parents,
	)?;
	// Ends the merge under way, if there is one.
	repo.cleanup_state()?;
	Ok(id)
}

/// The checkpoints of a version, newest first along first parents, or of the
/// one the writer is in when `version` is none. A version with nothing in it
/// yet has no rows. With a document, only the checkpoints that changed its
/// text.
fn checkpoints(
	root: &Path,
	version: Option<&str>,
	document: Option<Uuid>,
) -> Result<Vec<Checkpoint>> {
	let repo = Repository::open(root)?;
	let start = match version {
		None => head_commit(&repo)?,
		Some(name) => match repo.find_branch(name, BranchType::Local) {
			Ok(branch) => Some(branch.get().peel_to_commit()?),
			Err(e) if e.code() == ErrorCode::NotFound => None,
			Err(e) => return Err(e.into()),
		},
	};
	let Some(start) = start else {
		return Ok(Vec::new());
	};

	let mut rows = Vec::new();
	let mut paths = HashMap::new();
	for id in first_parents(&repo, start.id())? {
		let commit = repo.find_commit(id?)?;
		let words = match document {
			Some(document) => match changed_words(&repo, &commit, document, &mut paths)? {
				Some(words) => Some(words),
				None => continue,
			},
			None => None,
		};
		rows.push(Checkpoint {
			id: commit.id().to_string(),
			at: OffsetDateTime::from_unix_timestamp(commit.time().seconds())
				.unwrap_or(OffsetDateTime::UNIX_EPOCH),
			message: String::from_utf8_lossy(commit.message_bytes())
				.trim_end()
				.to_owned(),
			author: commit.author().name().unwrap_or_default().to_owned(),
			words,
		});
	}
	Ok(rows)
}

/// The branch name for a version: lowercase, each run of anything but letters
/// and digits made one hyphen, and no hyphen at either end.
fn slug(name: &str) -> Result<String> {
	let mut slug = String::new();
	for c in name.chars().flat_map(char::to_lowercase) {
		if c.is_alphanumeric() {
			slug.push(c);
		} else if !slug.is_empty() && !slug.ends_with('-') {
			slug.push('-');
		}
	}
	let slug = slug.trim_end_matches('-');
	if slug.is_empty() {
		return Err(Error::VersionName);
	}
	Ok(slug.to_owned())
}

/// Where the typed name lives: the project's local git config, which follows
/// the branch through a rename and goes when it is deleted.
fn name_key(slug: &str) -> String {
	format!("branch.{slug}.auroraname")
}

/// The branch `HEAD` names, even one with nothing kept in it yet.
fn current_branch(repo: &Repository) -> Result<Option<String>> {
	let head = repo.find_reference("HEAD")?;
	Ok(head
		.symbolic_target()?
		.and_then(|target| target.strip_prefix("refs/heads/"))
		.map(str::to_owned))
}

/// The version shown as Main: `main`, or `master` in an adopted repository
/// that has no `main`.
fn main_slug(repo: &Repository) -> Result<&'static str> {
	let current = current_branch(repo)?;
	let has = |slug: &str| {
		repo.find_branch(slug, BranchType::Local).is_ok() || current.as_deref() == Some(slug)
	};
	Ok(if !has(MAIN) && has(MASTER) {
		MASTER
	} else {
		MAIN
	})
}

/// A checkout no version names, as its short id and checkpoint.
fn detached(repo: &Repository) -> Result<Option<(String, Oid)>> {
	if !repo.head_detached()? {
		return Ok(None);
	}
	let head = repo.head()?.peel_to_commit()?;
	let short = head.as_object().short_id()?;
	Ok(Some((
		short.as_str().unwrap_or_default().to_owned(),
		head.id(),
	)))
}

fn branches(root: &Path) -> Result<Vec<Branch>> {
	let repo = Repository::open(root)?;
	let current = current_branch(&repo)?;
	let main = main_slug(&repo)?;

	let mut tips = Vec::new();
	for branch in repo.branches(Some(BranchType::Local))? {
		let (branch, _) = branch?;
		if let Some(slug) = branch.name()? {
			tips.push((slug.to_owned(), branch.get().peel_to_commit()?.id()));
		}
	}
	// Counted like a version, since leaving it loses what only it reaches.
	let detached = detached(&repo)?;
	if let Some((short, id)) = &detached {
		tips.push((short.clone(), *id));
	}
	let left = |slug: &str| detached.as_ref().is_some_and(|(short, _)| short == slug);

	let mut rows = Vec::new();
	for (slug, tip) in &tips {
		let mut walk = repo.revwalk()?;
		walk.push(*tip)?;
		for (other, tip) in &tips {
			if other != slug {
				walk.hide(*tip)?;
			}
		}
		let mut unmerged = 0;
		for id in walk {
			id?;
			unmerged += 1;
		}
		rows.push(Branch {
			slug: slug.clone(),
			name: String::new(),
			current: current.as_ref() == Some(slug) || left(slug),
			unmerged,
			fixed: slug == main || left(slug),
		});
	}
	// Main before its first checkpoint has no branch yet, but the writer is in it.
	if let Some(current) = current
		&& !tips.iter().any(|(slug, _)| *slug == current)
	{
		rows.push(Branch {
			fixed: current == main,
			slug: current,
			name: String::new(),
			current: true,
			unmerged: 0,
		});
	}

	for row in &mut rows {
		row.name = version_name(&repo, &row.slug)?;
	}
	rows.sort_by_key(|row| (row.slug != main, row.name.to_lowercase()));
	Ok(rows)
}

/// A version's name as the writer typed it.
fn version_name(repo: &Repository, slug: &str) -> Result<String> {
	if slug == main_slug(repo)? {
		return Ok("Main".to_owned());
	}
	Ok(repo
		.config()?
		.get_string(&name_key(slug))
		.unwrap_or_else(|_| slug.to_owned()))
}

/// Starts a version at the current checkpoint and moves the writer into it.
/// Only `HEAD` moves, so changes not yet in a checkpoint come along.
fn new_branch(root: &Path, name: &str) -> Result<String> {
	let slug = slug(name)?;
	let repo = Repository::open(root)?;
	let Some(head) = head_commit(&repo)? else {
		return Err(Error::NothingKept);
	};
	match repo.branch(&slug, &head, false) {
		Err(e) if e.code() == ErrorCode::Exists => return Err(Error::VersionExists),
		result => result?,
	};
	repo.config()?.set_str(&name_key(&slug), name.trim())?;
	repo.set_head(&format!("refs/heads/{slug}"))?;
	Ok(slug)
}

/// Returns the version's new slug.
fn renamed_branch(root: &Path, slug: &str, name: &str) -> Result<String> {
	let repo = Repository::open(root)?;
	if slug == main_slug(&repo)? {
		return Err(Error::MainFixed);
	}
	let renamed = self::slug(name)?;
	let mut branch = repo.find_branch(slug, BranchType::Local)?;
	if renamed != slug {
		match branch.rename(&renamed, false) {
			Err(e) if e.code() == ErrorCode::Exists => return Err(Error::VersionExists),
			result => result?,
		};
	}
	repo.config()?.set_str(&name_key(&renamed), name.trim())?;
	Ok(renamed)
}

fn dropped_branch(root: &Path, slug: &str) -> Result<()> {
	let repo = Repository::open(root)?;
	if slug == main_slug(&repo)? {
		return Err(Error::MainFixed);
	}
	if detached(&repo)?.is_some_and(|(short, _)| short == slug) {
		return Err(Error::CurrentVersion);
	}
	let mut branch = repo.find_branch(slug, BranchType::Local)?;
	if branch.is_head() {
		return Err(Error::CurrentVersion);
	}
	branch.delete()?;
	Ok(())
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

/// A version's tree and manifest. `HEAD` is the latest checkpoint, and with no
/// version, the project as it stands on disk, neither kept nor staged.
fn side<'r>(repo: &'r Repository, version: Option<&str>) -> Result<(Tree<'r>, Manifest)> {
	let tree = match version {
		Some("HEAD") => head_commit(repo)?.map(|head| head.tree()).transpose()?,
		Some(id) => Some(repo.find_commit(Oid::from_str(id)?)?.tree()?),
		None => {
			let mut index = working(repo)?;
			let tree = index.write_tree()?;
			// The repository hands out one index, so what it staged in memory
			// would otherwise be seen by the next caller.
			index.read(true)?;
			Some(repo.find_tree(tree)?)
		}
	};
	with_manifest(repo, tree)
}

/// What the next checkpoint holds, without touching `.git/index`. While a
/// merge waits, a conflicted path holds this version's side.
fn staged_side(repo: &Repository) -> Result<(Tree<'_>, Manifest)> {
	let mut index = repo.index()?;
	// Before the first checkpoint nothing has been staged.
	if index.is_empty() {
		return with_manifest(repo, None);
	}
	let conflicts = index
		.conflicts()?
		.collect::<std::result::Result<Vec<_>, _>>()?;
	let merging = !conflicts.is_empty();
	for conflict in conflicts {
		let sides = [&conflict.our, &conflict.their, &conflict.ancestor];
		let Some(side) = sides.into_iter().flatten().next() else {
			continue;
		};
		let path = String::from_utf8_lossy(&side.path).into_owned();
		index.conflict_remove(Path::new(&path))?;
		if let Some(mut ours) = conflict.our {
			ours.flags &= !STAGE_MASK;
			index.add(&ours)?;
		}
	}
	let tree = index.write_tree()?;
	if merging {
		// The repository hands out one index, so the next caller would
		// otherwise see the conflicts settled.
		index.read(true)?;
	}
	with_manifest(repo, Some(repo.find_tree(tree)?))
}

/// A tree and the manifest it holds. With no tree, nothing at all, so
/// everything on the other side reads as added.
fn with_manifest<'r>(repo: &'r Repository, tree: Option<Tree<'r>>) -> Result<(Tree<'r>, Manifest)> {
	let Some(tree) = tree else {
		let empty = repo.find_tree(repo.treebuilder(None)?.write()?)?;
		let mut nothing = Manifest {
			nodes: Vec::new(),
			..Manifest::new("", Format::Novel, OffsetDateTime::UNIX_EPOCH)
		};
		// Otherwise random, and nothing must equal nothing.
		nothing.book.identifier = Uuid::nil();
		return Ok((empty, nothing));
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
/// it stands on disk and `HEAD` is the latest checkpoint.
pub fn changes(root: &Path, a: Option<&str>, b: Option<&str>) -> Result<Changes> {
	let repo = Repository::open(root)?;
	Ok(compare(&side(&repo, a)?, &side(&repo, b)?))
}

/// What the Changes list shows: from the last checkpoint to what the next
/// one holds, and from there to the project on disk. A row in both lists is
/// partly in the next checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Status {
	pub staged: Changes,
	pub unstaged: Changes,
}

pub fn status(root: &Path) -> Result<Status> {
	let repo = Repository::open(root)?;
	let staged = staged_side(&repo)?;
	Ok(Status {
		staged: compare(&side(&repo, Some("HEAD"))?, &staged),
		unstaged: compare(&staged, &side(&repo, None)?),
	})
}

fn compare((old_tree, old): &(Tree, Manifest), (new_tree, new): &(Tree, Manifest)) -> Changes {
	let placed = |manifest: &Manifest| {
		let mut placed = Vec::new();
		place(&manifest.nodes, None, "", &mut placed);
		placed
	};
	let (was, is) = (placed(old), placed(new));
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
					blob_at(old_tree, &then.path) != blob_at(new_tree, &now.path)
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
	Changes {
		nodes,
		book: details(&old.book) != details(&new.book),
		cover: old.book.cover != new.book.cover
			|| blob_at(old_tree, &old.book.cover) != blob_at(new_tree, &new.book.cover),
	}
}

/// One row of the Changes list, to move in or out of the next checkpoint. A
/// node row covers a document's text, name and place, or a folder's name,
/// place and order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum Row {
	Node { id: Uuid },
	Book,
	Cover,
}

/// Writes one row into what the next checkpoint holds, as the project on
/// disk has it when staging, or as the last checkpoint has it when not.
fn carry(root: &Path, row: Row, staging: bool) -> Result<()> {
	let repo = Repository::open(root)?;
	let (from_tree, from) = side(&repo, if staging { None } else { Some("HEAD") })?;
	let (was_tree, was) = staged_side(&repo)?;
	let mut is = Manifest {
		nodes: was.nodes.clone(),
		book: was.book.clone(),
		..read_manifest(root)?
	};
	let held = blobs_of(&was_tree, &was);
	let held_cover = blob_at(&was_tree, &was.book.cover);
	let (mut blobs, mut cover) = (held.clone(), held_cover);

	match row {
		Row::Book => {
			is.book = Book {
				cover: is.book.cover,
				..from.book
			}
		}
		Row::Cover => {
			cover = blob_at(&from_tree, &from.book.cover);
			is.book.cover = from.book.cover;
		}
		Row::Node { id } => match tree::path(&from.nodes, id) {
			// ponytail: a folder goes with all it holds, even a document moved
			// out of it first whose move is not in the next checkpoint.
			None => {
				tree::remove(&mut is.nodes, id);
			}
			Some(path) => {
				take(&from.nodes, &mut is.nodes, id);
				if let Some(blob) = blob_at(&from_tree, &path) {
					blobs.insert(id, blob);
				}
			}
		},
	}

	let files = |manifest: &Manifest, blobs: &HashMap<Uuid, Oid>, cover: Option<Oid>| {
		let mut files: HashMap<String, Oid> = manifest
			.documents()
			.into_iter()
			.filter_map(|document| Some((document.path, *blobs.get(&document.id)?)))
			.collect();
		if let Some(cover) = cover {
			files.insert(manifest.book.cover.clone(), cover);
		}
		files
	};
	let mut old = files(&was, &held, held_cover);
	if let Some(manifest) = blob_at(&was_tree, MANIFEST_FILE) {
		old.insert(MANIFEST_FILE.to_owned(), manifest);
	}
	let mut new = files(&is, &blobs, cover);
	new.insert(MANIFEST_FILE.to_owned(), repo.blob(&to_json(&is)?)?);

	let mut index = repo.index()?;
	for path in old.keys().filter(|path| !new.contains_key(*path)) {
		index.remove_path(Path::new(path))?;
	}
	for (path, id) in new.iter().filter(|(path, id)| old.get(*path) != Some(id)) {
		index.add(&entry(path, *id, repo.find_blob(*id)?.size()))?;
	}
	index.write()?;
	Ok(())
}

/// Each document's file in a tree, by id.
fn blobs_of(tree: &Tree, manifest: &Manifest) -> HashMap<Uuid, Oid> {
	manifest
		.documents()
		.iter()
		.filter_map(|document| Some((document.id, blob_at(tree, &document.path)?)))
		.collect()
}

/// An index entry with no file stats, so the next look at the folder reads
/// the file itself to tell whether it matches.
fn entry(path: &str, id: Oid, size: usize) -> IndexEntry {
	IndexEntry {
		ctime: IndexTime::new(0, 0),
		mtime: IndexTime::new(0, 0),
		dev: 0,
		ino: 0,
		mode: 0o100644,
		uid: 0,
		gid: 0,
		file_size: size as u32,
		id,
		flags: path.len().min(0xfff) as u16,
		flags_extended: 0,
		path: path.as_bytes().to_vec(),
	}
}

/// Puts the node with `id` where `from` has it, under its own name and
/// fields. Any folder above it that `into` lacks comes along, holding nothing
/// else. A folder keeps the children `into` gives it, in `from`'s order.
fn take(from: &[Node], into: &mut Vec<Node>, id: Uuid) {
	let Some(trail) = tree::trail(from, id) else {
		return;
	};
	let bare = |node: &Node| match node {
		Node::Folder { .. } => {
			let mut node = node.clone();
			if let Node::Folder { children, .. } = &mut node {
				children.clear();
			}
			node
		}
		Node::Document { .. } => node.clone(),
	};
	let (node, above) = trail.split_last().expect("a trail ends at its node");
	for folder in above {
		if tree::find(into, folder.id()).is_none() {
			insert(from, into, bare(folder));
		}
	}

	let mut placed = bare(node);
	if let (Some(Node::Folder { children: held, .. }), Node::Folder { children, .. }) =
		(tree::remove(into, id), &mut placed)
	{
		*children = held;
		follow(children, tree::children(from, id).unwrap_or_default());
	}
	insert(from, into, placed);
}

/// Inserts a node into the folder `from` has it in, after the node it follows
/// there if `into` has that one too, or first.
fn insert(from: &[Node], into: &mut Vec<Node>, node: Node) {
	let parent = tree::parent(from, node.id()).map(Node::id);
	let siblings = match parent {
		Some(parent) => tree::children(from, parent).unwrap_or_default(),
		None => from,
	};
	let at = siblings
		.iter()
		.position(|sibling| sibling.id() == node.id())
		.unwrap_or(0);
	let children = match parent {
		Some(parent) => match tree::find_mut(into, parent) {
			Some(Node::Folder { children, .. }) => children,
			_ => return,
		},
		None => into,
	};
	let index = siblings[..at]
		.iter()
		.rev()
		.find_map(|sibling| children.iter().position(|c| c.id() == sibling.id()))
		.map_or(0, |before| before + 1);
	children.insert(index, node);
}

/// Puts the children `order` also has into its order, in the places they
/// already take, so the others stay where they are.
fn follow(children: &mut [Node], order: &[Node]) {
	let rank = |node: &Node| order.iter().position(|other| other.id() == node.id());
	let slots: Vec<usize> = (0..children.len())
		.filter(|&at| rank(&children[at]).is_some())
		.collect();
	let mut moving: Vec<Node> = slots.iter().map(|&at| children[at].clone()).collect();
	moving.sort_by_key(rank);
	for (at, node) in slots.into_iter().zip(moving) {
		children[at] = node;
	}
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

/// The book's author, which every version is kept under.
fn author(root: &Path) -> Result<String> {
	// The path arrives from the frontend, so it is not trusted to be sensible.
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Ok(read_manifest(root)?.book.author)
}

#[tauri::command]
pub fn add_checkpoint(root: PathBuf, message: String) -> Result<String> {
	let author = author(&root)?;
	Ok(checkpoint(&root, &message, &author)?.to_string())
}

#[tauri::command]
pub fn list_checkpoints(
	root: PathBuf,
	version: Option<String>,
	document: Option<Uuid>,
) -> Result<Vec<Checkpoint>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	checkpoints(&root, version.as_deref(), document)
}

/// Main first, then the others by name.
#[tauri::command]
pub fn list_branches(root: PathBuf) -> Result<Vec<Branch>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	branches(&root)
}

#[tauri::command]
pub fn create_branch(root: PathBuf, name: String) -> Result<String> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	new_branch(&root, &name)
}

#[tauri::command]
pub fn rename_branch(root: PathBuf, slug: String, name: String) -> Result<String> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	renamed_branch(&root, &slug, &name)
}

#[tauri::command]
pub fn delete_branch(root: PathBuf, slug: String) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	dropped_branch(&root, &slug)
}

/// Asked before switching, so the writer can decide what happens to changes
/// not yet in a checkpoint.
#[tauri::command]
pub fn changes_pending(root: PathBuf) -> Result<bool> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	pending(&root)
}

#[tauri::command]
pub fn switch_branch(root: PathBuf, slug: String) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	switch(&root, &slug)
}

/// Merges `slug` into the version the writer is in. None when it finished
/// with its own checkpoint, otherwise the conflicts left to settle.
#[tauri::command]
pub fn merge_branch(root: PathBuf, slug: String) -> Result<Option<Merging>> {
	let author = author(&root)?;
	merge(&root, &slug, &author)
}

/// The merge under way, if there is one.
#[tauri::command]
pub fn merge_state(root: PathBuf) -> Result<Option<Merging>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	merging(&root)
}

#[tauri::command]
pub fn abort_merge(root: PathBuf) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	abort(&root)
}

/// A conflicted document as clean text and spots, or none once it is settled.
#[tauri::command]
pub fn merge_spots(root: PathBuf, id: Uuid) -> Result<Option<Vec<Piece>>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	spots(&root, id)
}

#[tauri::command]
pub fn settle_document(root: PathBuf, id: Uuid, text: String) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	settle(&root, id, &text)
}

/// Settles every conflict but the text: `theirs` are the ones the writer took
/// the other version's side of, and the rest keep this version's.
#[tauri::command]
pub fn settle_binder(root: PathBuf, theirs: Vec<merge::Conflict>) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	settle_rest(&root, &theirs)
}

/// One side's cover while a merge is under way, as raw bytes like
/// `read_cover`.
#[tauri::command]
pub fn merge_cover(root: PathBuf, theirs: bool) -> Result<tauri::ipc::Response> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Ok(tauri::ipc::Response::new(side_cover(&root, theirs)?))
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

/// Either side left out is the project as it stands now.
#[tauri::command]
pub fn version_changes(root: PathBuf, a: Option<String>, b: Option<String>) -> Result<Changes> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	changes(&root, a.as_deref(), b.as_deref())
}

#[tauri::command]
pub fn version_status(root: PathBuf) -> Result<Status> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	status(&root)
}

#[tauri::command]
pub fn stage_row(root: PathBuf, row: Row) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	carry(&root, row, true)
}

#[tauri::command]
pub fn unstage_row(root: PathBuf, row: Row) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	carry(&root, row, false)
}

#[tauri::command]
pub fn stage_all(root: PathBuf) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	stage(&Repository::open(&root)?)?;
	Ok(())
}

#[tauri::command]
pub fn unstage_all(root: PathBuf) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	let repo = Repository::open(&root)?;
	let mut index = repo.index()?;
	match head_commit(&repo)? {
		Some(head) => index.read_tree(&head.tree()?)?,
		None => index.clear()?,
	}
	index.write()?;
	Ok(())
}

#[tauri::command]
pub fn version_text(root: PathBuf, version: String, id: Uuid) -> Result<Option<String>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	text(&root, &version, id)
}

/// Putting back writes files only, so what it changed shows as changes not
/// yet in the next checkpoint.
fn open_for_putting_back(root: &Path) -> Result<Repository> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
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
pub fn put_back_document(root: PathBuf, version: String, id: Uuid) -> Result<()> {
	let repo = open_for_putting_back(&root)?;
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
pub fn put_back_book(root: PathBuf, version: String) -> Result<()> {
	let repo = open_for_putting_back(&root)?;
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
pub fn put_back_everything(root: PathBuf, version: String) -> Result<()> {
	let repo = open_for_putting_back(&root)?;
	let (files, then) = side(&repo, Some(&version))?;
	let was = read_manifest(&root)?;

	// Removing untracked files takes documents added since the last
	// checkpoint too. Stats and the Trash are excluded, so they stay.
	repo.checkout_tree(
		files.as_object(),
		Some(
			CheckoutBuilder::new()
				.force()
				.remove_untracked(true)
				.update_index(false),
		),
	)?;
	settle_folders(&root, &was, &then)
}

/// Git keeps files, not folders, so after a checkout the folders `was` had
/// and `is` does not are taken away, and empty ones `is` has are made.
fn settle_folders(root: &Path, was: &Manifest, is: &Manifest) -> Result<()> {
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
	let was = folders(was);
	let is = folders(is);
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

/// Whether the project differs from its last checkpoint, on disk or in what
/// the next checkpoint holds. Stats and the Trash never count.
fn pending(root: &Path) -> Result<bool> {
	let repo = Repository::open(root)?;
	let mut options = StatusOptions::new();
	options.include_untracked(true);
	Ok(!repo.statuses(Some(&mut options))?.is_empty())
}

/// Moves the writer into another version, refusing while there are changes
/// a checkout would overwrite.
fn switch(root: &Path, slug: &str) -> Result<()> {
	if pending(root)? {
		return Err(Error::ChangesPending);
	}
	let repo = Repository::open(root)?;
	let target = repo
		.find_branch(slug, BranchType::Local)?
		.get()
		.peel_to_commit()?;
	let (_, was) = side(&repo, Some("HEAD"))?;
	let (files, is) = side(&repo, Some(&target.id().to_string()))?;

	// Nothing differs from `HEAD`, so a forced checkout loses no writing. It
	// removes what the target does not hold, updates the index to match, and
	// leaves ignored files alone.
	repo.checkout_tree(files.as_object(), Some(CheckoutBuilder::new().force()))?;
	settle_folders(root, &was, &is)?;
	repo.set_head(&format!("refs/heads/{slug}"))?;
	Ok(())
}

/// A merge waiting for its conflicts to be settled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Merging {
	/// The version being merged in, or none once no version ends at the
	/// checkpoint being merged.
	pub name: Option<String>,
	/// What the checkpoint that finishes the merge says, until the writer
	/// changes it.
	pub message: String,
	pub conflicts: Vec<merge::Conflict>,
}

/// The checkpoint being merged in, while a merge is under way.
fn merge_head(repo: &Repository) -> Result<Option<Commit<'_>>> {
	match repo.find_reference("MERGE_HEAD") {
		Ok(merged) => Ok(Some(merged.peel_to_commit()?)),
		Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
		Err(e) => Err(e.into()),
	}
}

/// The checkpoint both versions grew from, or none for histories that never
/// met.
fn merge_base<'r>(
	repo: &'r Repository,
	ours: &Commit,
	theirs: &Commit,
) -> Result<Option<Commit<'r>>> {
	match repo.merge_base(ours.id(), theirs.id()) {
		Ok(id) => Ok(Some(repo.find_commit(id)?)),
		Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
		Err(e) => Err(e.into()),
	}
}

fn merge_side<'r>(
	repo: &'r Repository,
	commit: Option<&Commit<'r>>,
) -> Result<(Tree<'r>, merge::Side)> {
	let (tree, manifest) = with_manifest(repo, commit.map(Commit::tree).transpose()?)?;
	let blobs = blobs_of(&tree, &manifest);
	let cover = blob_at(&tree, &manifest.book.cover);
	Ok((
		tree,
		merge::Side {
			manifest,
			blobs,
			cover,
		},
	))
}

/// Where conflict entries keep their stage in an index entry's flags.
const STAGE_SHIFT: u16 = 12;
const STAGE_MASK: u16 = 0x3000;

fn staged_entry(repo: &Repository, path: &str, id: Oid, stage: u16) -> Result<IndexEntry> {
	let mut entry = entry(path, id, repo.find_blob(id)?.size());
	entry.flags |= stage << STAGE_SHIFT;
	Ok(entry)
}

/// Merges another version into the one the writer is in. A clean merge adds
/// its own checkpoint, even when git could fast-forward. Otherwise the merge
/// stays under way: a document both changed in the same place keeps this
/// version's text on disk with its three sides in `.git/index`, and any other
/// conflict leaves `aurora.json` conflicted the same way.
fn merge(root: &Path, slug: &str, author: &str) -> Result<Option<Merging>> {
	if pending(root)? {
		return Err(Error::ChangesPending);
	}
	let repo = Repository::open(root)?;
	let Some(head) = head_commit(&repo)? else {
		return Err(Error::NothingKept);
	};
	let theirs = repo
		.find_branch(slug, BranchType::Local)?
		.get()
		.peel_to_commit()?;
	let base = merge_base(&repo, &head, &theirs)?;
	if base.as_ref().map(Commit::id) == Some(theirs.id()) {
		return Err(Error::AlreadyMerged);
	}
	let (base_tree, base) = merge_side(&repo, base.as_ref())?;
	let (our_tree, ours) = merge_side(&repo, Some(&head))?;
	let (their_tree, theirs_side) = merge_side(&repo, Some(&theirs))?;
	let merged = merge::manifests(&base, &ours, &theirs_side, &[]);

	// Git merges by path only the files no manifest names. Documents merge
	// by id, so a moved or renamed one keeps its text.
	let owned: HashSet<String> = [&base, &ours, &theirs_side]
		.iter()
		.flat_map(|side| side.manifest.documents())
		.map(|document| document.path)
		.chain(
			std::iter::once(MANIFEST_FILE)
				.chain(COVER_NAMES)
				.map(str::to_owned),
		)
		.collect();
	let others = repo.merge_trees(&base_tree, &our_tree, &their_tree, None)?;
	let mut index = repo.index()?;
	index.clear()?;
	for other in others.iter() {
		if !owned.contains(&*String::from_utf8_lossy(&other.path)) {
			index.add(&other)?;
		}
	}

	for document in merged.manifest.documents() {
		let path = &document.path;
		let id = document.id;
		let (b, o, t) = (
			base.blobs.get(&id),
			ours.blobs.get(&id),
			theirs_side.blobs.get(&id),
		);
		let (o, t) = match (o, t) {
			(Some(&o), Some(&t)) => (o, t),
			(Some(&only), None) | (None, Some(&only)) => {
				index.add(&staged_entry(&repo, path, only, 0)?)?;
				continue;
			}
			(None, None) => continue,
		};
		if let Some(Some(text)) = merge::three_way(&b.copied(), &Some(o), &Some(t)) {
			index.add(&staged_entry(&repo, path, text, 0)?)?;
			continue;
		}
		let ancestor = b.map(|&b| staged_entry(&repo, path, b, 1)).transpose()?;
		let (here, there) = (
			staged_entry(&repo, path, o, 2)?,
			staged_entry(&repo, path, t, 3)?,
		);
		let text = match &ancestor {
			Some(ancestor) => Some(repo.merge_file_from_index(ancestor, &here, &there, None)?)
				.filter(|text| text.is_automergeable()),
			None => None,
		};
		match text {
			Some(text) => {
				index.add(&staged_entry(&repo, path, repo.blob(text.content())?, 0)?)?;
			}
			None => {
				for side in ancestor.iter().chain([&here, &there]) {
					index.add(side)?;
				}
			}
		}
	}

	if let Some(cover) = merged.cover {
		index.add(&staged_entry(&repo, &merged.manifest.book.cover, cover, 0)?)?;
	}
	// With a conflict, ours here is the merge with this version's side of
	// each, so it is what the writer sees until they choose.
	let manifest = repo.blob(&to_json(&merged.manifest)?)?;
	match merged.conflicts.is_empty() {
		true => index.add(&staged_entry(&repo, MANIFEST_FILE, manifest, 0)?)?,
		false => {
			if let Some(base) = blob_at(&base_tree, MANIFEST_FILE) {
				index.add(&staged_entry(&repo, MANIFEST_FILE, base, 1)?)?;
			}
			index.add(&staged_entry(&repo, MANIFEST_FILE, manifest, 2)?)?;
			if let Some(theirs) = blob_at(&their_tree, MANIFEST_FILE) {
				index.add(&staged_entry(&repo, MANIFEST_FILE, theirs, 3)?)?;
			}
		}
	}

	// Nothing was pending, so removing untracked files only takes what the
	// merge moved away. Stats and the Trash are excluded and stay.
	repo.checkout_index(
		Some(&mut index),
		Some(
			CheckoutBuilder::new()
				.force()
				.allow_conflicts(true)
				.use_ours(true)
				.remove_untracked(true),
		),
	)?;
	index.write()?;
	settle_folders(root, &ours.manifest, &merged.manifest)?;

	let message = format!("Merged {}", version_name(&repo, slug)?);
	if index.has_conflicts() {
		fs::write(repo.path().join("MERGE_HEAD"), format!("{}\n", theirs.id()))?;
		fs::write(repo.path().join("MERGE_MSG"), format!("{message}\n"))?;
		return merging(root);
	}

	let tree = repo.find_tree(index.write_tree()?)?;
	let signature = signature(author)?;
	repo.commit(
		Some("HEAD"),
		&signature,
		&signature,
		&message,
		&tree,
		&[&head, &theirs],
	)?;
	Ok(None)
}

fn merging(root: &Path) -> Result<Option<Merging>> {
	let repo = Repository::open(root)?;
	let Some(merged) = merge_head(&repo)? else {
		return Ok(None);
	};

	let mut name = None;
	for branch in repo.branches(Some(BranchType::Local))? {
		let (branch, _) = branch?;
		if branch.get().target() == Some(merged.id())
			&& let Some(slug) = branch.name()?
		{
			name = Some(version_name(&repo, slug)?);
		}
	}

	// The merged manifest on disk places each document first, then this
	// version and the other.
	let mut ids = HashMap::new();
	let manifests = [
		read_manifest(root)?,
		side(&repo, Some("HEAD"))?.1,
		side(&repo, Some(&merged.id().to_string()))?.1,
	];
	for document in manifests.iter().flat_map(Manifest::documents) {
		ids.entry(document.path).or_insert(document.id);
	}
	let mut conflicts = Vec::new();
	let mut manifest = false;
	for conflict in repo.index()?.conflicts()? {
		let conflict = conflict?;
		let Some(side) = conflict.our.or(conflict.their).or(conflict.ancestor) else {
			continue;
		};
		let path = String::from_utf8_lossy(&side.path).into_owned();
		if path == MANIFEST_FILE {
			manifest = true;
			continue;
		}
		conflicts.push(merge::Conflict::Text {
			id: ids.get(&path).copied(),
			path,
		});
	}
	// The rest are worked out again from the three versions, which do not
	// change while the merge is under way.
	if manifest && let Some(head) = head_commit(&repo)? {
		let base = merge_base(&repo, &head, &merged)?;
		let (_, base) = merge_side(&repo, base.as_ref())?;
		let (_, ours) = merge_side(&repo, Some(&head))?;
		let (_, theirs) = merge_side(&repo, Some(&merged))?;
		conflicts.extend(merge::manifests(&base, &ours, &theirs, &[]).conflicts);
	}

	Ok(Some(Merging {
		name,
		message: fs::read_to_string(repo.path().join("MERGE_MSG"))?
			.trim_end()
			.to_owned(),
		conflicts,
	}))
}

/// One stretch of a conflicted document: text both versions agree on, or a
/// spot where each has its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum Piece {
	Clean { text: String },
	Spot { ours: String, theirs: String },
}

/// Long enough that no line a writer types is taken for a marker.
const MARKER_SIZE: usize = 64;

/// A conflicted document in order, or none when it has no conflict. Git's
/// file merge marks each spot, and the label on its markers is made for this
/// call, so markers a writer typed stay text.
fn spots(root: &Path, id: Uuid) -> Result<Option<Vec<Piece>>> {
	let repo = Repository::open(root)?;
	let path = tree::path(&read_manifest(root)?.nodes, id).ok_or(Error::UnknownDocument)?;
	let mut found = None;
	for conflict in repo.index()?.conflicts()? {
		let conflict = conflict?;
		if conflict
			.our
			.as_ref()
			.is_some_and(|our| our.path == path.as_bytes())
		{
			found = Some(conflict);
		}
	}
	let Some(conflict) = found else {
		return Ok(None);
	};

	let content = |side: &Option<IndexEntry>| -> Result<Vec<u8>> {
		Ok(match side {
			Some(side) => repo.find_blob(side.id)?.content().to_vec(),
			None => Vec::new(),
		})
	};
	let sides = [
		content(&conflict.ancestor)?,
		content(&conflict.our)?,
		content(&conflict.their)?,
	];
	let [base, ours, theirs] = sides.each_ref().map(|side| {
		let mut input = MergeFileInput::new();
		input.content(side);
		input
	});
	let label = Uuid::new_v4().to_string();
	let mut options = MergeFileOptions::new();
	options
		.our_label(&*label)
		.their_label(&*label)
		.marker_size(MARKER_SIZE as u16);
	let merged = git2::merge_file(&base, &ours, &theirs, Some(&mut options))?;
	Ok(Some(pieces(
		&String::from_utf8_lossy(merged.content()),
		&label,
	)))
}

fn pieces(merged: &str, label: &str) -> Vec<Piece> {
	let open = format!("{} {label}", "<".repeat(MARKER_SIZE));
	// ponytail: the middle marker carries no label, so a line of exactly 64
	// "=" inside a spot splits it early; diff the sides here if that bites.
	let middle = "=".repeat(MARKER_SIZE);
	let close = format!("{} {label}", ">".repeat(MARKER_SIZE));

	let mut pieces = Vec::new();
	let (mut clean, mut ours, mut theirs) = (String::new(), String::new(), String::new());
	// Outside a spot, in this version's side, or in the other's.
	let mut side = None;
	for line in merged.split_inclusive('\n') {
		let bare = line.trim_end_matches(['\n', '\r']);
		match side {
			None if bare == open => {
				if !clean.is_empty() {
					pieces.push(Piece::Clean {
						text: std::mem::take(&mut clean),
					});
				}
				side = Some(false);
			}
			Some(false) if bare == middle => side = Some(true),
			Some(true) if bare == close => {
				pieces.push(Piece::Spot {
					ours: std::mem::take(&mut ours),
					theirs: std::mem::take(&mut theirs),
				});
				side = None;
			}
			None => clean.push_str(line),
			Some(false) => ours.push_str(line),
			Some(true) => theirs.push_str(line),
		}
	}
	if !clean.is_empty() {
		pieces.push(Piece::Clean { text: clean });
	}
	pieces
}

/// Writes a conflicted document as the writer settled it and puts it in the
/// next checkpoint, which takes it off the conflicts.
fn settle(root: &Path, id: Uuid, text: &str) -> Result<()> {
	write_document(root.to_path_buf(), id, text.to_owned())?;
	let repo = Repository::open(root)?;
	let path = tree::path(&read_manifest(root)?.nodes, id).ok_or(Error::UnknownDocument)?;
	let mut index = repo.index()?;
	index.conflict_remove(Path::new(&path))?;
	index.add(&staged_entry(&repo, &path, repo.blob(text.as_bytes())?, 0)?)?;
	index.write()?;
	Ok(())
}

/// Merges the binder and the book again with the writer's sides, puts them in
/// the next checkpoint, and moves the files to match. A document still
/// conflicted keeps its three sides wherever it goes.
fn settle_rest(root: &Path, theirs: &[merge::Conflict]) -> Result<()> {
	let repo = Repository::open(root)?;
	let (Some(head), Some(merged)) = (head_commit(&repo)?, merge_head(&repo)?) else {
		return Ok(());
	};
	let base = merge_base(&repo, &head, &merged)?;
	let (_, base) = merge_side(&repo, base.as_ref())?;
	let (_, ours) = merge_side(&repo, Some(&head))?;
	let (_, other) = merge_side(&repo, Some(&merged))?;
	let settled = merge::manifests(&base, &ours, &other, theirs);
	// ponytail: the binder is merged afresh, so a rename or move made while
	// the merge was under way is lost; carry `was` over if writers do that.
	let was = read_manifest(root)?;

	let paths = |manifest: &Manifest| -> HashMap<Uuid, String> {
		manifest
			.documents()
			.into_iter()
			.map(|document| (document.id, document.path))
			.collect()
	};
	let (from, to) = (paths(&was), paths(&settled.manifest));
	let mut index = repo.index()?;
	// Every file that moves is read before any is written, so one taking
	// another's old name loses nothing. The text on disk goes, so writing
	// done during the merge comes along.
	let mut moving = Vec::new();
	for (id, old) in &from {
		let new = to.get(id);
		if new == Some(old) {
			continue;
		}
		let file = document_path(&was, root, old)?;
		let entries: Vec<IndexEntry> = index
			.iter()
			.filter(|entry| entry.path == old.as_bytes())
			.collect();
		for entry in &entries {
			index.remove(Path::new(old), i32::from(stage_of(entry)))?;
		}
		if let Some(new) = new {
			moving.push((new, fs::read(&file)?, entries));
		}
		remove_if_there(&file)?;
	}
	for (id, new) in &to {
		if from.contains_key(id) {
			continue;
		}
		let Some(&blob) = other.blobs.get(id).or(ours.blobs.get(id)) else {
			continue;
		};
		let entries = vec![staged_entry(&repo, new, blob, 0)?];
		moving.push((new, repo.find_blob(blob)?.content().to_vec(), entries));
	}
	for (path, text, entries) in moving {
		let file = document_path(&settled.manifest, root, path)?;
		if let Some(folder) = file.parent() {
			fs::create_dir_all(folder)?;
		}
		write_atomic(&file, &text)?;
		for entry in entries {
			index.add(&staged_entry(&repo, path, entry.id, stage_of(&entry))?)?;
		}
	}

	let cover = settled
		.conflicts
		.iter()
		.any(|conflict| matches!(conflict, merge::Conflict::Cover { .. }));
	for name in COVER_NAMES.iter().filter(|_| cover) {
		match settled
			.cover
			.filter(|_| settled.manifest.book.cover == *name)
		{
			Some(blob) => {
				write_atomic(&root.join(name), repo.find_blob(blob)?.content())?;
				index.add(&staged_entry(&repo, name, blob, 0)?)?;
			}
			None => {
				remove_if_there(&root.join(name))?;
				index.remove_path(Path::new(name))?;
			}
		}
	}

	let manifest = to_json(&settled.manifest)?;
	write_atomic(&root.join(MANIFEST_FILE), &manifest)?;
	index.remove_path(Path::new(MANIFEST_FILE))?;
	let manifest = repo.blob(&manifest)?;
	index.add(&staged_entry(&repo, MANIFEST_FILE, manifest, 0)?)?;
	index.write()?;
	settle_folders(root, &was, &settled.manifest)
}

fn stage_of(entry: &IndexEntry) -> u16 {
	(entry.flags & STAGE_MASK) >> STAGE_SHIFT
}

/// The cover the version the writer is in has, or the one being merged in.
fn side_cover(root: &Path, theirs: bool) -> Result<Vec<u8>> {
	let repo = Repository::open(root)?;
	let commit = match theirs {
		true => merge_head(&repo)?,
		false => head_commit(&repo)?,
	};
	let (_, side) = merge_side(&repo, Some(&commit.ok_or(Error::CoverMissing)?))?;
	let cover = side.cover.ok_or(Error::CoverMissing)?;
	Ok(repo.find_blob(cover)?.content().to_vec())
}

/// Puts the project back as it was before the merge began. Without a merge
/// under way, nothing happens, so no writing is lost.
fn abort(root: &Path) -> Result<()> {
	let repo = Repository::open(root)?;
	if merge_head(&repo)?.is_none() {
		return Ok(());
	}
	let head = head_commit(&repo)?.ok_or(Error::NothingKept)?;
	let was = read_manifest(root)?;
	repo.reset(head.as_object(), ResetType::Hard, None)?;
	repo.cleanup_state()?;
	settle_folders(root, &was, &side(&repo, Some("HEAD"))?.1)
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
	use crate::project::{Format, create, set_cover, write_book, write_manifest};
	use crate::tree::Node;
	use git2::RepositoryState;

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

	const FIRST: &str = "First";

	/// Opens the project and adds a first checkpoint, `FIRST`, so it has one
	/// to go back to.
	fn begun(root: &Path) {
		ensure_repository(root).unwrap();
		checkpoint(root, FIRST, "").unwrap();
	}

	fn unborn_main(repo: &Repository) -> bool {
		let head = repo.find_reference("HEAD").unwrap();
		head.symbolic_target().unwrap() == Some("refs/heads/main")
			&& repo.head().err().map(|e| e.code()) == Some(ErrorCode::UnbornBranch)
	}

	#[test]
	fn a_fresh_folder_begins_on_main_with_nothing_kept() {
		let dir = project();
		assert_eq!(ensure_repository(dir.path()).unwrap(), None);

		let repo = Repository::open(dir.path()).unwrap();
		assert!(unborn_main(&repo));
		assert!(checkpoints(dir.path(), None, None).unwrap().is_empty());
	}

	#[test]
	fn opening_again_keeps_no_new_version() {
		let dir = project();
		ensure_repository(dir.path()).unwrap();
		ensure_repository(dir.path()).unwrap();

		let repo = Repository::open(dir.path()).unwrap();
		assert!(unborn_main(&repo));
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

		ensure_repository(dir.path()).unwrap();

		assert_eq!(repo.head().unwrap().target(), Some(theirs));
		assert_eq!(versions(&repo), 1);
	}

	#[test]
	fn excluded_files_are_never_kept() {
		let dir = project();
		fs::write(dir.path().join(HISTORY_FILE), "{}").unwrap();
		fs::create_dir(dir.path().join(TRASH_DIR)).unwrap();
		fs::write(dir.path().join(TRASH_DIR).join("gone.md"), "gone").unwrap();

		begun(dir.path());

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

		let found = ensure_repository(&root).unwrap().unwrap();
		// Compared by name: the platforms disagree about how to spell the path.
		assert_eq!(found.file_name(), outer.path().file_name());
		assert!(root.join(".git").is_dir());

		Repository::open(&root)
			.unwrap()
			.config()
			.unwrap()
			.set_bool(NESTED_NOTICE_SHOWN, true)
			.unwrap();
		assert_eq!(ensure_repository(&root).unwrap(), None);
	}

	#[test]
	fn a_document_is_followed_through_renames_and_moves() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(
			parent.path(),
			"Ithaca",
			Format::Novel,
			OffsetDateTime::UNIX_EPOCH,
		)
		.unwrap();
		begun(&root);
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
		let mut expected = vec![repo_head(&root).to_string()];

		write_document(root.clone(), document, "One".to_owned()).unwrap();
		expected.push(kept(&root));

		let other = create_document(root.clone(), elsewhere, "Other".to_owned())
			.unwrap()
			.id;
		let other_added = kept(&root);

		rename_document(root.clone(), document, "Renamed".to_owned()).unwrap();
		kept(&root);

		write_document(root.clone(), document, "One two".to_owned()).unwrap();
		expected.push(kept(&root));

		move_node(root.clone(), document, elsewhere, 0).unwrap();
		write_document(root.clone(), document, "One two three".to_owned()).unwrap();
		expected.push(kept(&root));

		let ids = |document| -> Vec<String> {
			checkpoints(&root, None, Some(document))
				.unwrap()
				.into_iter()
				.map(|row| row.id)
				.collect()
		};
		expected.reverse();
		assert_eq!(ids(document), expected);
		let words: Vec<_> = checkpoints(&root, None, Some(document))
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
	fn a_checkpoint_needs_a_message() {
		let dir = project();
		ensure_repository(dir.path()).unwrap();

		let refused = checkpoint(dir.path(), " \n\t", "");
		assert!(matches!(refused, Err(Error::EmptyMessage)));
		assert!(unborn_main(&Repository::open(dir.path()).unwrap()));
	}

	#[test]
	fn the_first_checkpoint_on_main_has_no_parent() {
		let dir = project();
		ensure_repository(dir.path()).unwrap();
		let message = "Opening pages\n\nStill rough.";
		let id = checkpoint(dir.path(), message, "Herman Melville").unwrap();

		let repo = Repository::open(dir.path()).unwrap();
		let commit = repo.find_commit(id).unwrap();
		assert_eq!(commit.parent_count(), 0);
		assert_eq!(commit.message().unwrap(), message);
		assert_eq!(repo.head().unwrap().shorthand().unwrap(), "main");
		assert!(
			commit
				.tree()
				.unwrap()
				.get_path(Path::new("Manuscript/one.md"))
				.is_ok()
		);

		let rows = checkpoints(dir.path(), None, None).unwrap();
		assert_eq!(rows.len(), 1);
		assert_eq!(rows[0].id, id.to_string());
		assert_eq!(rows[0].message, message);
		assert_eq!(rows[0].author, "Herman Melville");
		assert_eq!(rows[0].words, None);
	}

	#[test]
	fn a_document_lists_only_the_checkpoints_that_changed_its_text() {
		let (_parent, root, document, elsewhere, _) = novel();
		let first = repo_head(&root);

		write_document(root.clone(), document, "One".to_owned()).unwrap();
		let one = checkpoint(&root, "One word", "").unwrap();
		create_document(root.clone(), elsewhere, "Other".to_owned()).unwrap();
		checkpoint(&root, "Another document", "").unwrap();
		write_document(root.clone(), document, "One two".to_owned()).unwrap();
		let two = checkpoint(&root, "Two words", "").unwrap();

		let rows = checkpoints(&root, None, Some(document)).unwrap();
		let ids: Vec<Oid> = rows.iter().map(|r| Oid::from_str(&r.id).unwrap()).collect();
		assert_eq!(ids, [two, one, first]);
		assert_eq!(rows[0].words, Some(2));
		assert_eq!(rows[1].words, Some(1));
		assert_eq!(
			checkpoints(&root, Some("main"), Some(document)).unwrap(),
			rows
		);
		assert!(
			checkpoints(&root, Some("elsewhere"), None)
				.unwrap()
				.is_empty()
		);
	}

	#[test]
	fn a_foreign_commit_lists_with_its_own_message() {
		let dir = project();
		let repo = Repository::init(dir.path()).unwrap();
		let mut index = repo.index().unwrap();
		index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
		let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
		let signature = Signature::now("Writer", "writer@example.com").unwrap();
		repo.commit(
			Some("HEAD"),
			&signature,
			&signature,
			"Chapter one done\n\nLong notes.\n",
			&tree,
			&[],
		)
		.unwrap();

		let rows = checkpoints(dir.path(), None, None).unwrap();
		assert_eq!(rows[0].message, "Chapter one done\n\nLong notes.");
		assert_eq!(rows[0].author, "Writer");
	}

	#[test]
	fn a_slug_is_lowercase_letters_and_digits_joined_by_single_hyphens() {
		assert_eq!(slug("Second Draft").unwrap(), "second-draft");
		assert_eq!(slug("  Tom's -- ending!! ").unwrap(), "tom-s-ending");
		assert_eq!(slug("Café 2").unwrap(), "café-2");
		assert!(matches!(slug(" -!- "), Err(Error::VersionName)));
		assert!(matches!(slug(""), Err(Error::VersionName)));
	}

	fn head_target(root: &Path) -> String {
		let repo = Repository::open(root).unwrap();
		let head = repo.find_reference("HEAD").unwrap();
		head.symbolic_target().unwrap().unwrap().to_owned()
	}

	fn typed_name(root: &Path, slug: &str) -> Option<String> {
		let config = Repository::open(root).unwrap().config().unwrap();
		config.get_string(&name_key(slug)).ok()
	}

	#[test]
	fn a_new_version_starts_here_and_brings_the_changes_along() {
		let (_parent, root, document, _, _) = novel();
		let first = repo_head(&root);
		write_document(root.clone(), document, "Not yet kept".to_owned()).unwrap();
		let index = fs::read(root.join(".git/index")).unwrap();

		assert_eq!(new_branch(&root, " Second Draft ").unwrap(), "second-draft");
		assert_eq!(head_target(&root), "refs/heads/second-draft");
		assert_eq!(repo_head(&root), first);
		assert_eq!(body_now(&root, document), "Not yet kept");
		assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);

		let rows = branches(&root).unwrap();
		let expected = [
			Branch {
				slug: "main".to_owned(),
				name: "Main".to_owned(),
				current: false,
				unmerged: 0,
				fixed: true,
			},
			Branch {
				slug: "second-draft".to_owned(),
				name: "Second Draft".to_owned(),
				current: true,
				unmerged: 0,
				fixed: false,
			},
		];
		assert_eq!(rows, expected);

		let taken = new_branch(&root, "second draft!");
		assert!(matches!(taken, Err(Error::VersionExists)));
		assert!(matches!(
			new_branch(&root, "Main"),
			Err(Error::VersionExists)
		));
	}

	#[test]
	fn a_new_version_waits_for_the_first_checkpoint() {
		let dir = project();
		ensure_repository(dir.path()).unwrap();

		let refused = new_branch(dir.path(), "Second Draft");
		assert!(matches!(refused, Err(Error::NothingKept)));
		let rows = branches(dir.path()).unwrap();
		assert_eq!(rows.len(), 1);
		assert_eq!(rows[0].name, "Main");
		assert!(rows[0].current);
	}

	#[test]
	fn a_renamed_version_takes_its_typed_name_along() {
		let (_parent, root, _, _, _) = novel();
		new_branch(&root, "Draft Two").unwrap();

		assert_eq!(
			renamed_branch(&root, "draft-two", "Draft Three").unwrap(),
			"draft-three"
		);
		assert_eq!(head_target(&root), "refs/heads/draft-three");
		assert_eq!(typed_name(&root, "draft-two"), None);
		assert_eq!(
			typed_name(&root, "draft-three").as_deref(),
			Some("Draft Three")
		);

		assert_eq!(
			renamed_branch(&root, "draft-three", "DRAFT three").unwrap(),
			"draft-three"
		);
		assert_eq!(
			typed_name(&root, "draft-three").as_deref(),
			Some("DRAFT three")
		);

		new_branch(&root, "Other").unwrap();
		let taken = renamed_branch(&root, "other", "Draft three");
		assert!(matches!(taken, Err(Error::VersionExists)));
		let main = renamed_branch(&root, "main", "Trunk");
		assert!(matches!(main, Err(Error::MainFixed)));
	}

	#[test]
	fn neither_main_nor_the_version_the_writer_is_in_can_be_deleted() {
		let (_parent, root, _, _, _) = novel();
		new_branch(&root, "Aside").unwrap();

		let current = dropped_branch(&root, "aside");
		assert!(matches!(current, Err(Error::CurrentVersion)));
		let main = dropped_branch(&root, "main");
		assert!(matches!(main, Err(Error::MainFixed)));

		Repository::open(&root)
			.unwrap()
			.set_head("refs/heads/main")
			.unwrap();
		dropped_branch(&root, "aside").unwrap();
		assert_eq!(typed_name(&root, "aside"), None);
		let slugs: Vec<String> = branches(&root)
			.unwrap()
			.into_iter()
			.map(|row| row.slug)
			.collect();
		assert_eq!(slugs, ["main"]);
	}

	/// The project as a repository made elsewhere leaves it, on `master`.
	fn adopted(root: &Path) {
		let repo = Repository::open(root).unwrap();
		repo.find_branch("main", BranchType::Local)
			.unwrap()
			.rename("master", false)
			.unwrap();
		repo.set_head("refs/heads/master").unwrap();
	}

	fn listed(root: &Path) -> Vec<(String, String, bool, bool)> {
		branches(root)
			.unwrap()
			.into_iter()
			.map(|row| (row.slug, row.name, row.current, row.fixed))
			.collect()
	}

	#[test]
	fn an_adopted_master_is_main_from_any_other_version() {
		let (_parent, root, _, _, _) = novel();
		adopted(&root);
		new_branch(&root, "Feature").unwrap();
		let repo = Repository::open(&root).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		repo.branch("zebra", &head, false).unwrap();

		assert_eq!(
			listed(&root),
			[
				("master".to_owned(), "Main".to_owned(), false, true),
				("feature".to_owned(), "Feature".to_owned(), true, false),
				("zebra".to_owned(), "zebra".to_owned(), false, false),
			]
		);
		let renamed = renamed_branch(&root, "master", "Trunk");
		assert!(matches!(renamed, Err(Error::MainFixed)));
		let dropped = dropped_branch(&root, "master");
		assert!(matches!(dropped, Err(Error::MainFixed)));
	}

	#[test]
	fn master_beside_main_is_a_version_like_any_other() {
		let (_parent, root, _, _, _) = novel();
		let repo = Repository::open(&root).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		repo.branch("master", &head, false).unwrap();

		assert_eq!(
			listed(&root),
			[
				("main".to_owned(), "Main".to_owned(), true, true),
				("master".to_owned(), "master".to_owned(), false, false),
			]
		);
		dropped_branch(&root, "master").unwrap();
	}

	#[test]
	fn a_detached_checkout_can_be_left_but_not_deleted() {
		let (_parent, root, _, _, _) = novel();
		let repo = Repository::open(&root).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		repo.set_head_detached(head.id()).unwrap();
		let kept = checkpoint(&root, "Only here", "").unwrap();

		let found = branches(&root).unwrap();
		let main = found.iter().find(|row| row.slug == "main").unwrap();
		assert!(!main.current);
		let loose = found.iter().find(|row| row.current).unwrap();
		assert!(kept.to_string().starts_with(&loose.slug));
		assert_eq!(loose.name, loose.slug);
		assert!(loose.fixed);
		assert_eq!(loose.unmerged, 1);

		let refused = dropped_branch(&root, &loose.slug);
		assert!(matches!(refused, Err(Error::CurrentVersion)));

		switch(&root, "main").unwrap();
		assert_eq!(head_target(&root), "refs/heads/main");
		assert_eq!(
			listed(&root),
			[("main".to_owned(), "Main".to_owned(), true, true)]
		);
	}

	#[test]
	fn unmerged_counts_the_checkpoints_only_one_version_reaches() {
		let (_parent, root, _, _, _) = novel();
		new_branch(&root, "Draft").unwrap();
		checkpoint(&root, "One", "").unwrap();
		let draft = checkpoint(&root, "Two", "").unwrap();

		let unmerged = |root: &Path| -> Vec<(String, u32)> {
			branches(root)
				.unwrap()
				.into_iter()
				.map(|row| (row.slug, row.unmerged))
				.collect()
		};
		assert_eq!(
			unmerged(&root),
			[("main".to_owned(), 0), ("draft".to_owned(), 2)]
		);

		let repo = Repository::open(&root).unwrap();
		repo.set_head("refs/heads/main").unwrap();
		let main = repo.head().unwrap().peel_to_commit().unwrap();
		let draft = repo.find_commit(draft).unwrap();
		let signature = Signature::now("Writer", "writer@example.com").unwrap();
		repo.commit(
			Some("HEAD"),
			&signature,
			&signature,
			"Merged Draft",
			&draft.tree().unwrap(),
			&[&main, &draft],
		)
		.unwrap();

		assert_eq!(
			unmerged(&root),
			[("main".to_owned(), 1), ("draft".to_owned(), 0)]
		);
	}

	#[test]
	fn switching_waits_while_changes_are_not_in_a_checkpoint() {
		let (_parent, root, document, _, _) = novel();
		new_branch(&root, "Draft").unwrap();
		fs::write(root.join(HISTORY_FILE), "[]").unwrap();
		assert!(!pending(&root).unwrap());

		write_document(root.clone(), document, "Not yet kept".to_owned()).unwrap();
		assert!(pending(&root).unwrap());
		let refused = switch(&root, "main");
		assert!(matches!(refused, Err(Error::ChangesPending)));
		assert_eq!(head_target(&root), "refs/heads/draft");
		assert_eq!(body_now(&root, document), "Not yet kept");
	}

	#[test]
	fn switching_brings_the_whole_project_to_the_other_version() {
		let (_parent, root, edited, here, there) = novel();
		let moved = create_document(root.clone(), here, "Moved".to_owned())
			.unwrap()
			.id;
		let deleted = create_document(root.clone(), here, "Deleted".to_owned())
			.unwrap()
			.id;
		write_document(root.clone(), edited, "Main's text".to_owned()).unwrap();
		let main = checkpoint(&root, "On Main", "").unwrap().to_string();

		new_branch(&root, "Draft").unwrap();
		write_document(root.clone(), edited, "Draft's text".to_owned()).unwrap();
		create_document(root.clone(), there, "Added".to_owned()).unwrap();
		move_node(root.clone(), moved, there, 0).unwrap();
		delete_document(root.clone(), deleted).unwrap();
		let empty = create_folder(root.clone(), here, "Empty".to_owned(), None)
			.unwrap()
			.id();
		let empty_path = tree::path(&read_manifest(&root).unwrap().nodes, empty).unwrap();
		let draft = checkpoint(&root, "On Draft", "").unwrap().to_string();
		fs::write(root.join(HISTORY_FILE), "[]").unwrap();

		switch(&root, "main").unwrap();
		assert_eq!(head_target(&root), "refs/heads/main");
		assert!(changes(&root, Some(&main), None).unwrap().nodes.is_empty());
		assert!(!pending(&root).unwrap());
		assert_eq!(body_now(&root, edited), "Main's text");
		assert!(!root.join(&empty_path).exists());
		assert_eq!(fs::read_to_string(root.join(HISTORY_FILE)).unwrap(), "[]");
		assert_eq!(list_trash(root.clone()).unwrap().len(), 1);

		switch(&root, "draft").unwrap();
		assert!(changes(&root, Some(&draft), None).unwrap().nodes.is_empty());
		assert!(!pending(&root).unwrap());
		assert!(root.join(&empty_path).is_dir());
	}

	const BASE: &str = "One.\n\nTwo.\n\nThree.\n";

	/// A novel whose first document has `BASE`, then a version "Act two"
	/// where it reads `draft`, and Main where it reads `main`. Act two also
	/// edits a second document. Returns the two documents and both checkpoints.
	fn forked(main: &str, draft: &str) -> (tempfile::TempDir, PathBuf, Uuid, Uuid, Oid, Oid) {
		let (parent, root, seeded, here, _) = novel();
		let other = create_document(root.clone(), here, "Other".to_owned())
			.unwrap()
			.id;
		write_document(root.clone(), seeded, BASE.to_owned()).unwrap();
		checkpoint(&root, "Base", "").unwrap();

		new_branch(&root, "Act two").unwrap();
		write_document(root.clone(), seeded, draft.to_owned()).unwrap();
		write_document(root.clone(), other, "Only in Act two".to_owned()).unwrap();
		let draft = checkpoint(&root, "In Act two", "").unwrap();

		switch(&root, "main").unwrap();
		let main = match main == BASE {
			true => repo_head(&root),
			false => {
				write_document(root.clone(), seeded, main.to_owned()).unwrap();
				checkpoint(&root, "On Main", "").unwrap()
			}
		};
		(parent, root, seeded, other, main, draft)
	}

	fn parents(root: &Path) -> Vec<Oid> {
		let repo = Repository::open(root).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		head.parent_ids().collect()
	}

	fn merge_state_of(root: &Path) -> RepositoryState {
		Repository::open(root).unwrap().state()
	}

	#[test]
	fn a_clean_merge_adds_its_own_checkpoint() {
		let (_parent, root, seeded, other, main, draft) = forked(
			"One, on Main.\n\nTwo.\n\nThree.\n",
			"One.\n\nTwo.\n\nThree, in Act two.\n",
		);

		assert_eq!(merge(&root, "act-two", "").unwrap(), None);

		let repo = Repository::open(&root).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		assert_eq!(head.message().unwrap(), "Merged Act two");
		assert_eq!(parents(&root), [main, draft]);
		assert_eq!(
			read_document(root.clone(), seeded).unwrap(),
			"One, on Main.\n\nTwo.\n\nThree, in Act two.\n"
		);
		assert_eq!(
			read_document(root.clone(), other).unwrap(),
			"Only in Act two"
		);
		assert!(!pending(&root).unwrap());
		assert_eq!(merging(&root).unwrap(), None);
		assert_eq!(merge_state_of(&root), RepositoryState::Clean);

		let again = merge(&root, "act-two", "");
		assert!(matches!(again, Err(Error::AlreadyMerged)));
	}

	#[test]
	fn a_merge_git_could_fast_forward_still_adds_a_checkpoint() {
		let (_parent, root, seeded, _, main, draft) = forked(BASE, "One, in Act two.\n");

		assert_eq!(merge(&root, "act-two", "").unwrap(), None);

		assert_eq!(parents(&root), [main, draft]);
		assert_eq!(
			read_document(root.clone(), seeded).unwrap(),
			"One, in Act two.\n"
		);
	}

	#[test]
	fn merging_waits_while_changes_are_not_in_a_checkpoint() {
		let (_parent, root, seeded, _, main, _) = forked(BASE, "One, in Act two.\n");
		write_document(root.clone(), seeded, "Not yet kept".to_owned()).unwrap();

		let refused = merge(&root, "act-two", "");

		assert!(matches!(refused, Err(Error::ChangesPending)));
		assert_eq!(repo_head(&root), main);
		assert_eq!(merge_state_of(&root), RepositoryState::Clean);
	}

	const MAINS: &str = "One, on Main.\n\nTwo.\n\nThree.\n";

	/// Main and Act two both rewrote the first paragraph.
	fn conflicted() -> (tempfile::TempDir, PathBuf, Uuid, Uuid, Oid, Oid) {
		let forked = forked(MAINS, "One, in Act two.\n\nTwo.\n\nThree.\n");
		let merging = merge(&forked.1, "act-two", "").unwrap().unwrap();
		let path = tree::path(&read_manifest(&forked.1).unwrap().nodes, forked.2).unwrap();
		assert_eq!(
			merging,
			Merging {
				name: Some("Act two".to_owned()),
				message: "Merged Act two".to_owned(),
				conflicts: vec![merge::Conflict::Text {
					id: Some(forked.2),
					path,
				}],
			}
		);
		forked
	}

	#[test]
	fn a_conflicting_paragraph_leaves_this_versions_text_and_waits() {
		let (_parent, root, seeded, other, main, _) = conflicted();

		assert_eq!(repo_head(&root), main);
		assert_eq!(merge_state_of(&root), RepositoryState::Merge);
		assert_eq!(read_document(root.clone(), seeded).unwrap(), MAINS);
		assert_eq!(
			read_document(root.clone(), other).unwrap(),
			"Only in Act two"
		);
		assert_eq!(merging(&root).unwrap().unwrap().conflicts.len(), 1);

		let repo = Repository::open(&root).unwrap();
		let index = repo.index().unwrap();
		let conflict = index.conflicts().unwrap().next().unwrap().unwrap();
		assert!(conflict.ancestor.is_some() && conflict.our.is_some() && conflict.their.is_some());
		assert!(index.get_path(Path::new(MANIFEST_FILE), 0).is_some());
	}

	#[test]
	fn aborting_returns_to_the_last_checkpoint() {
		let (_parent, root, seeded, other, main, _) = conflicted();

		abort(&root).unwrap();

		assert_eq!(repo_head(&root), main);
		assert_eq!(merge_state_of(&root), RepositoryState::Clean);
		assert_eq!(merging(&root).unwrap(), None);
		assert!(!pending(&root).unwrap());
		assert_eq!(read_document(root.clone(), seeded).unwrap(), MAINS);
		assert_eq!(read_document(root.clone(), other).unwrap(), "");
	}

	#[test]
	fn aborting_with_no_merge_keeps_changes() {
		let (_parent, root, seeded, ..) = forked(BASE, "One, in Act two.\n");
		write_document(root.clone(), seeded, "Not yet kept".to_owned()).unwrap();

		abort(&root).unwrap();

		assert_eq!(body_now(&root, seeded), "Not yet kept");
	}

	#[test]
	fn a_checkpoint_finishes_the_merge_once_conflicts_are_settled() {
		let (_parent, root, seeded, _, main, draft) = conflicted();

		let refused = checkpoint(&root, "Merged Act two", "");
		assert!(matches!(refused, Err(Error::ConflictsRemain)));

		write_document(root.clone(), seeded, "One, settled.\n".to_owned()).unwrap();
		stage_all(root.clone()).unwrap();
		checkpoint(&root, "Merged Act two", "").unwrap();

		assert_eq!(parents(&root), [main, draft]);
		assert_eq!(merge_state_of(&root), RepositoryState::Clean);
		assert_eq!(merging(&root).unwrap(), None);
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn a_conflicted_document_comes_apart_into_clean_text_and_spots() {
		let (_parent, root, seeded, other, ..) = forked(
			"One, on Main.\n\nTwo.\n\n<<<<<<< typed\n\nThree, on Main.\n",
			"One, in Act two.\n\nTwo.\n\n<<<<<<< typed\n\nThree, in Act two.\n",
		);
		merge(&root, "act-two", "").unwrap().unwrap();

		assert_eq!(
			spots(&root, seeded).unwrap(),
			Some(vec![
				Piece::Spot {
					ours: "One, on Main.\n".to_owned(),
					theirs: "One, in Act two.\n".to_owned(),
				},
				Piece::Clean {
					text: "\nTwo.\n\n<<<<<<< typed\n\n".to_owned(),
				},
				Piece::Spot {
					ours: "Three, on Main.\n".to_owned(),
					theirs: "Three, in Act two.\n".to_owned(),
				},
			])
		);
		assert_eq!(spots(&root, other).unwrap(), None);
	}

	#[test]
	fn settling_a_document_takes_it_off_the_conflicts() {
		let (_parent, root, seeded, _, main, draft) = conflicted();

		settle(&root, seeded, "One, settled.\n\nTwo.\n\nThree.\n").unwrap();

		assert_eq!(spots(&root, seeded).unwrap(), None);
		assert_eq!(merging(&root).unwrap().unwrap().conflicts, []);
		assert_eq!(
			read_document(root.clone(), seeded).unwrap(),
			"One, settled.\n\nTwo.\n\nThree.\n"
		);
		checkpoint(&root, "Merged Act two", "").unwrap();
		assert_eq!(parents(&root), [main, draft]);
		assert!(!pending(&root).unwrap());
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
		begun(&root);
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
		checkpoint(root, "Kept", "").unwrap().to_string()
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
	fn before_anything_is_kept_every_document_reads_as_added() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(
			parent.path(),
			"Ithaca",
			Format::Novel,
			OffsetDateTime::UNIX_EPOCH,
		)
		.unwrap();
		ensure_repository(&root).unwrap();

		let changes = changes(&root, Some("HEAD"), None).unwrap();
		let documents = read_manifest(&root).unwrap().documents();
		assert!(!documents.is_empty());
		for document in documents {
			let change = change_of(&changes, document.id).unwrap();
			assert_eq!(change.before, None);
			assert!(change.after.is_some());
		}
		assert!(!root.join(".git/index").exists());
	}

	#[test]
	fn comparing_against_now_leaves_what_is_staged_alone() {
		let (_parent, root, seeded, ..) = novel();
		write_document(root.clone(), seeded, "Unkept".to_owned()).unwrap();
		let index = root.join(".git/index");
		let staged = fs::read(&index).unwrap();

		let changes = changes(&root, Some("HEAD"), None).unwrap();
		assert!(change_of(&changes, seeded).unwrap().edited);
		assert_eq!(fs::read(&index).unwrap(), staged);
	}

	#[test]
	fn before_the_first_checkpoint_nothing_is_staged_and_everything_is_added() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(
			parent.path(),
			"Ithaca",
			Format::Novel,
			OffsetDateTime::UNIX_EPOCH,
		)
		.unwrap();
		ensure_repository(&root).unwrap();

		let status = status(&root).unwrap();
		assert_eq!(status.staged.nodes, Vec::new());
		assert!(!status.staged.book);
		assert!(!status.staged.cover);
		for document in read_manifest(&root).unwrap().documents() {
			assert_eq!(
				change_of(&status.unstaged, document.id).unwrap().before,
				None
			);
		}
		assert!(status.unstaged.book);
		assert!(!root.join(".git/index").exists());
	}

	#[test]
	fn after_a_checkpoint_every_kind_of_change_is_unstaged() {
		let (_parent, root, seeded, here, there) = novel();
		let add = |parent: Uuid, name: &str| {
			create_document(root.clone(), parent, name.to_owned())
				.unwrap()
				.id
		};
		let (moving, doomed, renaming) = (
			add(here, "Moving"),
			add(here, "Doomed"),
			add(here, "Renaming"),
		);
		let (_, two) = (add(here, "One"), add(here, "Two"));
		let part = create_folder(root.clone(), there, "Part".to_owned(), None)
			.unwrap()
			.id();
		checkpoint(&root, "Set up", "").unwrap();

		write_document(root.clone(), seeded, "Edited".to_owned()).unwrap();
		rename_document(root.clone(), renaming, "Renamed".to_owned()).unwrap();
		move_node(root.clone(), moving, there, 0).unwrap();
		delete_document(root.clone(), doomed).unwrap();
		let added = add(here, "Added");
		move_node(root.clone(), two, here, 0).unwrap();
		rename_folder(root.clone(), part, "Act".to_owned()).unwrap();
		let mut book = read_manifest(&root).unwrap().book;
		book.title = "Odyssey".to_owned();
		write_book(root.clone(), book).unwrap();
		let image = root.parent().unwrap().join("cover.png");
		fs::write(&image, b"\x89PNG\r\n\x1a\none").unwrap();
		set_cover(root.clone(), image).unwrap();

		let status = status(&root).unwrap();
		assert!(status.staged.nodes.is_empty() && !status.staged.book && !status.staged.cover);
		let unstaged = &status.unstaged;
		assert_eq!(*unstaged, changes(&root, Some("HEAD"), None).unwrap());
		assert!(change_of(unstaged, seeded).unwrap().edited);
		assert!(change_of(unstaged, renaming).unwrap().renamed);
		assert!(change_of(unstaged, moving).unwrap().moved);
		assert_eq!(change_of(unstaged, doomed).unwrap().after, None);
		assert_eq!(change_of(unstaged, added).unwrap().before, None);
		assert!(change_of(unstaged, part).unwrap().renamed);
		let reordered = &change_of(unstaged, here).unwrap().reordered;
		assert_eq!(reordered.len(), 1);
		assert!(reordered[0].name.starts_with("Two"));
		assert!(unstaged.book && unstaged.cover);
	}

	#[test]
	fn a_document_staged_by_hand_moves_to_the_next_checkpoint() {
		let (_parent, root, seeded, here, _) = novel();
		let other = create_document(root.clone(), here, "Other".to_owned())
			.unwrap()
			.id;
		checkpoint(&root, "Set up", "").unwrap();
		write_document(root.clone(), seeded, "Staged".to_owned()).unwrap();
		write_document(root.clone(), other, "Not staged".to_owned()).unwrap();

		let documents = read_manifest(&root).unwrap().documents();
		let path = &documents.iter().find(|d| d.id == seeded).unwrap().path;
		let repo = Repository::open(&root).unwrap();
		let mut index = repo.index().unwrap();
		index.add_path(Path::new(path)).unwrap();
		index.write().unwrap();

		let status = super::status(&root).unwrap();
		assert!(change_of(&status.staged, seeded).unwrap().edited);
		assert!(change_of(&status.unstaged, seeded).is_none());
		assert!(change_of(&status.staged, other).is_none());
		assert!(change_of(&status.unstaged, other).unwrap().edited);

		// Written again after staging, so partly in the next checkpoint.
		write_document(root.clone(), seeded, "Staged, then more".to_owned()).unwrap();
		let status = super::status(&root).unwrap();
		assert!(change_of(&status.staged, seeded).unwrap().edited);
		assert!(change_of(&status.unstaged, seeded).unwrap().edited);
	}

	fn rows(changes: &Changes) -> Vec<Row> {
		let mut rows: Vec<Row> = changes
			.nodes
			.iter()
			.map(|change| Row::Node { id: change.id })
			.collect();
		if changes.book {
			rows.push(Row::Book);
		}
		if changes.cover {
			rows.push(Row::Cover);
		}
		rows
	}

	fn nothing_in(changes: &Changes) -> bool {
		changes.nodes.is_empty() && !changes.book && !changes.cover
	}

	fn checkpoint_manifest(root: &Path, id: Oid) -> Manifest {
		let repo = Repository::open(root).unwrap();
		side(&repo, Some(&id.to_string())).unwrap().1
	}

	#[test]
	fn staging_one_of_two_moved_documents_checkpoints_only_that_one() {
		let (_parent, root, _, here, there) = novel();
		let add = |name: &str| {
			create_document(root.clone(), here, name.to_owned())
				.unwrap()
				.id
		};
		let (staged, left) = (add("Staged"), add("Left"));
		checkpoint(&root, "Set up", "").unwrap();
		move_node(root.clone(), staged, there, 0).unwrap();
		move_node(root.clone(), left, there, 0).unwrap();

		carry(&root, Row::Node { id: staged }, true).unwrap();
		let status = status(&root).unwrap();
		assert!(change_of(&status.staged, staged).unwrap().moved);
		assert!(change_of(&status.staged, left).is_none());
		assert!(change_of(&status.unstaged, left).unwrap().moved);
		assert!(change_of(&status.unstaged, staged).is_none());

		let id = checkpoint(&root, "Moved one", "").unwrap();
		let kept = checkpoint_manifest(&root, id);
		let parent = |id: Uuid| tree::parent(&kept.nodes, id).map(Node::id);
		assert_eq!(parent(staged), Some(there));
		assert_eq!(parent(left), Some(here));

		// The checkpoint, written out on its own, reads back with every id.
		let out = tempfile::tempdir().unwrap();
		let repo = Repository::open(&root).unwrap();
		let tree = repo.find_commit(id).unwrap().tree().unwrap();
		repo.checkout_tree(
			tree.as_object(),
			Some(CheckoutBuilder::new().target_dir(out.path()).force()),
		)
		.unwrap();
		let mut read = crate::document::refresh(out.path()).unwrap().documents();
		let mut held = kept.documents();
		read.sort_by_key(|document| document.id);
		held.sort_by_key(|document| document.id);
		assert_eq!(read, held);
		assert!(
			change_of(&super::status(&root).unwrap().unstaged, left)
				.unwrap()
				.moved
		);
	}

	#[test]
	fn a_document_brings_its_new_folder_and_the_folder_takes_it_back() {
		let (_parent, root, _, here, _) = novel();
		checkpoint(&root, "Set up", "").unwrap();
		let act = create_folder(root.clone(), here, "Act".to_owned(), None)
			.unwrap()
			.id();
		let add = |name: &str| {
			create_document(root.clone(), act, name.to_owned())
				.unwrap()
				.id
		};
		let (scene, other) = (add("Scene"), add("Other"));

		carry(&root, Row::Node { id: scene }, true).unwrap();
		let status = status(&root).unwrap();
		assert_eq!(change_of(&status.staged, act).unwrap().before, None);
		assert_eq!(change_of(&status.staged, scene).unwrap().before, None);
		assert!(change_of(&status.staged, other).is_none());
		assert!(change_of(&status.unstaged, other).is_some());

		carry(&root, Row::Node { id: act }, false).unwrap();
		assert!(nothing_in(&super::status(&root).unwrap().staged));
	}

	#[test]
	fn unstaging_writes_back_what_the_last_checkpoint_had() {
		let (_parent, root, seeded, _, there) = novel();
		checkpoint(&root, "Set up", "").unwrap();
		write_document(root.clone(), seeded, "Edited".to_owned()).unwrap();
		rename_document(root.clone(), seeded, "Renamed".to_owned()).unwrap();
		move_node(root.clone(), seeded, there, 0).unwrap();

		carry(&root, Row::Node { id: seeded }, true).unwrap();
		let staged = status(&root).unwrap().staged;
		let change = change_of(&staged, seeded).unwrap();
		assert!(change.edited && change.renamed && change.moved);

		carry(&root, Row::Node { id: seeded }, false).unwrap();
		let status = super::status(&root).unwrap();
		assert!(nothing_in(&status.staged));
		let change = change_of(&status.unstaged, seeded).unwrap();
		assert!(change.edited && change.renamed && change.moved);
	}

	#[test]
	fn the_book_the_cover_and_a_folders_order_stage_on_their_own() {
		let (_parent, root, _, here, _) = novel();
		let add = |name: &str| {
			create_document(root.clone(), here, name.to_owned())
				.unwrap()
				.id
		};
		let (_, last) = (add("One"), add("Two"));
		checkpoint(&root, "Set up", "").unwrap();
		move_node(root.clone(), last, here, 0).unwrap();
		let mut book = read_manifest(&root).unwrap().book;
		book.title = "Odyssey".to_owned();
		write_book(root.clone(), book).unwrap();
		let image = root.parent().unwrap().join("cover.png");
		fs::write(&image, b"\x89PNG\r\n\x1a\none").unwrap();
		set_cover(root.clone(), image).unwrap();

		carry(&root, Row::Book, true).unwrap();
		let status = status(&root).unwrap();
		assert!(status.staged.book && !status.staged.cover && status.staged.nodes.is_empty());
		assert!(status.unstaged.cover && !status.unstaged.book);

		carry(&root, Row::Cover, true).unwrap();
		carry(&root, Row::Node { id: here }, true).unwrap();
		let status = super::status(&root).unwrap();
		assert!(status.staged.cover);
		assert_eq!(change_of(&status.staged, here).unwrap().reordered.len(), 1);
		assert!(nothing_in(&status.unstaged));
	}

	/// Every kind of change, so staging them one at a time has to arrive at
	/// the project as it is.
	fn everything_changed(root: &Path, seeded: Uuid, here: Uuid, there: Uuid) {
		let add = |parent: Uuid, name: &str| {
			create_document(root.to_path_buf(), parent, name.to_owned())
				.unwrap()
				.id
		};
		let (moving, doomed, renaming) = (
			add(here, "Moving"),
			add(here, "Doomed"),
			add(here, "Renaming"),
		);
		let two = add(here, "Two");
		let part = create_folder(root.to_path_buf(), there, "Part".to_owned(), None)
			.unwrap()
			.id();
		checkpoint(root, "Set up", "").unwrap();

		write_document(root.to_path_buf(), seeded, "Edited".to_owned()).unwrap();
		rename_document(root.to_path_buf(), renaming, "Renamed".to_owned()).unwrap();
		move_node(root.to_path_buf(), moving, part, 0).unwrap();
		delete_document(root.to_path_buf(), doomed).unwrap();
		move_node(root.to_path_buf(), two, here, 0).unwrap();
		rename_folder(root.to_path_buf(), part, "Act".to_owned()).unwrap();
		let act = create_folder(root.to_path_buf(), there, "New".to_owned(), None)
			.unwrap()
			.id();
		add(act, "Inside");
		add(here, "Added");
		let mut book = read_manifest(root).unwrap().book;
		book.title = "Odyssey".to_owned();
		write_book(root.to_path_buf(), book).unwrap();
		let image = root.parent().unwrap().join("cover.png");
		fs::write(&image, b"\x89PNG\r\n\x1a\none").unwrap();
		set_cover(root.to_path_buf(), image).unwrap();
	}

	#[test]
	fn staging_every_row_one_at_a_time_arrives_at_the_project_as_it_is() {
		let (_parent, root, seeded, here, there) = novel();
		everything_changed(&root, seeded, here, there);

		for row in rows(&status(&root).unwrap().unstaged) {
			carry(&root, row, true).unwrap();
		}
		assert!(nothing_in(&status(&root).unwrap().unstaged));
		let repo = Repository::open(&root).unwrap();
		let index = repo.index().unwrap();
		let manifest = index.get_path(Path::new(MANIFEST_FILE), 0).unwrap();
		assert_eq!(
			repo.find_blob(manifest.id).unwrap().content(),
			fs::read(root.join(MANIFEST_FILE)).unwrap()
		);

		checkpoint(&root, "All of it", "").unwrap();
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn unstaging_every_row_one_at_a_time_leaves_nothing_staged() {
		let (_parent, root, seeded, here, there) = novel();
		everything_changed(&root, seeded, here, there);
		stage_all(root.clone()).unwrap();

		for row in rows(&status(&root).unwrap().staged) {
			carry(&root, row, false).unwrap();
		}
		let status = super::status(&root).unwrap();
		assert!(nothing_in(&status.staged));
		assert_eq!(status.unstaged, changes(&root, Some("HEAD"), None).unwrap());
	}

	#[test]
	fn unstaging_all_and_checkpointing_takes_everything() {
		let (_parent, root, seeded, here, there) = novel();
		everything_changed(&root, seeded, here, there);
		carry(&root, Row::Book, true).unwrap();
		unstage_all(root.clone()).unwrap();
		assert!(nothing_in(&status(&root).unwrap().staged));

		checkpoint(&root, "All of it", "").unwrap();
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn the_first_checkpoint_can_hold_one_document() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(
			parent.path(),
			"Ithaca",
			Format::Novel,
			OffsetDateTime::UNIX_EPOCH,
		)
		.unwrap();
		ensure_repository(&root).unwrap();
		let documents = read_manifest(&root).unwrap().documents();
		let first = documents[0].id;

		carry(&root, Row::Node { id: first }, true).unwrap();
		let status = status(&root).unwrap();
		assert_eq!(change_of(&status.staged, first).unwrap().before, None);
		assert!(!status.staged.book);

		let id = checkpoint(&root, "First", "").unwrap();
		let kept = checkpoint_manifest(&root, id);
		let ids: Vec<Uuid> = kept.documents().iter().map(|d| d.id).collect();
		assert_eq!(ids, vec![first]);
		assert!(super::status(&root).unwrap().unstaged.book);
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

	/// The last checkpoint and what the next one holds.
	fn checkpoint_and_index(root: &Path) -> (Oid, Oid) {
		let repo = Repository::open(root).unwrap();
		let index = repo.index().unwrap().write_tree().unwrap();
		(repo_head(root), index)
	}

	#[test]
	fn a_document_comes_back_with_its_text_name_and_folder() {
		let (_parent, root, seeded, _, there) = novel();
		write_document(root.clone(), seeded, "First".to_owned()).unwrap();
		let first = kept(&root);
		rename_document(root.clone(), seeded, "Renamed".to_owned()).unwrap();
		move_node(root.clone(), seeded, there, 0).unwrap();
		write_document(root.clone(), seeded, "Second".to_owned()).unwrap();
		stage_all(root.clone()).unwrap();
		let before = checkpoint_and_index(&root);

		put_back_document(root.clone(), first.clone(), seeded).unwrap();

		assert!(changes(&root, Some(&first), None).unwrap().nodes.is_empty());
		assert_eq!(body_now(&root, seeded), "First");
		assert_eq!(checkpoint_and_index(&root), before);
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

		let before = checkpoint_and_index(&root);

		put_back_document(root.clone(), start.clone(), doomed).unwrap();
		put_back_document(root.clone(), start.clone(), newer).unwrap();

		// Nothing differs, so the id and the place among the others are back.
		assert!(changes(&root, Some(&start), None).unwrap().nodes.is_empty());
		assert_eq!(body_now(&root, doomed), "Kept");
		assert_eq!(list_trash(root.clone()).unwrap().len(), 2);
		assert_eq!(checkpoint_and_index(&root), before);
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

		stage_all(root.clone()).unwrap();
		let before = checkpoint_and_index(&root);

		put_back_book(root.clone(), start.clone()).unwrap();

		let changes = changes(&root, Some(&start), None).unwrap();
		assert!(!changes.book && !changes.cover);
		assert!(root.join("cover.png").is_file());
		assert!(!root.join("cover.jpg").exists());
		assert_eq!(checkpoint_and_index(&root), before);
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
		let staged = folder(there, "Staged");
		create_document(root.clone(), staged, "Inside".to_owned()).unwrap();
		let staged_path = path_of(staged);
		stage_all(root.clone()).unwrap();
		// Added after staging, so git has never seen it.
		let later = folder(there, "Later");
		create_document(root.clone(), later, "Inside".to_owned()).unwrap();
		let later_path = path_of(later);
		fs::write(root.join(HISTORY_FILE), "[]").unwrap();
		let before = checkpoint_and_index(&root);

		put_back_everything(root.clone(), start.clone()).unwrap();

		assert!(changes(&root, Some(&start), None).unwrap().nodes.is_empty());
		assert_eq!(body_now(&root, seeded), "First");
		assert!(root.join(&empty_path).is_dir());
		assert!(!root.join(&staged_path).exists());
		assert!(!root.join(&later_path).exists());
		assert_eq!(fs::read_to_string(root.join(HISTORY_FILE)).unwrap(), "[]");
		assert_eq!(list_trash(root.clone()).unwrap().len(), 1);
		assert_eq!(checkpoint_and_index(&root), before);
	}

	/// Keeps the project, does `there` in a new version "Act two" and `here`
	/// on Main, each kept, then merges Act two into Main.
	fn diverged(root: &Path, there: impl FnOnce(), here: impl FnOnce()) -> Vec<merge::Conflict> {
		checkpoint(root, "Base", "").unwrap();
		new_branch(root, "Act two").unwrap();
		there();
		checkpoint(root, "In Act two", "").unwrap();
		switch(root, "main").unwrap();
		here();
		checkpoint(root, "On Main", "").unwrap();
		let conflicts = merge(root, "act-two", "")
			.unwrap()
			.map_or_else(Vec::new, |merging| merging.conflicts);
		assert_eq!(
			merging(root)
				.unwrap()
				.map_or_else(Vec::new, |m| m.conflicts),
			conflicts
		);
		conflicts
	}

	/// The folder and name the project on disk gives a node.
	fn placed(root: &Path, id: Uuid) -> (Option<Uuid>, String) {
		let nodes = read_manifest(root).unwrap().nodes;
		let name = tree::find(&nodes, id).unwrap().name().to_owned();
		(tree::parent(&nodes, id).map(Node::id), name)
	}

	fn children_of(root: &Path, folder: Uuid) -> Vec<Uuid> {
		let nodes = read_manifest(root).unwrap().nodes;
		tree::children(&nodes, folder)
			.unwrap()
			.iter()
			.map(Node::id)
			.collect()
	}

	/// Settling is the merge screen's business, so until then a checkpoint
	/// is refused and the Changes list still reads.
	fn waits(root: &Path) {
		assert_eq!(merge_state_of(root), RepositoryState::Merge);
		assert!(status(root).is_ok());
		let refused = checkpoint(root, "Merged Act two", "");
		assert!(matches!(refused, Err(Error::ConflictsRemain)));
	}

	#[test]
	fn a_scene_one_side_moved_keeps_the_others_edit() {
		let (_parent, root, seeded, _, there) = novel();
		let conflicts = diverged(
			&root,
			|| move_node(root.clone(), seeded, there, 0).unwrap(),
			|| write_document(root.clone(), seeded, "Edited on Main".to_owned()).unwrap(),
		);

		assert_eq!(conflicts, []);
		assert_eq!(placed(&root, seeded).0, Some(there));
		assert_eq!(body_now(&root, seeded), "Edited on Main");
		assert_eq!(parents(&root).len(), 2);
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn empty_scenes_moved_and_renamed_keep_their_text() {
		let (_parent, root, _, here, there) = novel();
		let add = |name: &str| {
			create_document(root.clone(), here, name.to_owned())
				.unwrap()
				.id
		};
		let (a, b) = (add("A"), add("B"));
		let conflicts = diverged(
			&root,
			|| {
				move_node(root.clone(), b, there, 0).unwrap();
				move_node(root.clone(), a, there, 0).unwrap();
				rename_document(root.clone(), a, "Z".to_owned()).unwrap();
			},
			|| write_document(root.clone(), a, "Hello".to_owned()).unwrap(),
		);

		assert_eq!(conflicts, []);
		assert_eq!(placed(&root, a), (Some(there), "Z.md".to_owned()));
		assert_eq!(read_document(root.clone(), a).unwrap(), "Hello");
		assert_eq!(read_document(root.clone(), b).unwrap(), "");
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn two_new_documents_with_one_name_are_both_kept() {
		let (_parent, root, _, here, _) = novel();
		let add = || {
			create_document(root.clone(), here, "New".to_owned()).unwrap();
		};
		assert_eq!(diverged(&root, add, add), []);

		let names: Vec<String> = children_of(&root, here)
			.into_iter()
			.map(|id| placed(&root, id).1)
			.collect();
		assert!(names.contains(&"New.md".to_owned()));
		assert!(names.contains(&"New 2.md".to_owned()));
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn renamed_on_both_keeps_this_versions_name() {
		let (_parent, root, seeded, ..) = novel();
		let conflicts = diverged(
			&root,
			|| {
				rename_document(root.clone(), seeded, "There".to_owned()).unwrap();
			},
			|| {
				rename_document(root.clone(), seeded, "Here".to_owned()).unwrap();
			},
		);

		assert_eq!(
			conflicts,
			[merge::Conflict::Renamed {
				id: seeded,
				ours: "Here.md".to_owned(),
				theirs: "There.md".to_owned(),
			}]
		);
		assert_eq!(placed(&root, seeded).1, "Here.md");
		waits(&root);
	}

	#[test]
	fn moved_on_both_keeps_this_versions_folder() {
		let (_parent, root, seeded, here, there) = novel();
		let conflicts = diverged(
			&root,
			|| move_node(root.clone(), seeded, there, 0).unwrap(),
			|| move_node(root.clone(), seeded, here, 0).unwrap(),
		);

		assert_eq!(
			conflicts,
			[merge::Conflict::Moved {
				id: seeded,
				ours: Some(here),
				theirs: Some(there),
			}]
		);
		assert_eq!(placed(&root, seeded).0, Some(here));
		waits(&root);
	}

	#[test]
	fn deleted_on_one_side_and_edited_on_the_other() {
		let (_parent, root, seeded, here, _) = novel();
		let other = create_document(root.clone(), here, "Other".to_owned())
			.unwrap()
			.id;
		let conflicts = diverged(
			&root,
			|| {
				delete_document(root.clone(), seeded).unwrap();
				write_document(root.clone(), other, "Theirs".to_owned()).unwrap();
			},
			|| {
				write_document(root.clone(), seeded, "Ours".to_owned()).unwrap();
				delete_document(root.clone(), other).unwrap();
			},
		);

		assert_eq!(
			conflicts,
			[
				merge::Conflict::Deleted {
					id: seeded,
					here: false,
					name: placed(&root, seeded).1,
					documents: 1,
				},
				merge::Conflict::Deleted {
					id: other,
					here: true,
					name: "Other.md".to_owned(),
					documents: 1,
				},
			]
		);
		assert_eq!(body_now(&root, seeded), "Ours");
		assert!(tree::find(&read_manifest(&root).unwrap().nodes, other).is_none());
		waits(&root);
	}

	/// A folder holding three new documents, in order.
	fn three_in(root: &Path, under: Uuid) -> (Uuid, [Uuid; 3]) {
		let folder = create_folder(root.to_path_buf(), under, "Scenes".to_owned(), None)
			.unwrap()
			.id();
		let add = |name: &str| {
			create_document(root.to_path_buf(), folder, name.to_owned())
				.unwrap()
				.id
		};
		(folder, [add("A"), add("B"), add("C")])
	}

	#[test]
	fn reordered_on_both_keeps_this_versions_order() {
		let (_parent, root, _, here, _) = novel();
		let (folder, [a, b, c]) = three_in(&root, here);
		let conflicts = diverged(
			&root,
			|| move_node(root.clone(), c, folder, 0).unwrap(),
			|| move_node(root.clone(), a, folder, 2).unwrap(),
		);

		assert_eq!(
			conflicts,
			[merge::Conflict::Reordered {
				id: Some(folder),
				ours: vec![b, c, a],
				theirs: vec![c, a, b],
			}]
		);
		assert_eq!(children_of(&root, folder), [b, c, a]);
		waits(&root);
	}

	#[test]
	fn one_sides_order_takes_the_others_new_document() {
		let (_parent, root, _, here, _) = novel();
		let (folder, [a, b, c]) = three_in(&root, here);
		let added = std::cell::Cell::new(None);
		let conflicts = diverged(
			&root,
			|| move_node(root.clone(), c, folder, 0).unwrap(),
			|| {
				let id = create_document(root.clone(), folder, "D".to_owned())
					.unwrap()
					.id;
				added.set(Some(id));
			},
		);

		// A new document follows the one it was added after.
		assert_eq!(conflicts, []);
		assert_eq!(children_of(&root, folder), [c, added.get().unwrap(), a, b]);
	}

	#[test]
	fn the_book_and_its_cover_changed_on_both() {
		let (_parent, root, ..) = novel();
		let change = |title: &str, image: &[u8]| {
			let png = root.parent().unwrap().join("cover.png");
			fs::write(&png, image).unwrap();
			set_cover(root.clone(), png).unwrap();
			let mut book = read_manifest(&root).unwrap().book;
			book.title = title.to_owned();
			write_book(root.clone(), book).unwrap();
		};
		let conflicts = diverged(
			&root,
			|| change("Theirs", b"\x89PNG\r\n\x1a\ntheirs"),
			|| change("Ours", b"\x89PNG\r\n\x1a\nours"),
		);

		assert_eq!(
			conflicts,
			[
				merge::Conflict::Field {
					id: None,
					field: "title".to_owned(),
					ours: "Ours".into(),
					theirs: "Theirs".into(),
				},
				merge::Conflict::Cover {
					ours: Some("cover.png".to_owned()),
					theirs: Some("cover.png".to_owned()),
				},
			]
		);
		assert_eq!(read_manifest(&root).unwrap().book.title, "Ours");
		assert!(fs::read(root.join("cover.png")).unwrap().ends_with(b"ours"));
		waits(&root);
	}

	#[test]
	fn a_documents_target_changed_on_both() {
		let (_parent, root, seeded, ..) = novel();
		let target = |words: u32| {
			let mut manifest = read_manifest(&root).unwrap();
			if let Some(Node::Document { target, .. }) = tree::find_mut(&mut manifest.nodes, seeded)
			{
				*target = Some(words);
			}
			write_manifest(&root, &mut manifest).unwrap();
		};
		let conflicts = diverged(&root, || target(100), || target(200));

		assert_eq!(
			conflicts,
			[merge::Conflict::Field {
				id: Some(seeded),
				field: "target".to_owned(),
				ours: 200.into(),
				theirs: 100.into(),
			}]
		);
		waits(&root);
	}

	#[test]
	fn the_changes_list_reads_while_a_paragraph_conflicts() {
		let (_parent, root, seeded, other, ..) = conflicted();

		let status = status(&root).unwrap();

		assert!(status.unstaged.nodes.is_empty());
		let edited: Vec<Uuid> = status
			.staged
			.nodes
			.iter()
			.filter(|change| change.edited)
			.map(|change| change.id)
			.collect();
		assert_eq!(edited, [other]);
		assert!(!edited.contains(&seeded));
	}

	/// Settles the binder with every conflict the other version's way or
	/// none of them, then checks the merge finishes with the files matching.
	fn settled_as(root: &Path, conflicts: Vec<merge::Conflict>, theirs: bool) {
		let chosen = if theirs { conflicts } else { Vec::new() };
		settle_rest(root, &chosen).unwrap();
		assert_eq!(merging(root).unwrap().unwrap().conflicts, []);
		checkpoint(root, "Merged Act two", "").unwrap();
		assert_eq!(parents(root).len(), 2);
		assert!(!pending(root).unwrap());
	}

	#[test]
	fn a_rename_on_both_settles_either_way() {
		for theirs in [false, true] {
			let (_parent, root, seeded, ..) = novel();
			write_document(root.clone(), seeded, "Kept".to_owned()).unwrap();
			let conflicts = diverged(
				&root,
				|| {
					rename_document(root.clone(), seeded, "There".to_owned()).unwrap();
				},
				|| {
					rename_document(root.clone(), seeded, "Here".to_owned()).unwrap();
				},
			);

			settled_as(&root, conflicts, theirs);
			let name = if theirs { "There.md" } else { "Here.md" };
			assert_eq!(placed(&root, seeded).1, name);
			assert_eq!(body_now(&root, seeded), "Kept");
		}
	}

	#[test]
	fn a_move_on_both_settles_either_way() {
		for theirs in [false, true] {
			let (_parent, root, seeded, here, there) = novel();
			let conflicts = diverged(
				&root,
				|| move_node(root.clone(), seeded, there, 0).unwrap(),
				|| move_node(root.clone(), seeded, here, 0).unwrap(),
			);

			settled_as(&root, conflicts, theirs);
			let folder = if theirs { there } else { here };
			assert_eq!(placed(&root, seeded).0, Some(folder));
		}
	}

	#[test]
	fn a_move_that_closes_a_cycle_settles_either_way() {
		for theirs in [false, true] {
			let (_parent, root, _, here, _) = novel();
			let folder = |name: &str| {
				create_folder(root.clone(), here, name.to_owned(), None)
					.unwrap()
					.id()
			};
			let (f, g) = (folder("F"), folder("G"));
			let inside = create_document(root.clone(), f, "Inside".to_owned())
				.unwrap()
				.id;
			let conflicts = diverged(
				&root,
				|| move_node(root.clone(), g, f, 0).unwrap(),
				|| move_node(root.clone(), f, g, 0).unwrap(),
			);
			assert_eq!(
				conflicts,
				[merge::Conflict::Moved {
					id: g,
					ours: Some(here),
					theirs: Some(f),
				}]
			);

			settled_as(&root, conflicts, theirs);
			let (outer, inner) = if theirs { (f, g) } else { (g, f) };
			assert_eq!(placed(&root, outer).0, Some(here));
			assert_eq!(placed(&root, inner).0, Some(outer));
			assert_eq!(placed(&root, inside).0, Some(f));
		}
	}

	#[test]
	fn a_deletion_against_an_edit_settles_either_way() {
		for theirs in [false, true] {
			let (_parent, root, seeded, here, _) = novel();
			let other = create_document(root.clone(), here, "Other".to_owned())
				.unwrap()
				.id;
			let conflicts = diverged(
				&root,
				|| {
					delete_document(root.clone(), seeded).unwrap();
					write_document(root.clone(), other, "Theirs".to_owned()).unwrap();
				},
				|| {
					write_document(root.clone(), seeded, "Ours".to_owned()).unwrap();
					delete_document(root.clone(), other).unwrap();
				},
			);

			settled_as(&root, conflicts, theirs);
			let nodes = read_manifest(&root).unwrap().nodes;
			assert_eq!(tree::find(&nodes, seeded).is_some(), !theirs);
			assert_eq!(tree::find(&nodes, other).is_some(), theirs);
			match theirs {
				true => assert_eq!(body_now(&root, other), "Theirs"),
				false => assert_eq!(body_now(&root, seeded), "Ours"),
			}
		}
	}

	#[test]
	fn a_deleted_folder_goes_or_comes_back_whole() {
		for here_deleted in [false, true] {
			for theirs in [false, true] {
				let (_parent, root, _, here, _) = novel();
				let (folder, [a, b, c]) = three_in(&root, here);
				let delete = || delete_folder(root.clone(), folder).unwrap();
				let edit = || write_document(root.clone(), a, "Edited".to_owned()).unwrap();
				let conflicts = match here_deleted {
					true => diverged(&root, edit, delete),
					false => diverged(&root, delete, edit),
				};
				assert_eq!(
					conflicts,
					[merge::Conflict::Deleted {
						id: folder,
						here: here_deleted,
						name: "Scenes".to_owned(),
						documents: 3,
					}]
				);

				settled_as(&root, conflicts, theirs);
				// Taking the other side undoes this version's deletion, or
				// takes the other's.
				let kept = here_deleted == theirs;
				let nodes = read_manifest(&root).unwrap().nodes;
				for id in [folder, a, b, c] {
					assert_eq!(tree::find(&nodes, id).is_some(), kept);
				}
				if kept {
					assert_eq!(body_now(&root, a), "Edited");
				}
			}
		}
	}

	#[test]
	fn a_reorder_on_both_settles_either_way() {
		for theirs in [false, true] {
			let (_parent, root, _, here, _) = novel();
			let (folder, [a, b, c]) = three_in(&root, here);
			let conflicts = diverged(
				&root,
				|| move_node(root.clone(), c, folder, 0).unwrap(),
				|| move_node(root.clone(), a, folder, 2).unwrap(),
			);

			settled_as(&root, conflicts, theirs);
			let order = if theirs { [c, a, b] } else { [b, c, a] };
			assert_eq!(children_of(&root, folder), order);
		}
	}

	#[test]
	fn a_field_on_both_settles_either_way_whatever_the_number_looks_like() {
		for theirs in [false, true] {
			let (_parent, root, seeded, ..) = novel();
			let target = |words: u32| {
				let mut manifest = read_manifest(&root).unwrap();
				if let Some(Node::Document { target, .. }) =
					tree::find_mut(&mut manifest.nodes, seeded)
				{
					*target = Some(words);
				}
				write_manifest(&root, &mut manifest).unwrap();
			};
			let mut conflicts = diverged(&root, || target(100), || target(200));
			// The frontend can send a number back as a float.
			if let [merge::Conflict::Field { theirs, .. }] = conflicts.as_mut_slice() {
				*theirs = serde_json::json!(100.0);
			}

			settled_as(&root, conflicts, theirs);
			let manifest = read_manifest(&root).unwrap();
			let words = if theirs { 100 } else { 200 };
			assert_eq!(manifest.documents()[0].target, Some(words));
		}
	}

	#[test]
	fn the_book_and_a_cover_of_another_kind_settle_either_way() {
		for theirs in [false, true] {
			let (_parent, root, ..) = novel();
			let change = |title: &str, file: &str, image: &[u8]| {
				let source = root.parent().unwrap().join(file);
				fs::write(&source, image).unwrap();
				set_cover(root.clone(), source).unwrap();
				let mut book = read_manifest(&root).unwrap().book;
				book.title = title.to_owned();
				write_book(root.clone(), book).unwrap();
			};
			let conflicts = diverged(
				&root,
				|| change("Theirs", "cover.jpg", b"\xff\xd8\xff\xe0theirs"),
				|| change("Ours", "cover.png", b"\x89PNG\r\n\x1a\nours"),
			);
			assert!(conflicts.contains(&merge::Conflict::Cover {
				ours: Some("cover.png".to_owned()),
				theirs: Some("cover.jpg".to_owned()),
			}));
			assert!(side_cover(&root, false).unwrap().ends_with(b"ours"));
			assert!(side_cover(&root, true).unwrap().ends_with(b"theirs"));

			settled_as(&root, conflicts, theirs);
			let book = read_manifest(&root).unwrap().book;
			let (title, kept, gone) = match theirs {
				true => ("Theirs", "cover.jpg", "cover.png"),
				false => ("Ours", "cover.png", "cover.jpg"),
			};
			assert_eq!((book.title.as_str(), book.cover.as_str()), (title, kept));
			assert!(root.join(kept).is_file());
			assert!(!root.join(gone).exists());
		}
	}

	#[test]
	fn a_checkpoint_finishes_the_merge_once_text_and_binder_are_settled() {
		let (_parent, root, seeded, ..) = novel();
		write_document(root.clone(), seeded, BASE.to_owned()).unwrap();
		let conflicts = diverged(
			&root,
			|| {
				let draft = "One, in Act two.\n\nTwo.\n\nThree.\n";
				write_document(root.clone(), seeded, draft.to_owned()).unwrap();
				rename_document(root.clone(), seeded, "There".to_owned()).unwrap();
			},
			|| {
				write_document(root.clone(), seeded, MAINS.to_owned()).unwrap();
				rename_document(root.clone(), seeded, "Here".to_owned()).unwrap();
			},
		);
		let (text, binder): (Vec<_>, Vec<_>) = conflicts
			.into_iter()
			.partition(|conflict| matches!(conflict, merge::Conflict::Text { .. }));
		assert_eq!(text.len(), 1);

		// The document moves to its new name still conflicted.
		settle_rest(&root, &binder).unwrap();
		assert_eq!(placed(&root, seeded).1, "There.md");
		assert!(spots(&root, seeded).unwrap().is_some());
		waits(&root);

		settle(&root, seeded, "One, settled.\n\nTwo.\n\nThree.\n").unwrap();
		checkpoint(&root, "Merged Act two", "").unwrap();
		assert_eq!(parents(&root).len(), 2);
		assert!(!pending(&root).unwrap());
		assert_eq!(body_now(&root, seeded), "One, settled.\n\nTwo.\n\nThree.\n");
	}
}
