//! Merging two versions' manifests by node id, against the checkpoint both
//! grew from. This decides where every document and folder goes, what it is
//! called, and what the book says. Text is merged in `versions`.

use std::collections::{BTreeSet, HashMap, HashSet};

use git2::Oid;
use serde::Serialize;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::project::{Book, Manifest};
use crate::tree::Node;

/// Something both versions changed differently. Until the writer chooses,
/// the merge keeps what the version they are in has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum Conflict {
	/// The same paragraph of a document. No id for a file no manifest names.
	Text {
		id: Option<Uuid>,
		path: String,
	},
	Renamed {
		id: Uuid,
		ours: String,
		theirs: String,
	},
	/// Each side's folder, none being the top level.
	Moved {
		id: Uuid,
		ours: Option<Uuid>,
		theirs: Option<Uuid>,
	},
	/// Deleted in one version and changed in the other, along with everything
	/// under it. `here` is true when the version the writer is in deleted it.
	Deleted {
		id: Uuid,
		here: bool,
	},
	/// A folder's order as each side has it, none being the top level.
	Reordered {
		id: Option<Uuid>,
		ours: Vec<Uuid>,
		theirs: Vec<Uuid>,
	},
	/// One field of a node, or of the book when there is no id. A side
	/// without the field has null.
	Field {
		id: Option<Uuid>,
		field: String,
		#[ts(type = "unknown")]
		ours: Value,
		#[ts(type = "unknown")]
		theirs: Value,
	},
	Cover,
}

/// One version: its manifest, the blob of each document, and its cover image.
pub struct Side {
	pub manifest: Manifest,
	pub blobs: HashMap<Uuid, Oid>,
	pub cover: Option<Oid>,
}

pub struct Merged {
	pub manifest: Manifest,
	/// The image the merged book names as its cover.
	pub cover: Option<Oid>,
	pub conflicts: Vec<Conflict>,
}

/// The merged value, or none when both sides changed it differently.
pub(crate) fn three_way<T: PartialEq + Clone>(base: &T, ours: &T, theirs: &T) -> Option<T> {
	if ours == theirs || theirs == base {
		Some(ours.clone())
	} else if ours == base {
		Some(theirs.clone())
	} else {
		None
	}
}

/// A tree taken apart: each node without its children, under its parent,
/// and each folder's order. The top level is the folder `None`.
struct Flat<'a> {
	side: &'a Side,
	nodes: HashMap<Uuid, (Option<Uuid>, Node)>,
	orders: HashMap<Option<Uuid>, Vec<Uuid>>,
	/// Every id, each folder before what it holds.
	walk: Vec<Uuid>,
}

fn flatten(side: &Side) -> Flat<'_> {
	fn go(nodes: &[Node], parent: Option<Uuid>, flat: &mut Flat) {
		flat.orders
			.insert(parent, nodes.iter().map(Node::id).collect());
		for node in nodes {
			let mut bare = node.clone();
			if let Node::Folder { children, .. } = &mut bare {
				children.clear();
			}
			flat.nodes.insert(node.id(), (parent, bare));
			flat.walk.push(node.id());
			if let Node::Folder { children, .. } = node {
				go(children, Some(node.id()), flat);
			}
		}
	}
	let mut flat = Flat {
		side,
		nodes: HashMap::new(),
		orders: HashMap::new(),
		walk: Vec::new(),
	};
	go(&side.manifest.nodes, None, &mut flat);
	flat
}

/// Whether `side` changed the node, or anything under it, since `base`.
fn changed(id: Uuid, base: &Flat, side: &Flat) -> bool {
	let (Some(then), Some(now)) = (base.nodes.get(&id), side.nodes.get(&id)) else {
		return true;
	};
	if then != now || base.side.blobs.get(&id) != side.side.blobs.get(&id) {
		return true;
	}
	let order = side.orders.get(&Some(id));
	order != base.orders.get(&Some(id))
		|| order
			.into_iter()
			.flatten()
			.any(|child| changed(*child, base, side))
}

/// A node's details as JSON: everything but its id, name and children, then
/// its own fields apart, so each merges key by key.
fn details(node: &Node) -> (Map<String, Value>, Map<String, Value>) {
	let mut map = match serde_json::to_value(node) {
		Ok(Value::Object(map)) => map,
		_ => Map::new(),
	};
	let fields = match map.remove("fields") {
		Some(Value::Object(fields)) => fields,
		_ => Map::new(),
	};
	for key in ["node", "id", "name", "children"] {
		map.remove(key);
	}
	(map, fields)
}

fn merge_map(
	id: Option<Uuid>,
	base: &Map<String, Value>,
	ours: &Map<String, Value>,
	theirs: &Map<String, Value>,
	conflicts: &mut Vec<Conflict>,
) -> Map<String, Value> {
	let keys: BTreeSet<&String> = base
		.keys()
		.chain(ours.keys())
		.chain(theirs.keys())
		.collect();
	let mut merged = Map::new();
	for key in keys {
		let (b, o, t) = (base.get(key), ours.get(key), theirs.get(key));
		let value = three_way(&b, &o, &t).unwrap_or_else(|| {
			conflicts.push(Conflict::Field {
				id,
				field: key.clone(),
				ours: o.cloned().unwrap_or(Value::Null),
				theirs: t.cloned().unwrap_or(Value::Null),
			});
			o
		});
		if let Some(value) = value {
			merged.insert(key.clone(), value.clone());
		}
	}
	merged
}

fn renamed(node: &mut Node, to: String) {
	match node {
		Node::Folder { name, .. } | Node::Document { name, .. } => *name = to,
	}
}

/// A node present in all three: name, folder and details each merge apart.
fn merge_node(
	id: Uuid,
	(base_parent, base): &(Option<Uuid>, Node),
	(our_parent, ours): &(Option<Uuid>, Node),
	(their_parent, theirs): &(Option<Uuid>, Node),
	conflicts: &mut Vec<Conflict>,
) -> (Option<Uuid>, Node) {
	let name = three_way(&base.name(), &ours.name(), &theirs.name()).unwrap_or_else(|| {
		conflicts.push(Conflict::Renamed {
			id,
			ours: ours.name().to_owned(),
			theirs: theirs.name().to_owned(),
		});
		ours.name()
	});
	let parent = three_way(base_parent, our_parent, their_parent).unwrap_or_else(|| {
		conflicts.push(Conflict::Moved {
			id,
			ours: *our_parent,
			theirs: *their_parent,
		});
		*our_parent
	});

	let ((base, base_fields), (here, here_fields), (there, there_fields)) =
		(details(base), details(ours), details(theirs));
	let mut map = match serde_json::to_value(ours) {
		Ok(Value::Object(map)) => map,
		_ => Map::new(),
	};
	for key in here.keys().chain(there.keys()) {
		map.remove(key);
	}
	map.extend(merge_map(Some(id), &base, &here, &there, conflicts));
	map.remove("fields");
	let fields = merge_map(
		Some(id),
		&base_fields,
		&here_fields,
		&there_fields,
		conflicts,
	);
	if !fields.is_empty() {
		map.insert("fields".to_owned(), Value::Object(fields));
	}
	let mut node: Node =
		serde_json::from_value(Value::Object(map)).unwrap_or_else(|_| ours.clone());
	renamed(&mut node, name.to_owned());
	(parent, node)
}

/// Whether following parents up from `id` comes back to it.
fn in_cycle(id: Uuid, parents: &HashMap<Uuid, Option<Uuid>>) -> bool {
	let mut at = parents.get(&id).copied().flatten();
	for _ in 0..parents.len() {
		match at {
			None => return false,
			Some(parent) if parent == id => return true,
			Some(parent) => at = parents.get(&parent).copied().flatten(),
		}
	}
	false
}

struct Merge<'a> {
	base: Flat<'a>,
	ours: Flat<'a>,
	theirs: Flat<'a>,
	parents: HashMap<Uuid, Option<Uuid>>,
	nodes: HashMap<Uuid, Node>,
	conflicts: Vec<Conflict>,
}

impl Merge<'_> {
	/// The folder's merged order. One side's reordering wins over the other
	/// leaving it alone; both reordering differently is a conflict.
	fn order(&mut self, folder: Option<Uuid>) -> Vec<Uuid> {
		let members: HashSet<Uuid> = self
			.parents
			.iter()
			.filter(|(_, parent)| **parent == folder)
			.map(|(id, _)| *id)
			.collect();
		let common = |a: &[Uuid], b: &[Uuid]| -> Vec<Uuid> {
			a.iter().filter(|id| b.contains(id)).copied().collect()
		};
		let base = self.base.orders.get(&folder);
		let reordered =
			|side: &[Uuid]| base.is_some_and(|base| common(side, base) != common(base, side));
		let (first, second) = match (
			self.ours.orders.get(&folder),
			self.theirs.orders.get(&folder),
		) {
			(Some(here), Some(there)) => {
				let (moved_here, moved_there) = (reordered(here), reordered(there));
				if moved_here && moved_there && common(here, there) != common(there, here) {
					self.conflicts.push(Conflict::Reordered {
						id: folder,
						ours: here.clone(),
						theirs: there.clone(),
					});
				}
				match moved_there && !moved_here {
					true => (there.clone(), here.clone()),
					false => (here.clone(), there.clone()),
				}
			}
			(Some(only), None) | (None, Some(only)) => (only.clone(), Vec::new()),
			(None, None) => (Vec::new(), Vec::new()),
		};

		let mut placed: Vec<Uuid> = first
			.into_iter()
			.filter(|id| members.contains(id))
			.collect();
		// ponytail: quadratic, fine for a folder's children.
		for (at, id) in second.iter().enumerate() {
			if !members.contains(id) || placed.contains(id) {
				continue;
			}
			let after = second[..at]
				.iter()
				.rev()
				.find_map(|before| placed.iter().position(|p| p == before));
			placed.insert(after.map_or(0, |i| i + 1), *id);
		}
		placed
	}

	/// The folder's children, in order, with what they hold. A name the folder
	/// already has gets a number, so two documents never share a file.
	fn build(&mut self, folder: Option<Uuid>) -> Vec<Node> {
		let mut taken = HashSet::new();
		let mut built = Vec::new();
		for id in self.order(folder) {
			let Some(mut node) = self.nodes.remove(&id) else {
				continue;
			};
			if !taken.insert(node.name().to_lowercase()) {
				let name = node.name().to_owned();
				let (stem, extension) = match (&node, name.rfind('.')) {
					(Node::Document { .. }, Some(dot)) => name.split_at(dot),
					_ => (name.as_str(), ""),
				};
				let free = (2..)
					.map(|n| format!("{stem} {n}{extension}"))
					.find(|candidate| !taken.contains(&candidate.to_lowercase()))
					.unwrap_or_default();
				taken.insert(free.to_lowercase());
				renamed(&mut node, free);
			}
			if let Node::Folder { children, .. } = &mut node {
				*children = self.build(Some(id));
			}
			built.push(node);
		}
		built
	}
}

/// Merges `ours` and `theirs` against `base`. A side that changed something
/// the other left alone wins; where both changed it differently, ours stays
/// and the conflict is reported.
pub fn manifests(base: &Side, ours: &Side, theirs: &Side) -> Merged {
	let mut merge = Merge {
		base: flatten(base),
		ours: flatten(ours),
		theirs: flatten(theirs),
		parents: HashMap::new(),
		nodes: HashMap::new(),
		conflicts: Vec::new(),
	};
	// Folders deleted on one side and kept, or left deleted, along with what
	// they hold, because the other side changed them.
	let mut kept = HashSet::new();
	let mut gone = HashSet::new();

	let ids: Vec<Uuid> = merge
		.ours
		.walk
		.iter()
		.chain(
			merge
				.theirs
				.walk
				.iter()
				.filter(|id| !merge.ours.nodes.contains_key(id)),
		)
		.copied()
		.collect();
	for id in ids {
		let base = merge.base.nodes.get(&id);
		let placed = match (base, merge.ours.nodes.get(&id), merge.theirs.nodes.get(&id)) {
			(Some(base), Some(here), Some(there)) => {
				Some(merge_node(id, base, here, there, &mut merge.conflicts))
			}
			// ponytail: the same id added on both sides keeps ours whole.
			(None, Some(here), _) => Some(here.clone()),
			(None, None, Some(there)) => Some(there.clone()),
			(Some(_), Some(here), None) => {
				if kept.contains(&here.0) {
					kept.insert(Some(id));
					Some(here.clone())
				} else if changed(id, &merge.base, &merge.ours) {
					merge.conflicts.push(Conflict::Deleted { id, here: false });
					kept.insert(Some(id));
					Some(here.clone())
				} else {
					None
				}
			}
			(Some(_), None, Some(there)) => {
				if gone.contains(&there.0) {
					gone.insert(Some(id));
				} else if changed(id, &merge.base, &merge.theirs) {
					merge.conflicts.push(Conflict::Deleted { id, here: true });
					gone.insert(Some(id));
				}
				None
			}
			(_, None, None) => None,
		};
		if let Some((parent, node)) = placed {
			merge.parents.insert(id, parent);
			merge.nodes.insert(id, node);
		}
	}

	// Neither side has a cycle, but ours moving A into B while theirs moves B
	// into A would make one. Ours wins for the node that closes it.
	let ids: Vec<Uuid> = merge.ours.walk.clone();
	for id in ids {
		let ours = merge.ours.nodes[&id].0;
		let theirs = merge.parents.get(&id).copied().flatten();
		if theirs != ours && in_cycle(id, &merge.parents) {
			merge.conflicts.push(Conflict::Moved { id, ours, theirs });
			merge.parents.insert(id, ours);
		}
	}

	// Anything whose folder did not survive is left out with it.
	let nodes = merge.build(None);

	let book = |side: &Side| match serde_json::to_value(&side.manifest.book) {
		Ok(Value::Object(mut map)) => {
			map.remove("cover");
			map
		}
		_ => Map::new(),
	};
	let mut details = merge_map(
		None,
		&book(base),
		&book(ours),
		&book(theirs),
		&mut merge.conflicts,
	);
	let cover = |side: &Side| (side.manifest.book.cover.clone(), side.cover);
	let (cover_name, cover) =
		three_way(&cover(base), &cover(ours), &cover(theirs)).unwrap_or_else(|| {
			merge.conflicts.push(Conflict::Cover);
			cover(ours)
		});
	details.insert("cover".to_owned(), Value::String(cover_name));
	let book: Book = serde_json::from_value(Value::Object(details))
		.unwrap_or_else(|_| ours.manifest.book.clone());

	// ponytail: the project's name keeps ours when both renamed it.
	let name = three_way(
		&base.manifest.name,
		&ours.manifest.name,
		&theirs.manifest.name,
	)
	.unwrap_or_else(|| ours.manifest.name.clone());
	Merged {
		manifest: Manifest {
			name,
			book,
			nodes,
			..ours.manifest.clone()
		},
		cover,
		conflicts: merge.conflicts,
	}
}
