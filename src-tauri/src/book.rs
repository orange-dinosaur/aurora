//! The compile walk: the project's tree turned into the shape a book has.
//!
//! Every renderer stands on this. The walk takes the tree the sidebar is drawn
//! from, which already says of each node whether it is in the book, and works
//! out what the story is: what surrounds it, what its divisions are, and which
//! documents make up each one. Nothing here opens a file, so a renderer decides
//! for itself when to read the text a scene points at.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use uuid::Uuid;

use crate::document::{DocumentView, NodeView, body};
use crate::project::{Book, MANUSCRIPT, matter};
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

/// What every scene of the book holds, by id. The compile walk says what the
/// book is made of and this says what is in it, which is the one thing the
/// walk cannot work out without opening a file.
pub type Prose = HashMap<Uuid, String>;

/// How a scene change is written. Aurora's editor reads every spelling of a
/// thematic break and writes this one back, in `SCENE_BREAK` in
/// `src/markdown.ts`; an export the writer opens in Aurora again should meet
/// what it would have written itself.
const BREAK: &str = "---";

/// Every scene the book holds, in reading order.
pub fn all_scenes(compiled: &Compiled) -> Vec<&Scene> {
	let chapters = compiled.body.iter().flat_map(|division| match division {
		Division::Part(part) => part.chapters.iter().collect::<Vec<_>>(),
		Division::Chapter(chapter) => vec![chapter],
	});

	compiled
		.front
		.iter()
		.chain(chapters.flat_map(|chapter| chapter.scenes.iter()))
		.chain(compiled.back.iter())
		.collect()
}

/// Reads the prose of every scene in the book, with each one's front matter
/// block lifted off. A scene whose file will not open is left out and renders
/// as nothing rather than stopping the export: the tree comes from the
/// manifest, and the manifest can name a file that is no longer there.
pub fn prose(root: &Path, compiled: &Compiled) -> Prose {
	all_scenes(compiled)
		.into_iter()
		.filter_map(|scene| {
			let text = fs::read_to_string(root.join(&scene.path)).ok()?;
			Some((scene.id, body(&text).trim().to_owned()))
		})
		.collect()
}

/// The whole book as one Markdown document: the title and author, then what
/// comes before the story, the story itself, and what comes after.
///
/// A part and a piece of matter are both `#`, since both are a division of the
/// book, and a chapter is `##` whether it is a folder of scenes or the one
/// document that is the whole of it. Scenes inside a chapter run together with
/// a break between them, because a scene is a fragment of a chapter rather than
/// something with a name of its own.
pub fn markdown(book: &Book, compiled: &Compiled, prose: &Prose) -> String {
	let mut blocks = Vec::new();

	if !book.title.is_empty() {
		blocks.push(format!("# {}", book.title));
	}
	if !book.author.is_empty() {
		blocks.push(format!("by {}", book.author));
	}

	for scene in &compiled.front {
		piece(&mut blocks, scene, prose);
	}
	for division in &compiled.body {
		match division {
			Division::Part(part) => {
				blocks.push(format!("# {}", part.title));
				for chapter in &part.chapters {
					chapter_blocks(&mut blocks, chapter, prose);
				}
			}
			Division::Chapter(chapter) => chapter_blocks(&mut blocks, chapter, prose),
		}
	}
	for scene in &compiled.back {
		piece(&mut blocks, scene, prose);
	}

	// One blank line between blocks, and a newline at the end: a text file ends
	// in one, and every tool that reads Markdown expects it.
	let mut out = blocks.join("\n\n");
	if !out.is_empty() {
		out.push('\n');
	}
	out
}

/// A document that stands on its own, which is what every piece of front and
/// back matter is. It keeps its title, unlike a scene.
fn piece(blocks: &mut Vec<String>, scene: &Scene, prose: &Prose) {
	blocks.push(format!("# {}", scene.title));
	blocks.extend(told(std::slice::from_ref(scene), prose));
}

fn chapter_blocks(blocks: &mut Vec<String>, chapter: &Chapter, prose: &Prose) {
	blocks.push(format!("## {}", chapter.title));

	for (at, text) in told(&chapter.scenes, prose).into_iter().enumerate() {
		if at > 0 {
			blocks.push(BREAK.to_owned());
		}
		blocks.push(text);
	}
}

/// The prose of the scenes that have any. A scene that could not be read, or
/// that holds nothing but its front matter, is passed over here rather than
/// leaving an empty block or a break with nothing on either side of it.
fn told(scenes: &[Scene], prose: &Prose) -> Vec<String> {
	scenes
		.iter()
		.filter_map(|scene| prose.get(&scene.id))
		.filter(|text| !text.is_empty())
		.cloned()
		.collect()
}

/// Whether `pulldown-cmark` reads back everything Aurora's editor writes. The
/// list mirrors `src/markdown.test.ts`, which is where what the editor writes
/// is settled, so a construct added there belongs here too. Nothing in the
/// build parses Markdown yet; the EPUB and DOCX renderers will, and this is
/// what they will stand on.
#[cfg(test)]
mod reading_back {
	use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

	/// The parser as a renderer should ask for it. None of these three are in
	/// CommonMark, so each has to be turned on by name: strikethrough because
	/// Aurora's editor writes it, tables and footnotes because it keeps a
	/// pasted one verbatim and an export must not be the place they are lost.
	fn options() -> Options {
		Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_FOOTNOTES
	}

	/// What the parser understood, named the way these tests talk about it.
	fn read(markdown: &str) -> Vec<String> {
		Parser::new_ext(markdown, options())
			.map(|event| match event {
				Event::Start(tag) => format!("start {}", started(&tag)),
				Event::End(end) => format!("end {}", ended(&end)),
				Event::Text(text) => format!("text {text}"),
				Event::Code(text) => format!("code {text}"),
				Event::SoftBreak => "soft break".to_owned(),
				Event::HardBreak => "hard break".to_owned(),
				Event::Rule => "scene break".to_owned(),
				other => format!("{other:?}"),
			})
			.collect()
	}

	fn started(tag: &Tag) -> String {
		match tag {
			Tag::Paragraph => "paragraph".to_owned(),
			Tag::Heading { level, .. } => format!("heading {level}"),
			Tag::BlockQuote(_) => "quote".to_owned(),
			Tag::CodeBlock(_) => "code block".to_owned(),
			Tag::List(None) => "list".to_owned(),
			Tag::List(Some(_)) => "numbered list".to_owned(),
			Tag::Item => "item".to_owned(),
			Tag::Emphasis => "emphasis".to_owned(),
			Tag::Strong => "strong".to_owned(),
			Tag::Strikethrough => "strikethrough".to_owned(),
			Tag::Link { dest_url, .. } => format!("link {dest_url}"),
			other => format!("{other:?}"),
		}
	}

	fn ended(end: &TagEnd) -> String {
		match end {
			TagEnd::Paragraph => "paragraph".to_owned(),
			TagEnd::Heading(_) => "heading".to_owned(),
			TagEnd::BlockQuote(_) => "quote".to_owned(),
			TagEnd::CodeBlock => "code block".to_owned(),
			TagEnd::List(_) => "list".to_owned(),
			TagEnd::Item => "item".to_owned(),
			TagEnd::Emphasis => "emphasis".to_owned(),
			TagEnd::Strong => "strong".to_owned(),
			TagEnd::Strikethrough => "strikethrough".to_owned(),
			TagEnd::Link => "link".to_owned(),
			other => format!("{other:?}"),
		}
	}

	#[test]
	fn everything_the_editor_writes_reads_back() {
		let wanted: &[(&str, &[&str])] = &[
			(
				"# Chapter One",
				&["start heading h1", "text Chapter One", "end heading"],
			),
			(
				"### The room above the shop",
				&[
					"start heading h3",
					"text The room above the shop",
					"end heading",
				],
			),
			(
				"She was *late* and **cross**.",
				&[
					"start paragraph",
					"text She was ",
					"start emphasis",
					"text late",
					"end emphasis",
					"text  and ",
					"start strong",
					"text cross",
					"end strong",
					"text .",
					"end paragraph",
				],
			),
			(
				"It was ~~fine~~ awful.",
				&[
					"start paragraph",
					"text It was ",
					"start strikethrough",
					"text fine",
					"end strikethrough",
					"text  awful.",
					"end paragraph",
				],
			),
			(
				"Run `cargo test` first.",
				&[
					"start paragraph",
					"text Run ",
					"code cargo test",
					"text  first.",
					"end paragraph",
				],
			),
			(
				"- one\n- two",
				&[
					"start list",
					"start item",
					"text one",
					"end item",
					"start item",
					"text two",
					"end item",
					"end list",
				],
			),
			(
				"1. one\n2. two",
				&[
					"start numbered list",
					"start item",
					"text one",
					"end item",
					"start item",
					"text two",
					"end item",
					"end list",
				],
			),
			(
				"> He never came back.",
				&[
					"start quote",
					"start paragraph",
					"text He never came back.",
					"end paragraph",
					"end quote",
				],
			),
			(
				"```rust\nfn main() {}\n```",
				&["start code block", "text fn main() {}\n", "end code block"],
			),
			(
				"See [the map](https://example.com/map).",
				&[
					"start paragraph",
					"text See ",
					"start link https://example.com/map",
					"text the map",
					"end link",
					"text .",
					"end paragraph",
				],
			),
			(
				"One line.\nAnd another.",
				&[
					"start paragraph",
					"text One line.",
					"soft break",
					"text And another.",
					"end paragraph",
				],
			),
			(
				"- one\n    - inner\n- two",
				&[
					"start list",
					"start item",
					"text one",
					"start list",
					"start item",
					"text inner",
					"end item",
					"end list",
					"end item",
					"start item",
					"text two",
					"end item",
					"end list",
				],
			),
		];

		for (markdown, events) in wanted {
			assert_eq!(read(markdown), *events, "reading {markdown:?}");
		}
	}

	#[test]
	fn a_scene_break_is_read_as_a_break_and_not_as_a_heading() {
		// The one that would bite: three dashes under a line of prose is a
		// setext heading in Markdown, and Aurora writes a scene break this way.
		assert_eq!(
			read("One.\n\n---\n\nTwo."),
			[
				"start paragraph",
				"text One.",
				"end paragraph",
				"scene break",
				"start paragraph",
				"text Two.",
				"end paragraph",
			]
		);
	}

	#[test]
	fn strikethrough_is_only_read_when_it_is_asked_for() {
		let plain: Vec<String> = Parser::new_ext("It was ~~fine~~ awful.", Options::empty())
			.filter_map(|event| match event {
				Event::Text(text) => Some(text.to_string()),
				_ => None,
			})
			.collect();

		assert_eq!(plain.concat(), "It was ~~fine~~ awful.");
		assert!(read("It was ~~fine~~ awful.").contains(&"start strikethrough".to_owned()));
	}

	#[test]
	fn a_pasted_table_is_read_as_a_table() {
		let table = "| a | b |\n| --- | --- |\n| 1 | 2 |";

		assert!(
			read(table)
				.iter()
				.any(|event| event.starts_with("start Table")),
			"a table needs ENABLE_TABLES: without it every row reads as prose"
		);
	}

	#[test]
	fn a_pasted_footnote_is_read_as_a_footnote() {
		let footnote = "Text[^1]\n\n[^1]: A note.";

		assert!(
			read(footnote)
				.iter()
				.any(|event| event.starts_with("FootnoteReference")),
			"a footnote needs ENABLE_FOOTNOTES: without it the marker reads as brackets"
		);
	}

	#[test]
	fn an_html_comment_arrives_as_html_for_a_renderer_to_decide_about() {
		assert!(
			read("<!-- check this name -->\n\nProse.")
				.iter()
				.any(|event| event.starts_with("Html(")),
			"a renderer has to say what it does with html rather than never meeting it"
		);
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

	/// A book with nothing said about it but these two.
	fn book(title: &str, author: &str) -> Book {
		let mut book = Book::new(title);
		book.author = author.to_owned();
		book
	}

	/// What the scenes hold, named by title, since their ids are made as the
	/// tree is built.
	fn prose_of(compiled: &Compiled, said: &[(&str, &str)]) -> Prose {
		all_scenes(compiled)
			.into_iter()
			.filter_map(|scene| {
				said.iter()
					.find(|(title, _)| *title == scene.title)
					.map(|(_, text)| (scene.id, (*text).to_owned()))
			})
			.collect()
	}

	#[test]
	fn the_title_and_the_author_come_first() {
		let compiled = compile(&tree(vec![doc("Opening.md")]));
		let prose = prose_of(&compiled, &[("Opening", "She ran.")]);

		assert_eq!(
			markdown(&book("Ithaca", "Ada Lovelace"), &compiled, &prose),
			"# Ithaca\n\nby Ada Lovelace\n\n## Opening\n\nShe ran.\n"
		);
	}

	#[test]
	fn a_book_with_no_author_says_nothing_about_one() {
		let compiled = compile(&tree(vec![doc("Opening.md")]));
		let prose = prose_of(&compiled, &[("Opening", "She ran.")]);

		assert_eq!(
			markdown(&book("Ithaca", ""), &compiled, &prose),
			"# Ithaca\n\n## Opening\n\nShe ran.\n"
		);
	}

	#[test]
	fn a_part_is_one_hash_and_a_chapter_is_two() {
		let compiled = compile(&tree(vec![part(
			"Part One",
			vec![chapter_folder("Arrival", vec![doc("Dawn.md")])],
		)]));
		let prose = prose_of(&compiled, &[("Dawn", "She arrived.")]);

		assert_eq!(
			markdown(&book("Ithaca", ""), &compiled, &prose),
			"# Ithaca\n\n# Part One\n\n## Arrival\n\nShe arrived.\n"
		);
	}

	#[test]
	fn scenes_in_a_chapter_are_parted_by_a_break() {
		let compiled = compile(&tree(vec![chapter_folder(
			"Arrival",
			vec![doc("Dawn.md"), doc("Dusk.md")],
		)]));
		let prose = prose_of(
			&compiled,
			&[("Dawn", "She arrived."), ("Dusk", "She left.")],
		);

		assert_eq!(
			markdown(&book("Ithaca", ""), &compiled, &prose),
			"# Ithaca\n\n## Arrival\n\nShe arrived.\n\n---\n\nShe left.\n"
		);
	}

	#[test]
	fn matter_keeps_its_own_title_and_sits_around_the_story() {
		let compiled = compile(&tree(vec![
			front_matter(vec![doc("Dedication.md")]),
			doc("Opening.md"),
			back_matter(vec![doc("Afterword.md")]),
		]));
		let prose = prose_of(
			&compiled,
			&[
				("Dedication", "For Ada."),
				("Opening", "She ran."),
				("Afterword", "Thanks."),
			],
		);

		assert_eq!(
			markdown(&book("Ithaca", ""), &compiled, &prose),
			"# Ithaca\n\n# Dedication\n\nFor Ada.\n\n## Opening\n\nShe ran.\n\n# Afterword\n\nThanks.\n"
		);
	}

	#[test]
	fn a_scene_with_nothing_in_it_leaves_no_break_behind() {
		let compiled = compile(&tree(vec![chapter_folder(
			"Arrival",
			vec![doc("Dawn.md"), doc("Empty.md"), doc("Dusk.md")],
		)]));
		// The middle one holds nothing, and its file could not be read at all.
		let prose = prose_of(
			&compiled,
			&[("Dawn", "She arrived."), ("Dusk", "She left.")],
		);

		assert_eq!(
			markdown(&book("Ithaca", ""), &compiled, &prose),
			"# Ithaca\n\n## Arrival\n\nShe arrived.\n\n---\n\nShe left.\n"
		);
	}

	#[test]
	fn a_book_with_nothing_in_it_renders_to_nothing() {
		let compiled = compile(&[]);

		assert_eq!(markdown(&book("", ""), &compiled, &Prose::new()), "");
	}

	#[test]
	fn all_scenes_reads_front_then_story_then_back() {
		let compiled = compile(&tree(vec![
			back_matter(vec![doc("Afterword.md")]),
			part(
				"Part One",
				vec![chapter_folder("Arrival", vec![doc("Dawn.md")])],
			),
			doc("Interlude.md"),
			front_matter(vec![doc("Dedication.md")]),
		]));

		let titles: Vec<&str> = all_scenes(&compiled)
			.into_iter()
			.map(|scene| scene.title.as_str())
			.collect();

		assert_eq!(titles, ["Dedication", "Dawn", "Interlude", "Afterword"]);
	}

	#[test]
	fn prose_arrives_without_the_front_matter_block() {
		let root = tempfile::tempdir().unwrap();
		fs::create_dir(root.path().join("Manuscript")).unwrap();
		fs::write(
			root.path().join("Manuscript/Dawn.md"),
			"---\ninBook: true\n---\n\nShe arrived.\n",
		)
		.unwrap();
		let compiled = compile(&tree(vec![doc("Manuscript/Dawn.md")]));

		let held = prose(root.path(), &compiled);

		let scene = all_scenes(&compiled)[0];
		assert_eq!(
			held.get(&scene.id).map(String::as_str),
			Some("She arrived.")
		);
	}

	#[test]
	fn a_scene_whose_file_is_gone_is_left_out() {
		let root = tempfile::tempdir().unwrap();
		let compiled = compile(&tree(vec![doc("Manuscript/Missing.md")]));

		let held = prose(root.path(), &compiled);

		assert!(held.is_empty());
		assert_eq!(
			markdown(&book("Ithaca", ""), &compiled, &held),
			"# Ithaca\n\n## Missing\n"
		);
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
