use super::*;

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
pub(super) fn carry(root: &Path, row: Row, staging: bool) -> Result<()> {
	let repo = Repository::open(root)?;
	refuse_while_merging(&repo)?;
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
			// A document moved out before the folder went has its move carried
			// too, or the folder would take it along.
			None => {
				let held = tree::remove(&mut is.nodes, id).map_or_else(Vec::new, |node| {
					tree::documents(std::slice::from_ref(&node))
				});
				for document in held {
					if let Some(path) = tree::path(&from.nodes, document.id) {
						take(&from.nodes, &mut is.nodes, document.id);
						if let Some(blob) = blob_at(&from_tree, &path) {
							blobs.insert(document.id, blob);
						}
					}
				}
			}
			Some(path) => {
				take(&from.nodes, &mut is.nodes, id);
				if let Some(blob) = blob_at(&from_tree, &path) {
					blobs.insert(id, blob);
				}
			}
		},
	}

	// Half of a swap of names would put two nodes on one path.
	let mut placed = Vec::new();
	place(&is.nodes, None, "", &mut placed);
	let mut paths = HashSet::new();
	if !placed
		.iter()
		.all(|(_, node)| paths.insert(node.path.clone()))
	{
		return Err(Error::NameStaged);
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

	// A document's take files go where its node goes, so the two never part:
	// the row's own document, and any a folder's row took out with it.
	if let Row::Node { id } = row {
		let staying: HashSet<Uuid> = is.documents().iter().map(|doc| doc.id).collect();
		let gone = was.documents().into_iter().map(|doc| doc.id);
		for document in gone.filter(|doc| !staying.contains(doc)).chain([id]) {
			old.extend(take_files(&repo, &was_tree, document));
			if tree::find(&from.nodes, document).is_some() {
				new.extend(take_files(&repo, &from_tree, document));
			}
		}
	}

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
pub(super) fn blobs_of(tree: &Tree, manifest: &Manifest) -> HashMap<Uuid, Oid> {
	manifest
		.documents()
		.iter()
		.filter_map(|document| Some((document.id, blob_at(tree, &document.path)?)))
		.collect()
}

/// An index entry with no file stats, so the next look at the folder reads
/// the file itself to tell whether it matches.
pub(super) fn entry(path: &str, id: Oid, size: usize) -> IndexEntry {
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
pub(super) fn take(from: &[Node], into: &mut Vec<Node>, id: Uuid) {
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
	// A folder that holds this node in `from` but sits inside it in `into`
	// comes out first, or removing the node below would take it along.
	for folder in above {
		let inside = tree::trail(into, folder.id())
			.is_some_and(|trail| trail.iter().any(|node| node.id() == id));
		if inside {
			take(from, into, folder.id());
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
pub(super) fn insert(from: &[Node], into: &mut Vec<Node>, node: Node) {
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
pub(super) fn follow(children: &mut [Node], order: &[Node]) {
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
