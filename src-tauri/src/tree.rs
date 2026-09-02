//! A project's contents as a tree, and the walks over it.
//!
//! A tree is a `Vec<Node>`: the top level holds the project's sections, and
//! every level below it is one ordered list of folders and documents. That
//! order is compile order, so walking the array in order is the order a reader
//! would meet the text in.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::document::Document;

/// What a folder inside the Manuscript is. A folder anywhere else has no kind:
/// it is a folder with a name and nothing more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FolderKind {
	Part,
	Chapter,
}

/// One entry in a project's tree.
///
/// Folders and documents share a single ordered list at each level rather than
/// being kept in separate ones, because that order is what a compile walks: a
/// part and a loose chapter sitting side by side need a sequence between them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "camelCase")]
pub enum Node {
	Folder {
		id: Uuid,
		/// The directory's name on disk. A node's path is walked to, never
		/// stored, so renaming a folder is this one field.
		name: String,
		/// Absent outside the Manuscript.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		kind: Option<FolderKind>,
		#[serde(default)]
		children: Vec<Node>,
	},
	Document {
		id: Uuid,
		/// The file's name on disk, extension included. The title is this
		/// without the extension, as it has always been.
		name: String,
		/// How many words the writer is aiming at, if they have said.
		#[serde(default, skip_serializing_if = "Option::is_none")]
		target: Option<u32>,
	},
}

impl Node {
	fn folder(name: &str) -> Self {
		Node::Folder {
			id: Uuid::new_v4(),
			name: name.to_owned(),
			kind: None,
			children: Vec::new(),
		}
	}

	/// A node's id, whichever kind it is. Folder and document ids are both v4
	/// UUIDs, so one lookup serves the pair of them.
	pub fn id(&self) -> Uuid {
		match self {
			Node::Folder { id, .. } | Node::Document { id, .. } => *id,
		}
	}

	/// The node's name on disk, a document's extension included.
	pub fn name(&self) -> &str {
		match self {
			Node::Folder { name, .. } | Node::Document { name, .. } => name,
		}
	}
}

/// Builds the tree a version 2 manifest describes: its folders in the order it
/// declares them, each document under the folder its path names.
///
/// Document ids carry over untouched, so anything holding one still finds its
/// document. Folders had no identity before this, so theirs are new on every
/// read until the tree is written back.
pub fn tree_from_flat(folders: &[String], documents: &[Document]) -> Vec<Node> {
	let mut nodes: Vec<Node> = folders.iter().map(|name| Node::folder(name)).collect();

	for document in documents {
		let mut segments: Vec<&str> = document
			.path
			.split('/')
			.filter(|segment| !segment.is_empty())
			.collect();

		// A path with nothing in it names no file, so there is nothing to
		// carry over. `refresh` drops it on the next scan either way.
		let Some(file) = segments.pop() else {
			continue;
		};

		let mut level = &mut nodes;
		for folder in segments {
			let found = level
				.iter()
				.position(|node| matches!(node, Node::Folder { name, .. } if name == folder));
			// A version 2 manifest is only supposed to hold `folder/file.md`
			// under a folder it declares, but a document is the writer's and a
			// missing folder is not a reason to lose one.
			let at = match found {
				Some(at) => at,
				None => {
					level.push(Node::folder(folder));
					level.len() - 1
				}
			};
			let Node::Folder { children, .. } = &mut level[at] else {
				unreachable!("the index came from a folder")
			};
			level = children;
		}

		level.push(Node::Document {
			id: document.id,
			name: file.to_owned(),
			target: document.target,
		});
	}

	nodes
}

/// Every node in the tree, depth first: a folder, then all it holds, then
/// whatever follows it. This is compile order.
pub fn walk(nodes: &[Node]) -> Walk<'_> {
	Walk {
		levels: vec![nodes.iter()],
	}
}

/// The iterator [`walk`] returns.
pub struct Walk<'a> {
	/// One iterator per level currently open, the deepest last.
	levels: Vec<std::slice::Iter<'a, Node>>,
}

impl<'a> Iterator for Walk<'a> {
	type Item = &'a Node;

	fn next(&mut self) -> Option<&'a Node> {
		loop {
			match self.levels.last_mut()?.next() {
				Some(node) => {
					if let Node::Folder { children, .. } = node {
						self.levels.push(children.iter());
					}
					return Some(node);
				}
				// This level is spent, so carry on with the one above.
				None => {
					self.levels.pop();
				}
			}
		}
	}
}

/// The node with this id, folder or document.
pub fn find(nodes: &[Node], id: Uuid) -> Option<&Node> {
	walk(nodes).find(|node| node.id() == id)
}

/// The chain from the top level down to `id`, the node itself last. `None` if
/// nothing in the tree carries that id.
pub fn trail(nodes: &[Node], id: Uuid) -> Option<Vec<&Node>> {
	for node in nodes {
		if node.id() == id {
			return Some(vec![node]);
		}
		let Node::Folder { children, .. } = node else {
			continue;
		};
		if let Some(mut chain) = trail(children, id) {
			chain.insert(0, node);
			return Some(chain);
		}
	}
	None
}

/// The folder holding this node. `None` when it sits at the top level, and
/// when the tree does not have it at all.
pub fn parent(nodes: &[Node], id: Uuid) -> Option<&Node> {
	let chain = trail(nodes, id)?;
	chain.len().checked_sub(2).map(|above| chain[above])
}

/// Where a node lives, relative to the project root: the folders above it and
/// its own name, joined the way a manifest has always spelled a document path.
pub fn path(nodes: &[Node], id: Uuid) -> Option<String> {
	let chain = trail(nodes, id)?;
	Some(
		chain
			.iter()
			.map(|node| node.name())
			.collect::<Vec<_>>()
			.join("/"),
	)
}

/// What a folder holds, in order. `None` for a document and for an id the tree
/// does not have; an empty slice for a folder holding nothing.
pub fn children(nodes: &[Node], id: Uuid) -> Option<&[Node]> {
	match find(nodes, id)? {
		Node::Folder { children, .. } => Some(children),
		Node::Document { .. } => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn flat(path: &str, target: Option<u32>) -> Document {
		Document {
			id: Uuid::new_v4(),
			path: path.to_owned(),
			target,
		}
	}
	fn names_of(nodes: &[Node]) -> Vec<&str> {
		nodes
			.iter()
			.map(|node| match node {
				Node::Folder { name, .. } | Node::Document { name, .. } => name.as_str(),
			})
			.collect()
	}

	fn children_of<'a>(nodes: &'a [Node], folder: &str) -> &'a [Node] {
		nodes
			.iter()
			.find_map(|node| match node {
				Node::Folder { name, children, .. } if name == folder => Some(children.as_slice()),
				_ => None,
			})
			.unwrap_or_else(|| panic!("no folder called {folder}"))
	}

	#[test]
	fn documents_land_under_the_folder_their_path_names() {
		let folders = vec!["Manuscript".to_owned(), "Notes".to_owned()];
		let documents = vec![
			flat("Manuscript/Chapter 1.md", Some(2000)),
			flat("Manuscript/Chapter 2.md", None),
			flat("Notes/Notes.md", None),
		];

		let tree = tree_from_flat(&folders, &documents);

		assert_eq!(names_of(&tree), ["Manuscript", "Notes"]);
		assert_eq!(
			names_of(children_of(&tree, "Manuscript")),
			["Chapter 1.md", "Chapter 2.md"],
			"documents keep the order the manifest gave them"
		);
		assert_eq!(names_of(children_of(&tree, "Notes")), ["Notes.md"]);
	}

	#[test]
	fn a_document_keeps_its_id_and_its_target() {
		let folders = vec!["Manuscript".to_owned()];
		let documents = vec![flat("Manuscript/Chapter 1.md", Some(2000))];

		let tree = tree_from_flat(&folders, &documents);

		let Node::Document { id, target, .. } = &children_of(&tree, "Manuscript")[0] else {
			panic!("expected a document")
		};
		assert_eq!(*id, documents[0].id, "ids carry over so lookups still work");
		assert_eq!(*target, Some(2000));
	}

	#[test]
	fn a_folder_with_no_documents_survives_as_an_empty_one() {
		let folders = vec!["Manuscript".to_owned(), "Characters".to_owned()];
		let documents = vec![flat("Manuscript/Chapter 1.md", None)];

		let tree = tree_from_flat(&folders, &documents);

		assert_eq!(names_of(&tree), ["Manuscript", "Characters"]);
		assert!(children_of(&tree, "Characters").is_empty());
	}

	#[test]
	fn a_document_naming_an_undeclared_folder_is_not_lost() {
		let folders = vec!["Manuscript".to_owned()];
		let documents = vec![flat("Elsewhere/Thing.md", None)];

		let tree = tree_from_flat(&folders, &documents);

		assert_eq!(names_of(&tree), ["Manuscript", "Elsewhere"]);
		assert_eq!(names_of(children_of(&tree, "Elsewhere")), ["Thing.md"]);
	}

	#[test]
	fn a_nested_path_creates_the_folders_it_names() {
		let folders = vec!["Manuscript".to_owned()];
		let documents = vec![
			flat("Manuscript/Part One/Chapter 3/Scene 2.md", None),
			flat("Manuscript/Part One/Chapter 3/Scene 3.md", None),
		];

		let tree = tree_from_flat(&folders, &documents);

		let part = children_of(&tree, "Manuscript");
		assert_eq!(names_of(part), ["Part One"]);
		let chapter = children_of(part, "Part One");
		assert_eq!(names_of(chapter), ["Chapter 3"]);
		assert_eq!(
			names_of(children_of(chapter, "Chapter 3")),
			["Scene 2.md", "Scene 3.md"],
			"a folder is made once, not once per document"
		);
	}

	#[test]
	fn a_path_with_nothing_in_it_is_dropped() {
		let tree = tree_from_flat(&["Manuscript".to_owned()], &[flat("", None)]);

		assert_eq!(names_of(&tree), ["Manuscript"]);
		assert!(children_of(&tree, "Manuscript").is_empty());
	}

	#[test]
	fn a_tree_round_trips_through_json() {
		let tree = vec![Node::Folder {
			id: Uuid::new_v4(),
			name: "Manuscript".to_owned(),
			kind: None,
			children: vec![Node::Folder {
				id: Uuid::new_v4(),
				name: "Part One".to_owned(),
				kind: Some(FolderKind::Part),
				children: vec![Node::Document {
					id: Uuid::new_v4(),
					name: "Chapter 1.md".to_owned(),
					target: None,
				}],
			}],
		}];

		let json = serde_json::to_string(&tree).unwrap();
		assert!(
			json.contains(r#""node":"folder""#) && json.contains(r#""node":"document""#),
			"the tag says which a node is: {json}"
		);
		assert!(json.contains(r#""kind":"part""#), "a part says so: {json}");
		assert!(
			!json.contains(r#""kind":null"#) && !json.contains("target"),
			"a folder with no kind and a document with no target write no key: {json}"
		);
		assert_eq!(
			serde_json::from_str::<Vec<Node>>(&json).unwrap(),
			tree,
			"what is written reads back the same"
		);
	}

	fn document(name: &str) -> Node {
		Node::Document {
			id: Uuid::new_v4(),
			name: name.to_owned(),
			target: None,
		}
	}

	fn folder(name: &str, kind: Option<FolderKind>, children: Vec<Node>) -> Node {
		Node::Folder {
			id: Uuid::new_v4(),
			name: name.to_owned(),
			kind,
			children,
		}
	}

	/// Manuscript
	///   Part One
	///     Chapter 1
	///       Scene 1.md
	///       Scene 2.md
	///     Chapter 2
	///       Scene 3.md
	///   Front Matter.md
	/// Notes
	///   Ideas.md
	fn sample() -> Vec<Node> {
		vec![
			folder(
				"Manuscript",
				None,
				vec![
					folder(
						"Part One",
						Some(FolderKind::Part),
						vec![
							folder(
								"Chapter 1",
								Some(FolderKind::Chapter),
								vec![document("Scene 1.md"), document("Scene 2.md")],
							),
							folder(
								"Chapter 2",
								Some(FolderKind::Chapter),
								vec![document("Scene 3.md")],
							),
						],
					),
					document("Front Matter.md"),
				],
			),
			folder("Notes", None, vec![document("Ideas.md")]),
		]
	}

	fn id_of(nodes: &[Node], name: &str) -> Uuid {
		walk(nodes)
			.find(|node| node.name() == name)
			.unwrap_or_else(|| panic!("no node called {name}"))
			.id()
	}

	#[test]
	fn a_document_is_found_at_depth() {
		let tree = sample();
		let id = id_of(&tree, "Scene 3.md");

		let found = find(&tree, id).expect("the scene is in the tree");

		assert_eq!(found.name(), "Scene 3.md");
		assert_eq!(found.id(), id);
	}

	#[test]
	fn a_folder_answers_the_same_lookup() {
		let tree = sample();
		let id = id_of(&tree, "Chapter 2");

		assert!(
			matches!(find(&tree, id), Some(Node::Folder { .. })),
			"one lookup takes either kind of id"
		);
	}

	#[test]
	fn an_id_the_tree_does_not_hold_is_missing_everywhere() {
		let tree = sample();
		let stranger = Uuid::new_v4();

		assert!(find(&tree, stranger).is_none());
		assert!(trail(&tree, stranger).is_none());
		assert!(parent(&tree, stranger).is_none());
		assert!(path(&tree, stranger).is_none());
		assert!(children(&tree, stranger).is_none());
	}

	#[test]
	fn a_path_is_walked_down_through_three_folders() {
		let tree = sample();

		assert_eq!(
			path(&tree, id_of(&tree, "Scene 2.md")).unwrap(),
			"Manuscript/Part One/Chapter 1/Scene 2.md"
		);
	}

	#[test]
	fn a_top_level_folder_is_the_whole_of_its_own_path() {
		let tree = sample();

		assert_eq!(path(&tree, id_of(&tree, "Notes")).unwrap(), "Notes");
	}

	#[test]
	fn a_parent_is_the_folder_holding_the_node() {
		let tree = sample();

		let above = parent(&tree, id_of(&tree, "Scene 1.md")).expect("the scene sits in a chapter");

		assert_eq!(above.name(), "Chapter 1");
	}

	#[test]
	fn nothing_at_the_top_level_has_a_parent() {
		let tree = sample();

		assert!(parent(&tree, id_of(&tree, "Manuscript")).is_none());
	}

	#[test]
	fn a_folder_lists_what_it_holds_in_order() {
		let tree = sample();

		let held = children(&tree, id_of(&tree, "Manuscript")).expect("a folder holds a list");

		assert_eq!(
			names_of(held),
			["Part One", "Front Matter.md"],
			"a folder and a document sit in one sequence"
		);
	}

	#[test]
	fn an_empty_folder_holds_a_list_with_nothing_in_it() {
		let tree = vec![folder("Characters", None, Vec::new())];

		let held = children(&tree, id_of(&tree, "Characters")).expect("still a folder");

		assert!(held.is_empty());
	}

	#[test]
	fn a_document_holds_nothing() {
		let tree = sample();

		assert!(children(&tree, id_of(&tree, "Front Matter.md")).is_none());
	}

	#[test]
	fn a_walk_reads_the_tree_in_compile_order() {
		let tree = sample();

		assert_eq!(
			walk(&tree).map(Node::name).collect::<Vec<_>>(),
			[
				"Manuscript",
				"Part One",
				"Chapter 1",
				"Scene 1.md",
				"Scene 2.md",
				"Chapter 2",
				"Scene 3.md",
				"Front Matter.md",
				"Notes",
				"Ideas.md"
			],
			"a folder comes before what it holds, and every level keeps its order"
		);
	}

	#[test]
	fn walking_an_empty_tree_yields_nothing() {
		assert_eq!(walk(&[]).count(), 0);
	}
}
