//! The compile walk: the project's tree turned into the shape a book has.
//!
//! Every renderer stands on this. The walk takes the tree the sidebar is drawn
//! from, which already says of each node whether it is in the book, and works
//! out what the story is: what surrounds it, what its divisions are, and which
//! documents make up each one. Nothing here opens a file, so a renderer decides
//! for itself when to read the text a scene points at.

use uuid::Uuid;

use crate::document::{DocumentView, NodeView};
use crate::project::{MANUSCRIPT, matter};
use crate::tree::FolderKind;

/// One document the book takes, with where its text is. The title is the file
/// name without its extension, as it is everywhere else in Aurora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scene {
	pub id: Uuid,
	pub title: String,
	/// Relative to the project root, forward slashes, the way the manifest
	/// keeps it.
	pub path: String,
}

/// A run of scenes under one heading. A chapter written as a single document
/// holds that one scene, so the renderers have one thing to draw either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
	pub id: Uuid,
	pub title: String,
	pub scenes: Vec<Scene>,
}

/// An arc of the story, holding chapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
	pub id: Uuid,
	pub title: String,
	pub chapters: Vec<Chapter>,
}

/// One step through the body of the book. A part and a chapter that is not in
/// one sit side by side in the Manuscript, and the order between them is the
/// writer's, so the body is a single sequence rather than a list of parts with
/// the loose chapters kept somewhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Division {
	Part(Part),
	Chapter(Chapter),
}

/// The whole book, in the order it reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Compiled {
	pub front: Vec<Scene>,
	pub body: Vec<Division>,
	pub back: Vec<Scene>,
}

/// The project tree compiled into a book. `nodes` is the project's sections;
/// everything outside the Manuscript is notes about the story rather than the
/// story, so only the Manuscript is walked.
///
/// Anything the writer has switched off is left out, and so is everything under
/// it: a chapter out of the book takes its scenes with it without their having
/// to be switched off one at a time. A division left with nothing in it drops
/// out too, since a heading over no prose is not something a renderer can do
/// anything sensible with.
pub fn compile(nodes: &[NodeView]) -> Compiled {
	let mut compiled = Compiled::default();
	let Some(manuscript) = manuscript(nodes) else {
		return compiled;
	};

	for node in manuscript.iter().filter(|node| kept(node)) {
		match node {
			// Only here: a matter kind can exist nowhere but directly inside
			// the Manuscript, so nothing deeper down is ever asked.
			NodeView::Folder { kind, children, .. } if matter(*kind) => {
				let matter_scenes = scenes(children);
				if *kind == Some(FolderKind::FrontMatter) {
					compiled.front.extend(matter_scenes);
				} else {
					compiled.back.extend(matter_scenes);
				}
			}
			NodeView::Folder {
				id,
				name,
				kind: Some(FolderKind::Part),
				children,
				..
			} => {
				let chapters = chapters(children);
				if !chapters.is_empty() {
					compiled.body.push(Division::Part(Part {
						id: *id,
						title: name.clone(),
						chapters,
					}));
				}
			}
			// A chapter, whether it is a folder of scenes or the one document
			// that is the whole of it.
			node => {
				if let Some(chapter) = chapter(node) {
					compiled.body.push(Division::Chapter(chapter));
				}
			}
		}
	}

	compiled
}

/// What the Manuscript holds, if the project has one and the writer has not
/// taken it out of the book.
fn manuscript(nodes: &[NodeView]) -> Option<&[NodeView]> {
	nodes.iter().find_map(|node| match node {
		NodeView::Folder {
			name,
			children,
			in_book,
			..
		} if name == MANUSCRIPT && *in_book => Some(children.as_slice()),
		_ => None,
	})
}

/// The chapters of a part, in the order the writer put them in. A folder is a
/// chapter whatever kind it claims to be: inside a part there is nothing else
/// for it to be.
fn chapters(nodes: &[NodeView]) -> Vec<Chapter> {
	nodes
		.iter()
		.filter(|node| kept(node))
		.filter_map(chapter)
		.collect()
}

/// One node as a chapter, or nothing when it is a folder with no scenes left
/// in it.
fn chapter(node: &NodeView) -> Option<Chapter> {
	match node {
		NodeView::Folder {
			id, name, children, ..
		} => {
			let scenes = scenes(children);
			(!scenes.is_empty()).then(|| Chapter {
				id: *id,
				title: name.clone(),
				scenes,
			})
		}
		NodeView::Document { document, .. } => Some(Chapter {
			id: document.id,
			title: document.title.clone(),
			scenes: vec![scene(document)],
		}),
	}
}

/// Every document under `nodes` that is still in the book, however deep it
/// sits, in tree order. A chapter is meant to hold documents, but nothing stops
/// a writer nesting a folder inside one, and a scene the sidebar shows must not
/// vanish from the export because of where it was put.
fn scenes(nodes: &[NodeView]) -> Vec<Scene> {
	let mut found = Vec::new();
	gather(nodes, &mut found);
	found
}

fn gather(nodes: &[NodeView], found: &mut Vec<Scene>) {
	for node in nodes.iter().filter(|node| kept(node)) {
		match node {
			NodeView::Folder { children, .. } => gather(children, found),
			NodeView::Document { document, .. } => found.push(scene(document)),
		}
	}
}

fn scene(document: &DocumentView) -> Scene {
	Scene {
		id: document.id,
		title: document.title.clone(),
		path: document.path.clone(),
	}
}

/// Whether the book takes this node at all.
fn kept(node: &NodeView) -> bool {
	match node {
		NodeView::Folder { in_book, .. } | NodeView::Document { in_book, .. } => *in_book,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::document::Document;

	fn document_node(path: &str, in_book: bool) -> NodeView {
		let document = Document {
			id: Uuid::new_v4(),
			path: path.to_owned(),
			target: None,
		};
		NodeView::Document {
			document: (&document).into(),
			in_book,
		}
	}

	fn folder_node(
		name: &str,
		kind: Option<FolderKind>,
		in_book: bool,
		children: Vec<NodeView>,
	) -> NodeView {
		NodeView::Folder {
			id: Uuid::new_v4(),
			name: name.to_owned(),
			kind,
			children,
			words: 0,
			in_book,
		}
	}

	fn doc(path: &str) -> NodeView {
		document_node(path, true)
	}

	fn off_doc(path: &str) -> NodeView {
		document_node(path, false)
	}

	fn folder(name: &str, children: Vec<NodeView>) -> NodeView {
		folder_node(name, None, true, children)
	}

	fn off_folder(name: &str, children: Vec<NodeView>) -> NodeView {
		folder_node(name, None, false, children)
	}

	fn part(name: &str, children: Vec<NodeView>) -> NodeView {
		folder_node(name, Some(FolderKind::Part), true, children)
	}

	fn off_part(name: &str, children: Vec<NodeView>) -> NodeView {
		folder_node(name, Some(FolderKind::Part), false, children)
	}

	fn front_matter(children: Vec<NodeView>) -> NodeView {
		folder_node(
			"Front Matter",
			Some(FolderKind::FrontMatter),
			true,
			children,
		)
	}

	fn off_front_matter(children: Vec<NodeView>) -> NodeView {
		folder_node(
			"Front Matter",
			Some(FolderKind::FrontMatter),
			false,
			children,
		)
	}

	fn back_matter(children: Vec<NodeView>) -> NodeView {
		folder_node("Back Matter", Some(FolderKind::BackMatter), true, children)
	}

	fn chapter_folder(name: &str, children: Vec<NodeView>) -> NodeView {
		folder_node(name, Some(FolderKind::Chapter), true, children)
	}

	fn off_chapter_folder(name: &str, children: Vec<NodeView>) -> NodeView {
		folder_node(name, Some(FolderKind::Chapter), false, children)
	}

	/// A project holding one Manuscript with these children in it.
	fn tree(children: Vec<NodeView>) -> Vec<NodeView> {
		vec![folder(MANUSCRIPT, children)]
	}

	/// The compiled book as one flat list of lines, which is what nearly every
	/// test below is really asserting about: what came out, in what order,
	/// under what.
	fn outline(compiled: &Compiled) -> Vec<String> {
		let mut lines = Vec::new();

		for scene in &compiled.front {
			lines.push(format!("front {}", scene.title));
		}
		for division in &compiled.body {
			match division {
				Division::Part(part) => {
					lines.push(format!("part {}", part.title));
					for chapter in &part.chapters {
						lines.extend(chapter_lines(chapter));
					}
				}
				Division::Chapter(chapter) => lines.extend(chapter_lines(chapter)),
			}
		}
		for scene in &compiled.back {
			lines.push(format!("back {}", scene.title));
		}

		lines
	}

	fn chapter_lines(chapter: &Chapter) -> Vec<String> {
		let mut lines = vec![format!("chapter {}", chapter.title)];
		lines.extend(
			chapter
				.scenes
				.iter()
				.map(|scene| format!("scene {}", scene.title)),
		);
		lines
	}

	#[test]
	fn a_project_without_a_manuscript_compiles_to_nothing() {
		let compiled = compile(&[folder("Notes", vec![doc("Idea.md")])]);

		assert_eq!(compiled, Compiled::default());
	}

	#[test]
	fn a_manuscript_out_of_the_book_compiles_to_nothing() {
		let compiled = compile(&[off_folder(MANUSCRIPT, vec![doc("Opening.md")])]);

		assert_eq!(compiled, Compiled::default());
	}

	#[test]
	fn only_the_manuscript_is_compiled() {
		let compiled = compile(&[
			folder(MANUSCRIPT, vec![doc("Opening.md")]),
			folder("Characters", vec![doc("Ada.md")]),
			folder("Outline", vec![doc("Beats.md")]),
		]);

		assert_eq!(outline(&compiled), ["chapter Opening", "scene Opening"]);
	}

	#[test]
	fn a_lone_document_is_a_chapter_of_one_scene() {
		let compiled = compile(&tree(vec![doc("Opening.md")]));

		assert_eq!(outline(&compiled), ["chapter Opening", "scene Opening"]);
	}

	#[test]
	fn parts_hold_chapters_hold_scenes() {
		let compiled = compile(&tree(vec![part(
			"Part One",
			vec![chapter_folder(
				"Arrival",
				vec![doc("Dawn.md"), doc("Dusk.md")],
			)],
		)]));

		assert_eq!(
			outline(&compiled),
			[
				"part Part One",
				"chapter Arrival",
				"scene Dawn",
				"scene Dusk",
			]
		);
	}

	#[test]
	fn a_chapter_of_one_document_sits_beside_a_part() {
		let compiled = compile(&tree(vec![
			part("Part One", vec![doc("Dawn.md")]),
			doc("Interlude.md"),
			chapter_folder("Departure", vec![doc("Dusk.md")]),
		]));

		assert_eq!(
			outline(&compiled),
			[
				"part Part One",
				"chapter Dawn",
				"scene Dawn",
				"chapter Interlude",
				"scene Interlude",
				"chapter Departure",
				"scene Dusk",
			]
		);
	}

	#[test]
	fn matter_comes_out_by_role_and_not_by_where_it_sits() {
		let compiled = compile(&tree(vec![
			back_matter(vec![doc("Afterword.md")]),
			doc("Opening.md"),
			front_matter(vec![doc("Dedication.md")]),
		]));

		assert_eq!(
			outline(&compiled),
			[
				"front Dedication",
				"chapter Opening",
				"scene Opening",
				"back Afterword",
			]
		);
	}

	#[test]
	fn matter_flattens_the_folders_inside_it() {
		let compiled = compile(&tree(vec![front_matter(vec![
			doc("Dedication.md"),
			folder("Praise", vec![doc("The Times.md")]),
		])]));

		assert_eq!(outline(&compiled), ["front Dedication", "front The Times"]);
	}

	#[test]
	fn matter_is_only_looked_for_at_the_top_of_the_manuscript() {
		// `tree::may_hold` will not let this be made, and a renderer that met
		// one anyway should still get something it can draw.
		let compiled = compile(&tree(vec![part(
			"Part One",
			vec![front_matter(vec![doc("Epigraph.md")])],
		)]));

		assert!(compiled.front.is_empty());
		assert_eq!(
			outline(&compiled),
			["part Part One", "chapter Front Matter", "scene Epigraph"]
		);
	}

	#[test]
	fn a_scene_out_of_the_book_is_dropped() {
		let compiled = compile(&tree(vec![chapter_folder(
			"Arrival",
			vec![doc("Dawn.md"), off_doc("Cut.md"), doc("Dusk.md")],
		)]));

		assert_eq!(
			outline(&compiled),
			["chapter Arrival", "scene Dawn", "scene Dusk"]
		);
	}

	#[test]
	fn a_chapter_out_of_the_book_takes_its_scenes_with_it() {
		let compiled = compile(&tree(vec![
			chapter_folder("Arrival", vec![doc("Dawn.md")]),
			off_chapter_folder("Cut", vec![doc("Ghost.md"), doc("Spectre.md")]),
		]));

		assert_eq!(outline(&compiled), ["chapter Arrival", "scene Dawn"]);
	}

	#[test]
	fn a_part_out_of_the_book_takes_its_chapters_with_it() {
		let compiled = compile(&tree(vec![
			off_part("Cut", vec![chapter_folder("Ghost", vec![doc("Ghost.md")])]),
			doc("Opening.md"),
		]));

		assert_eq!(outline(&compiled), ["chapter Opening", "scene Opening"]);
	}

	#[test]
	fn front_matter_out_of_the_book_leaves_nothing_in_front() {
		let compiled = compile(&tree(vec![
			off_front_matter(vec![doc("Dedication.md")]),
			doc("Opening.md"),
		]));

		assert!(compiled.front.is_empty());
		assert_eq!(outline(&compiled), ["chapter Opening", "scene Opening"]);
	}

	#[test]
	fn a_chapter_with_no_scenes_left_is_dropped() {
		let compiled = compile(&tree(vec![
			chapter_folder("Empty", vec![]),
			chapter_folder("Silenced", vec![off_doc("Cut.md")]),
			doc("Opening.md"),
		]));

		assert_eq!(outline(&compiled), ["chapter Opening", "scene Opening"]);
	}

	#[test]
	fn a_part_with_no_chapters_left_is_dropped() {
		let compiled = compile(&tree(vec![
			part(
				"Hollow",
				vec![chapter_folder("Silenced", vec![off_doc("Cut.md")])],
			),
			doc("Opening.md"),
		]));

		assert_eq!(outline(&compiled), ["chapter Opening", "scene Opening"]);
	}

	#[test]
	fn a_folder_nested_in_a_chapter_flattens_into_it() {
		let compiled = compile(&tree(vec![chapter_folder(
			"Arrival",
			vec![
				doc("Dawn.md"),
				folder("Drafts", vec![doc("Noon.md")]),
				doc("Dusk.md"),
			],
		)]));

		assert_eq!(
			outline(&compiled),
			["chapter Arrival", "scene Dawn", "scene Noon", "scene Dusk"]
		);
	}

	#[test]
	fn a_scene_carries_the_path_its_text_is_at() {
		let compiled = compile(&tree(vec![chapter_folder(
			"Arrival",
			vec![doc("Manuscript/Arrival/Dawn.md")],
		)]));

		let Some(Division::Chapter(chapter)) = compiled.body.first() else {
			panic!("the chapter is the only thing in the body");
		};
		assert_eq!(chapter.scenes[0].path, "Manuscript/Arrival/Dawn.md");
		assert_eq!(chapter.scenes[0].title, "Dawn");
	}

	#[test]
	fn a_chapter_of_one_document_is_the_document_itself() {
		let compiled = compile(&tree(vec![doc("Interlude.md")]));

		let Some(Division::Chapter(chapter)) = compiled.body.first() else {
			panic!("the chapter is the only thing in the body");
		};
		assert_eq!(chapter.id, chapter.scenes[0].id);
	}
}
