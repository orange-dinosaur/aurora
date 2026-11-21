use super::*;

/// Counted the way the rest of Aurora counts a document: its body only.
pub(super) fn words_in(bytes: &[u8]) -> u32 {
	let text = String::from_utf8_lossy(bytes);
	crate::document::words_of(body(&text)) as u32
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
	/// A document whose takes differ, in their details or their files, which
	/// can be the whole of what changed. Never set on a folder.
	pub takes: bool,
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
pub(super) struct Placed {
	pub(super) folder: bool,
	name: String,
	parent: Option<Uuid>,
	pub(super) path: String,
	children: Vec<Uuid>,
	/// A document's takes and which of them is chosen.
	takes: (BTreeMap<Uuid, Take>, Option<Uuid>),
}

pub(super) fn id_of(node: &Node) -> Uuid {
	match node {
		Node::Folder { id, .. } | Node::Document { id, .. } => *id,
	}
}

pub(super) fn place(
	nodes: &[Node],
	parent: Option<Uuid>,
	prefix: &str,
	placed: &mut Vec<(Uuid, Placed)>,
) {
	for node in nodes {
		let (name, folder, children, takes) = match node {
			Node::Folder { name, children, .. } => (
				name,
				true,
				children.iter().map(id_of).collect(),
				Default::default(),
			),
			Node::Document {
				name,
				takes,
				chosen,
				..
			} => (name, false, Vec::new(), (takes.clone(), *chosen)),
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
				takes,
			},
		));
		if let Node::Folder { children, .. } = node {
			place(children, Some(id), &format!("{path}/"), placed);
		}
	}
}

/// A version's tree and manifest. `HEAD` is the latest checkpoint, and with no
/// version, the project as it stands on disk, neither kept nor staged.
pub(super) fn side<'r>(
	repo: &'r Repository,
	version: Option<&str>,
) -> Result<(Tree<'r>, Manifest)> {
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
pub(super) fn staged_side(repo: &Repository) -> Result<(Tree<'_>, Manifest)> {
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
pub(super) fn with_manifest<'r>(
	repo: &'r Repository,
	tree: Option<Tree<'r>>,
) -> Result<(Tree<'r>, Manifest)> {
	let nothing = || {
		let mut nothing = Manifest {
			nodes: Vec::new(),
			..Manifest::new("", Format::Novel, OffsetDateTime::UNIX_EPOCH)
		};
		// Otherwise random, and nothing must equal nothing.
		nothing.book.identifier = Uuid::nil();
		nothing
	};
	let Some(tree) = tree else {
		let empty = repo.find_tree(repo.treebuilder(None)?.write()?)?;
		return Ok((empty, nothing()));
	};
	// A commit from before the project was an Aurora project holds no binder.
	let Ok(manifest) = tree.get_path(Path::new(MANIFEST_FILE)) else {
		return Ok((tree, nothing()));
	};
	let manifest = repo.find_blob(manifest.id())?;
	let manifest = serde_json::from_slice(manifest.content())?;
	Ok((tree, manifest))
}

/// Which children changed places, each with the child it now follows. The
/// longest run still in its old order stays put and everything else moved, so
/// dragging one child names only that child. What was added, deleted or moved
/// to another folder is reported on its own.
pub(super) fn reordered(then: &[Uuid], now: &[Uuid]) -> Vec<(Uuid, Option<Uuid>)> {
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

pub(super) fn blob_at(tree: &Tree, path: &str) -> Option<Oid> {
	tree.get_path(Path::new(path)).ok().map(|entry| entry.id())
}

pub(super) fn takes_folder(id: Uuid) -> String {
	format!("{TAKES_DIR}/{id}")
}

/// The files of a document's takes in a tree, by path.
pub(super) fn take_files(repo: &Repository, tree: &Tree, id: Uuid) -> HashMap<String, Oid> {
	let folder = takes_folder(id);
	let Some(files) = blob_at(tree, &folder).and_then(|oid| repo.find_tree(oid).ok()) else {
		return HashMap::new();
	};
	files
		.iter()
		.filter_map(|file| Some((format!("{folder}/{}", file.name().ok()?), file.id())))
		.collect()
}

/// Opens the repository for a look at the project as it stands. The trees and
/// blobs that comparing builds go to memory, so each look leaves no loose
/// objects behind in `.git`.
pub(super) fn reading(root: &Path) -> Result<Repository> {
	let repo = Repository::open(root)?;
	repo.odb()?.add_new_mempack_backend(1000)?;
	Ok(repo)
}

/// What changed from version `a` to version `b`, where none is the project as
/// it stands on disk and `HEAD` is the latest checkpoint.
pub fn changes(root: &Path, a: Option<&str>, b: Option<&str>) -> Result<Changes> {
	let repo = reading(root)?;
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
	let repo = reading(root)?;
	let staged = staged_side(&repo)?;
	Ok(Status {
		staged: compare(&side(&repo, Some("HEAD"))?, &staged),
		unstaged: compare(&staged, &side(&repo, None)?),
	})
}

pub(super) fn compare(
	(old_tree, old): &(Tree, Manifest),
	(new_tree, new): &(Tree, Manifest),
) -> Changes {
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
			takes: !now.folder
				&& then.is_some_and(|then| {
					let folder = takes_folder(*id);
					then.takes != now.takes
						|| blob_at(old_tree, &folder) != blob_at(new_tree, &folder)
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
			|| change.takes
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
			takes: false,
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
