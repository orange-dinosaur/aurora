//! The compile walk: the project's tree turned into the shape a book has.
//!
//! Every renderer stands on this. The walk takes the tree the sidebar is drawn
//! from, which already says of each node whether it is in the book, and works
//! out what the story is: what surrounds it, what its divisions are, and which
//! documents make up each one. Nothing here opens a file, so a renderer decides
//! for itself when to read the text a scene points at.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use pulldown_cmark::Options;
use serde::Serialize;
use tauri::ipc::Channel;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::document::{DocumentView, NodeView, body, document_tree, words_of};
use crate::docx::docx;
use crate::epub::{Flavour, epub};
use crate::project::{
	Book, Error, ExportFormat, Format, MANUSCRIPT, RESERVED, Result, cover_bytes, matter,
	read_manifest,
};
use crate::script::fountain::{Block, parse};
use crate::script::pdf::pdf;
use crate::text::text;
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

/// One stretch of the book as a writer draws it: a piece of front or back
/// matter, a part with its chapters, or a chapter outside any part.
pub enum Section<'a> {
	Matter(&'a Scene),
	Part(&'a Part),
	Chapter(&'a Chapter),
}

/// The book's sections in reading order.
pub fn sections(compiled: &Compiled) -> Vec<Section<'_>> {
	let body = compiled.body.iter().map(|division| match division {
		Division::Part(part) => Section::Part(part),
		Division::Chapter(chapter) => Section::Chapter(chapter),
	});

	compiled
		.front
		.iter()
		.map(Section::Matter)
		.chain(body)
		.chain(compiled.back.iter().map(Section::Matter))
		.collect()
}

/// Every scene the book holds, in reading order.
pub fn all_scenes(compiled: &Compiled) -> Vec<&Scene> {
	sections(compiled)
		.into_iter()
		.flat_map(|section| match section {
			Section::Matter(scene) => vec![scene],
			Section::Part(part) => part
				.chapters
				.iter()
				.flat_map(|chapter| &chapter.scenes)
				.collect(),
			Section::Chapter(chapter) => chapter.scenes.iter().collect(),
		})
		.collect()
}

/// What the Markdown parser is asked to read beyond the core. The editor
/// writes strikethrough, and keeps a pasted table or footnote as it came.
pub fn options() -> Options {
	Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_FOOTNOTES
}

/// The title the book goes by. A reader has to be given one.
pub fn title_of(book: &Book) -> &str {
	match book.title.trim() {
		"" => "Untitled",
		title => title,
	}
}

/// Whether XML 1.0 can hold `c` at all.
pub fn legal(c: char) -> bool {
	matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}')
		|| c >= '\u{10000}'
}

/// Text made safe to sit in XML, between tags or inside an attribute's quotes.
pub fn escaped(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	for c in text.chars().filter(|&c| legal(c)) {
		match c {
			'&' => out.push_str("&amp;"),
			'<' => out.push_str("&lt;"),
			'>' => out.push_str("&gt;"),
			'"' => out.push_str("&quot;"),
			'\'' => out.push_str("&apos;"),
			c => out.push(c),
		}
	}
	out
}

/// Reads the prose of every scene in the book, with each one's front matter
/// block lifted off. A scene whose file will not open is left out and renders
/// as nothing rather than stopping the export: the tree comes from the
/// manifest, and the manifest can name a file that is no longer there.
pub fn prose(root: &Path, compiled: &Compiled) -> Prose {
	read_prose(root, compiled, |_| {}).0
}

/// `prose`, telling `read` how many scenes it has been through after each one,
/// along with the paths of the scenes it could not read.
fn read_prose(
	root: &Path,
	compiled: &Compiled,
	mut read: impl FnMut(usize),
) -> (Prose, Vec<String>) {
	let mut prose = Prose::new();
	let mut unread = Vec::new();
	for (at, scene) in all_scenes(compiled).into_iter().enumerate() {
		match fs::read_to_string(root.join(&scene.path)) {
			Ok(text) => {
				prose.insert(scene.id, body(&text).trim().to_owned());
			}
			Err(_) => unread.push(scene.path.clone()),
		}
		read(at + 1);
	}
	(prose, unread)
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

	let (title, author) = (book.title.trim(), book.author.trim());
	if !title.is_empty() {
		blocks.push(format!("# {title}"));
	}
	if !author.is_empty() {
		blocks.push(author.to_owned());
	}

	for section in sections(compiled) {
		match section {
			Section::Matter(scene) => piece(&mut blocks, scene, prose),
			Section::Part(part) => {
				blocks.push(format!("# {}", part.title));
				for chapter in &part.chapters {
					chapter_blocks(&mut blocks, chapter, prose);
				}
			}
			Section::Chapter(chapter) => chapter_blocks(&mut blocks, chapter, prose),
		}
	}

	// One blank line between blocks, and a newline at the end: a text file ends
	// in one, and every tool that reads Markdown expects it.
	let mut out = blocks.join("\n\n");
	if !out.is_empty() {
		out.push('\n');
	}
	out
}

/// The screenplay as one Fountain file: the title page, then every scene's
/// body in binder order. A scene already holds Fountain, so it goes out as
/// written, and acts add nothing but their order.
pub fn fountain(book: &Book, compiled: &Compiled, prose: &Prose) -> String {
	let author = book.author.trim();
	let mut page = vec![title_field("Title", title_of(book))];
	if !author.is_empty() {
		page.push(title_field("Credit", "Written by"));
		page.push(title_field("Author", author));
	}
	for (key, value) in [("Contact", &book.contact), ("Draft date", &book.draft)] {
		if !value.trim().is_empty() {
			page.push(title_field(key, value.trim()));
		}
	}

	// A blank line ends the title page, and one parts each scene from the next.
	let mut blocks = vec![page.concat().trim_end().to_owned()];
	blocks.extend(
		all_scenes(compiled)
			.into_iter()
			.filter_map(|scene| prose.get(&scene.id))
			.filter(|text| !text.is_empty())
			.cloned(),
	);
	let mut out = blocks.join("\n\n");
	out.push('\n');
	out
}

/// Every scene's body read as Fountain, in binder order.
fn script(compiled: &Compiled, prose: &Prose) -> Vec<Vec<Block>> {
	all_scenes(compiled)
		.into_iter()
		.map(|scene| parse(prose.get(&scene.id).map_or("", String::as_str)))
		.collect()
}

/// One key of a Fountain title page. A value of several lines goes under its
/// key, each line indented, which is how Fountain tells it from the next key.
fn title_field(key: &str, value: &str) -> String {
	let lines: Vec<&str> = value
		.lines()
		.map(str::trim)
		.filter(|l| !l.is_empty())
		.collect();
	match lines.as_slice() {
		[line] => format!("{key}: {line}\n"),
		_ => {
			let indented: String = lines.iter().map(|line| format!("    {line}\n")).collect();
			format!("{key}:\n{indented}")
		}
	}
}

/// A document that stands on its own, which is what every piece of front and
/// back matter is. It keeps its title, unlike a scene.
fn piece(blocks: &mut Vec<String>, scene: &Scene, prose: &Prose) {
	blocks.push(format!("# {}", scene.title));
	blocks.extend(
		told(std::slice::from_ref(scene), prose)
			.iter()
			.map(|text| demoted(text)),
	);
}

/// A scene's own headings, two levels down, so they sit under the chapter's
/// title instead of beside it. Lines inside a fenced block are code and stay
/// as they are, and a heading already at five or six is left alone.
fn demoted(text: &str) -> String {
	let mut fenced = false;
	let lines: Vec<String> = text
		.split('\n')
		.map(|line| {
			if line.trim_start().starts_with("```") {
				fenced = !fenced;
			}
			let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
			let heading = (1..=4).contains(&hashes) && line[hashes..].starts_with(' ');
			if !fenced && heading {
				format!("##{line}")
			} else {
				line.to_owned()
			}
		})
		.collect();
	lines.join("\n")
}

fn chapter_blocks(blocks: &mut Vec<String>, chapter: &Chapter, prose: &Prose) {
	blocks.push(format!("## {}", chapter.title));
	blocks.extend(broken(&chapter.scenes, prose));
}

/// The prose of a chapter's scenes, with a break between each and the next.
pub fn broken(scenes: &[Scene], prose: &Prose) -> Vec<String> {
	let mut blocks = Vec::new();
	for (at, text) in told(scenes, prose).into_iter().enumerate() {
		if at > 0 {
			blocks.push(BREAK.to_owned());
		}
		blocks.push(demoted(&text));
	}
	blocks
}

/// The prose of the scenes that have any. A scene that could not be read, or
/// that holds nothing but its front matter, is passed over here rather than
/// leaving an empty block or a break with nothing on either side of it.
pub fn told(scenes: &[Scene], prose: &Prose) -> Vec<String> {
	scenes
		.iter()
		.filter_map(|scene| prose.get(&scene.id))
		.filter(|text| !text.is_empty())
		.cloned()
		.collect()
}

/// How much the book holds, for the Export panel to say before anything is
/// written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct Extent {
	pub scenes: usize,
	pub words: usize,
}

/// What an export would take, with the words counted the way the sidebar
/// counts them.
#[tauri::command(async)]
pub fn book_extent(root: PathBuf) -> Result<Extent> {
	let compiled = compile(&document_tree(root.clone())?);
	let words = prose(&root, &compiled)
		.values()
		.map(|text| words_of(text))
		.sum();

	Ok(Extent {
		scenes: all_scenes(&compiled).len(),
		words,
	})
}

/// How far an export has got. Every scene read is one step towards `total` and
/// so is every file written, so `done` over `total` is how full the bar is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "stage", rename_all = "camelCase")]
#[ts(export)]
pub enum Progress {
	Reading {
		done: usize,
		total: usize,
		scenes: usize,
	},
	Writing {
		done: usize,
		total: usize,
		name: String,
	},
}

/// Writes the book into `folder` once for each format ticked, reporting as it
/// goes, and answers with the names of the files written.
///
/// It runs off the main thread. A command on it holds its reports back until
/// it returns, and the bar would only ever be seen full.
#[tauri::command(async)]
pub fn export_book(
	root: PathBuf,
	folder: PathBuf,
	formats: Vec<ExportFormat>,
	progress: Channel<Progress>,
) -> Result<Vec<String>> {
	export(&root, &folder, &formats, |step| {
		// A report that goes astray costs the writer the bar, not the book.
		let _ = progress.send(step);
	})
}

/// The export itself. A file already there under the same name is replaced:
/// exporting again into the same folder is asking for the new book, not a
/// second copy beside the old one. A scene that cannot be read stops it before
/// anything is written, so no file ever leaves part of the book out.
pub fn export(
	root: &Path,
	folder: &Path,
	formats: &[ExportFormat],
	mut report: impl FnMut(Progress),
) -> Result<Vec<String>> {
	// Writing into the project would make the export part of the book.
	if folder.canonicalize()?.starts_with(root.canonicalize()?) {
		return Err(Error::ExportInsideProject);
	}
	let manifest = read_manifest(root)?;
	let screenplay = manifest.format == Format::Screenplay;
	if let Some(&refused) = formats
		.iter()
		.find(|format| format.for_scripts() != screenplay)
	{
		return Err(Error::ExportFormatRefused(refused));
	}
	let book = manifest.book;
	let compiled = compile(&document_tree(root.to_path_buf())?);
	let scenes = all_scenes(&compiled).len();
	let total = scenes + formats.len();
	let (prose, unread) = read_prose(root, &compiled, |done| {
		report(Progress::Reading {
			done,
			total,
			scenes,
		})
	});
	if !unread.is_empty() {
		return Err(Error::ScenesUnreadable(unread));
	}
	let stem = file_stem(&book.title);
	let epub_for = |flavour| {
		// A cover that has gone missing leaves the book without one rather
		// than stopping the export.
		let cover = cover_bytes(root).ok();
		let now = OffsetDateTime::now_utc();
		epub(&book, &compiled, &prose, cover.as_deref(), flavour, now)
	};

	let mut written = Vec::new();
	for &format in formats {
		let name = format!("{stem}.{}", extension(format));
		report(Progress::Writing {
			done: scenes + written.len(),
			total,
			name: name.clone(),
		});

		let made = (|| -> Result<()> {
			let bytes = match format {
				ExportFormat::Markdown => markdown(&book, &compiled, &prose).into_bytes(),
				ExportFormat::Epub => epub_for(Flavour::Plain)?,
				ExportFormat::Kepub => epub_for(Flavour::Kobo)?,
				ExportFormat::Docx => docx(&book, &compiled, &prose)?,
				ExportFormat::Text => text(&book, &compiled, &prose).into_bytes(),
				ExportFormat::Fountain => fountain(&book, &compiled, &prose).into_bytes(),
				ExportFormat::Pdf => pdf(&book, &script(&compiled, &prose)),
				ExportFormat::Fdx => {
					return Err(Error::ExportFormatRefused(format));
				}
			};
			// Whole or not at all: a reader never finds half a file under the
			// book's name.
			let part = folder.join(format!("{name}.part"));
			let done = fs::write(&part, bytes).and_then(|()| fs::rename(&part, folder.join(&name)));
			if done.is_err() {
				let _ = fs::remove_file(&part);
			}
			Ok(done?)
		})();
		if let Err(cause) = made {
			return Err(Error::ExportStopped {
				written,
				cause: Box::new(cause),
			});
		}
		written.push(name);
	}
	Ok(written)
}

/// What a file in each format ends in.
fn extension(format: ExportFormat) -> &'static str {
	match format {
		ExportFormat::Markdown => "md",
		ExportFormat::Epub => "epub",
		ExportFormat::Kepub => "kepub.epub",
		ExportFormat::Docx => "docx",
		ExportFormat::Text => "txt",
		ExportFormat::Fountain => "fountain",
		ExportFormat::Pdf => "pdf",
		ExportFormat::Fdx => "fdx",
	}
}

/// What an exported file is called: the book's title, less anything a file
/// name cannot hold on one system or another, since an export is as likely to
/// be copied onto a stick as kept where it was written.
fn file_stem(title: &str) -> String {
	let kept: String = title
		.chars()
		.filter(|c| !c.is_control() && !r#"/\:*?"<>|"#.contains(*c))
		.collect();
	// A leading dot would hide the file, and Windows drops a trailing one.
	let mut stem = kept.trim().trim_matches('.').trim();
	// Room for the longest extension and `.part` inside a 255-byte name.
	while stem.len() > 200 {
		let mut end = 200;
		while !stem.is_char_boundary(end) {
			end -= 1;
		}
		stem = stem[..end].trim_end_matches('.').trim_end();
	}

	if stem.is_empty() {
		"Untitled".to_owned()
	} else if RESERVED.iter().any(|name| stem.eq_ignore_ascii_case(name)) {
		format!("{stem}_")
	} else {
		stem.to_owned()
	}
}

/// Whether `pulldown-cmark` reads back everything Aurora's editor writes. The
/// list mirrors `src/markdown.test.ts`, which is where what the editor writes
/// is settled, so a construct added there belongs here too. The EPUB renderer
/// reads with the same options, `epub::options`, so this is what it stands on.
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
			words: 0,
			pages: None,
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
			pages: None,
			target: None,
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
	#[test]
	fn a_title_makes_a_file_name() {
		assert_eq!(file_stem("The Odyssey"), "The Odyssey");
		assert_eq!(file_stem("Either/Or: A Fragment"), "EitherOr A Fragment");
		assert_eq!(file_stem(".hidden"), "hidden");
		assert_eq!(file_stem("  ?  "), "Untitled");
		assert_eq!(file_stem("CON"), "CON_");
		assert_eq!(file_stem("Title."), "Title");
		assert!(file_stem(&"é".repeat(300)).len() <= 200);
	}

	#[test]
	fn an_export_leaves_an_older_manifest_as_it_was() {
		let parent = tempfile::tempdir().unwrap();
		let out = tempfile::tempdir().unwrap();
		let root = ithaca(parent.path());
		let path = root.join(crate::project::MANIFEST_FILE);
		let mut manifest: serde_json::Value =
			serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
		manifest["version"] = 3.into();
		fs::write(&path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
		let before = fs::read(&path).unwrap();

		export(&root, out.path(), &[ExportFormat::Markdown], |_| {}).unwrap();

		assert_eq!(fs::read(&path).unwrap(), before);
	}

	#[test]
	fn an_export_writes_one_file_per_format() {
		let parent = tempfile::tempdir().unwrap();
		let out = tempfile::tempdir().unwrap();

		let written = export(
			&ithaca(parent.path()),
			out.path(),
			&[
				ExportFormat::Markdown,
				ExportFormat::Epub,
				ExportFormat::Kepub,
				ExportFormat::Docx,
			],
			|_| {},
		)
		.unwrap();

		assert_eq!(
			written,
			vec![
				"Ithaca.md",
				"Ithaca.epub",
				"Ithaca.kepub.epub",
				"Ithaca.docx",
			]
		);
		let text = fs::read_to_string(out.path().join("Ithaca.md")).unwrap();
		assert!(text.starts_with("# Ithaca\n"));
		let epub = fs::read(out.path().join("Ithaca.epub")).unwrap();
		assert!(epub.starts_with(b"PK\x03\x04"));
	}

	#[test]
	fn an_export_reports_each_scene_read_then_each_file_written() {
		let parent = tempfile::tempdir().unwrap();
		let out = tempfile::tempdir().unwrap();
		let mut steps = Vec::new();

		export(
			&ithaca(parent.path()),
			out.path(),
			&[ExportFormat::Markdown],
			|step| steps.push(step),
		)
		.unwrap();

		let (last, reading) = steps.split_last().expect("an export reports something");
		let scenes = reading.len();
		let total = scenes + 1;
		for (at, step) in reading.iter().enumerate() {
			assert_eq!(
				*step,
				Progress::Reading {
					done: at + 1,
					total,
					scenes,
				}
			);
		}
		assert_eq!(
			*last,
			Progress::Writing {
				done: scenes,
				total,
				name: "Ithaca.md".to_owned(),
			}
		);
	}

	fn ithaca(parent: &Path) -> PathBuf {
		crate::project::create(
			parent,
			"Ithaca",
			crate::project::Format::Novel,
			time::OffsetDateTime::now_utc(),
		)
		.unwrap()
	}

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
			"# Ithaca\n\nAda Lovelace\n\n## Opening\n\nShe ran.\n"
		);
	}

	#[test]
	fn a_blank_title_writes_no_title_line() {
		let compiled = compile(&tree(vec![doc("Opening.md")]));
		let prose = prose_of(&compiled, &[("Opening", "She ran.")]);

		assert_eq!(
			markdown(&book(" ", " "), &compiled, &prose),
			"## Opening\n\nShe ran.\n"
		);
	}

	#[test]
	fn a_scenes_headings_go_two_levels_down_except_in_code() {
		let compiled = compile(&tree(vec![doc("Opening.md")]));
		let text = "# Later\n\n#hashtag\n\n```\n# not a heading\n```\n\n##### Deep";
		let prose = prose_of(&compiled, &[("Opening", text)]);

		assert_eq!(
			markdown(&book("", ""), &compiled, &prose),
			"## Opening\n\n### Later\n\n#hashtag\n\n```\n# not a heading\n```\n\n##### Deep\n"
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
	fn a_novel_will_not_export_as_a_script() {
		let parent = tempfile::tempdir().unwrap();
		let out = tempfile::tempdir().unwrap();
		let root = ithaca(parent.path());

		let refused = export(
			&root,
			out.path(),
			&[ExportFormat::Markdown, ExportFormat::Pdf],
			|_| {},
		);

		assert!(matches!(
			refused,
			Err(Error::ExportFormatRefused(ExportFormat::Pdf))
		));
		assert!(fs::read_dir(out.path()).unwrap().next().is_none());
	}

	#[test]
	fn a_screenplay_will_not_export_as_a_book() {
		let parent = tempfile::tempdir().unwrap();
		let out = tempfile::tempdir().unwrap();
		let root = crate::project::create(
			parent.path(),
			"Ithaca",
			Format::Screenplay,
			OffsetDateTime::now_utc(),
		)
		.unwrap();

		let refused = export(&root, out.path(), &[ExportFormat::Epub], |_| {});

		assert!(matches!(
			refused,
			Err(Error::ExportFormatRefused(ExportFormat::Epub))
		));
		assert!(fs::read_dir(out.path()).unwrap().next().is_none());
	}

	#[test]
	fn a_fountain_title_page_leaves_out_what_is_blank() {
		let mut book = book("Ithaca", "");
		book.draft = "  ".to_owned();

		let written = fountain(&book, &Compiled::default(), &Prose::new());

		assert_eq!(written, "Title: Ithaca\n");
	}

	#[test]
	fn a_screenplay_exports_as_fountain_in_binder_order() {
		let parent = tempfile::tempdir().unwrap();
		let out = tempfile::tempdir().unwrap();
		let root = crate::project::create(
			parent.path(),
			"Ithaca",
			Format::Screenplay,
			OffsetDateTime::now_utc(),
		)
		.unwrap();
		let path = root.join(crate::project::MANIFEST_FILE);
		let mut manifest: serde_json::Value =
			serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
		manifest["book"]["author"] = "Penelope".into();
		manifest["book"]["contact"] = "Agent Argos\n555 0100".into();
		manifest["book"]["draft"] = "Second draft".into();
		fs::write(&path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
		let manuscript = document_tree(root.clone())
			.unwrap()
			.into_iter()
			.find_map(|node| match node {
				NodeView::Folder { id, name, .. } if name == MANUSCRIPT => Some(id),
				_ => None,
			})
			.unwrap();
		for act in ["Opening", "Ending"] {
			let NodeView::Folder { id, .. } = crate::document::create_folder(
				root.clone(),
				manuscript,
				act.to_owned(),
				Some(FolderKind::Part),
			)
			.unwrap() else {
				unreachable!()
			};
			let scene =
				crate::document::create_document(root.clone(), id, "Hall".to_owned()).unwrap();
			let file = root.join(&scene.path);
			let text = fs::read_to_string(&file).unwrap();
			fs::write(&file, format!("{text}\nINT. HALL - {act}\n\nShe waits.\n")).unwrap();
		}

		let written = export(&root, out.path(), &[ExportFormat::Fountain], |_| {}).unwrap();

		assert_eq!(written, vec!["Ithaca.fountain"]);
		assert_eq!(
			fs::read_to_string(out.path().join("Ithaca.fountain")).unwrap(),
			"Title: Ithaca\nCredit: Written by\nAuthor: Penelope\n\
			Contact:\n    Agent Argos\n    555 0100\nDraft date: Second draft\n\n\
			INT.\n\n\
			INT. HALL - Opening\n\nShe waits.\n\n\
			INT. HALL - Ending\n\nShe waits.\n"
		);
	}

	#[test]
	fn a_scene_whose_file_is_gone_stops_the_export_by_name() {
		let parent = tempfile::tempdir().unwrap();
		let out = tempfile::tempdir().unwrap();
		let root = ithaca(parent.path());
		let compiled = compile(&document_tree(root.clone()).unwrap());
		let gone = all_scenes(&compiled)[0].path.clone();
		fs::remove_file(root.join(&gone)).unwrap();

		let refused = export(&root, out.path(), &[ExportFormat::Markdown], |_| {});

		assert!(matches!(refused, Err(Error::ScenesUnreadable(paths)) if paths == [gone]));
		assert_eq!(fs::read_dir(out.path()).unwrap().count(), 0);
	}

	#[test]
	fn an_export_into_the_project_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = ithaca(parent.path());

		let refused = export(&root, &root, &[ExportFormat::Markdown], |_| {});

		assert!(matches!(refused, Err(Error::ExportInsideProject)));
	}

	#[test]
	fn an_export_that_stops_names_what_it_wrote_and_leaves_no_part() {
		let parent = tempfile::tempdir().unwrap();
		let out = tempfile::tempdir().unwrap();
		fs::create_dir(out.path().join("Ithaca.docx")).unwrap();

		let stopped = export(
			&ithaca(parent.path()),
			out.path(),
			&[ExportFormat::Markdown, ExportFormat::Docx],
			|_| {},
		);

		let Err(Error::ExportStopped { written, .. }) = stopped else {
			panic!("the export should stop at the docx");
		};
		assert_eq!(written, ["Ithaca.md"]);
		assert!(!out.path().join("Ithaca.docx.part").exists());
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
