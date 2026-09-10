//! The book as plain text: the Markdown export's shape with the markup taken
//! out, for pasting into a form or reading in a terminal.

use std::collections::HashMap;
use std::mem::take;
use std::slice;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::book::{Chapter, Compiled, Division, Prose, Scene, told};
use crate::epub::options;
use crate::project::Book;

/// The line a scene break is centred on. Paragraphs are not wrapped to it: a
/// hard wrap puts a line break into every sentence of a book pasted anywhere
/// else, and a terminal wraps a long line by itself.
const WIDTH: usize = 72;

/// The whole book as plain text. Every title sits on a line of its own with two
/// blank lines above it, so it cannot be taken for a one-line paragraph, and
/// the scenes of a chapter are parted by a centred `#`.
pub fn text(book: &Book, compiled: &Compiled, prose: &Prose) -> String {
	let mut blocks = Vec::new();

	if !book.title.is_empty() {
		blocks.push(book.title.clone());
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
				blocks.push(title(&part.title));
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

	let mut out = blocks.join("\n\n").trim_start_matches('\n').to_owned();
	if !out.is_empty() {
		out.push('\n');
	}
	out
}

fn piece(blocks: &mut Vec<String>, scene: &Scene, prose: &Prose) {
	blocks.push(title(&scene.title));
	blocks.extend(plain_scenes(slice::from_ref(scene), prose));
}

fn chapter_blocks(blocks: &mut Vec<String>, chapter: &Chapter, prose: &Prose) {
	blocks.push(title(&chapter.title));
	for (at, block) in plain_scenes(&chapter.scenes, prose).into_iter().enumerate() {
		if at > 0 {
			blocks.push(scene_break());
		}
		blocks.push(block);
	}
}

/// The scenes' prose as plain text, passing over any that come out empty, such
/// as a scene holding nothing but a comment.
fn plain_scenes(scenes: &[Scene], prose: &Prose) -> Vec<String> {
	told(scenes, prose)
		.iter()
		.map(String::as_str)
		.map(plain)
		.filter(|text| !text.is_empty())
		.collect()
}

/// A title, with the blank line that sets it further apart than a paragraph.
fn title(name: &str) -> String {
	format!("\n{name}")
}

fn scene_break() -> String {
	format!("{:^WIDTH$}", "#").trim_end().to_owned()
}

/// A scene's Markdown with the markup taken out. A paragraph is one line and
/// blocks are parted by a blank line. Quotes, lists, code and footnotes are set
/// off by indenting them, and a link keeps its address in brackets after its
/// text, since on paper there is nothing to click.
fn plain(markdown: &str) -> String {
	let mut writer = Writer::default();
	for event in Parser::new_ext(markdown, options()) {
		writer.event(event);
	}
	writer.close();
	writer.blocks.join("\n\n")
}

/// Something that sets off the blocks inside it: a quote, a list item, a
/// footnote. The first line under it starts with `first` and every later one
/// with `rest`, so a bullet is written once and what follows hangs beneath it.
struct Frame {
	first: String,
	rest: String,
}

#[derive(Default)]
struct Writer {
	blocks: Vec<String>,
	/// The block being written.
	line: String,
	frames: Vec<Frame>,
	/// The next number of each open list, or none for a bulleted one.
	lists: Vec<Option<u64>>,
	/// Where each open link's text starts in `line`, and where it points.
	links: Vec<(usize, String)>,
	/// Footnote numbers by label, in the order the scene first mentions them.
	notes: HashMap<String, usize>,
	rows: Vec<Vec<String>>,
	cells: Vec<String>,
}

impl Writer {
	fn event(&mut self, event: Event) {
		match event {
			Event::Start(tag) => self.start(tag),
			Event::End(end) => self.end(end),
			Event::Text(text) | Event::Code(text) => self.line.push_str(&text),
			Event::SoftBreak => self.line.push(' '),
			Event::HardBreak => self.line.push('\n'),
			Event::FootnoteReference(label) => {
				let number = self.note(&label);
				self.line.push_str(&format!("[{number}]"));
			}
			Event::Rule => {
				self.close();
				self.blocks.push(scene_break());
			}
			// HTML in a scene is a comment the writer left for themselves.
			_ => {}
		}
	}

	fn start(&mut self, tag: Tag) {
		match tag {
			Tag::Paragraph | Tag::Heading { .. } | Tag::Table(_) => self.close(),
			Tag::BlockQuote(_) | Tag::CodeBlock(_) => {
				self.open("    ".to_owned(), "    ".to_owned())
			}
			Tag::List(start) => {
				self.close();
				self.lists.push(start);
			}
			Tag::Item => {
				let marker = match self.lists.last_mut() {
					Some(Some(number)) => {
						*number += 1;
						format!("{}. ", *number - 1)
					}
					_ => "• ".to_owned(),
				};
				self.hang(marker);
			}
			Tag::FootnoteDefinition(label) => {
				let marker = format!("[{}] ", self.note(&label));
				self.hang(marker);
			}
			Tag::Link { dest_url, .. } => {
				self.links.push((self.line.len(), dest_url.into_string()))
			}
			_ => {}
		}
	}

	fn end(&mut self, end: TagEnd) {
		match end {
			TagEnd::Paragraph | TagEnd::Heading(_) => self.close(),
			TagEnd::BlockQuote(_)
			| TagEnd::CodeBlock
			| TagEnd::Item
			| TagEnd::FootnoteDefinition => {
				self.close();
				self.frames.pop();
			}
			TagEnd::List(_) => {
				self.close();
				self.lists.pop();
			}
			TagEnd::TableCell => {
				let cell = take(&mut self.line);
				self.cells.push(cell.trim().to_owned());
			}
			TagEnd::TableHead | TagEnd::TableRow => self.rows.push(take(&mut self.cells)),
			TagEnd::Table => {
				self.line = table(&take(&mut self.rows));
				self.close();
			}
			TagEnd::Link => {
				if let Some((at, url)) = self.links.pop() {
					let shown = &self.line[at..];
					let said = url.is_empty()
						|| url.starts_with('#')
						|| shown == url || url.strip_prefix("mailto:") == Some(shown);
					if !said {
						self.line.push_str(&format!(" ({url})"));
					}
				}
			}
			_ => {}
		}
	}

	fn open(&mut self, first: String, rest: String) {
		self.close();
		self.frames.push(Frame { first, rest });
	}

	/// Opens a frame whose first line starts with `marker` and whose later lines
	/// line up with the text after it.
	fn hang(&mut self, marker: String) {
		let rest = " ".repeat(marker.chars().count());
		self.open(marker, rest);
	}

	/// Ends the block being written, starting each of its lines with what the
	/// frames around it ask for.
	fn close(&mut self) {
		let text = take(&mut self.line);
		let text = text.trim_end_matches('\n');
		if text.trim().is_empty() {
			return;
		}

		let mut lines = Vec::new();
		for line in text.split('\n') {
			let mut prefix = String::new();
			for frame in &mut self.frames {
				prefix.push_str(&frame.first);
				frame.first.clone_from(&frame.rest);
			}
			lines.push(format!("{prefix}{line}").trim_end().to_owned());
		}
		self.blocks.push(lines.join("\n"));
	}

	/// A footnote's number, given the first time the scene mentions its label.
	fn note(&mut self, label: &str) -> usize {
		let next = self.notes.len() + 1;
		*self.notes.entry(label.to_owned()).or_insert(next)
	}
}

/// A table's rows with each column padded to its widest cell.
fn table(rows: &[Vec<String>]) -> String {
	let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
	let widths: Vec<usize> = (0..columns)
		.map(|at| {
			rows.iter()
				.filter_map(|row| row.get(at))
				.map(|cell| cell.chars().count())
				.max()
				.unwrap_or(0)
		})
		.collect();

	rows.iter()
		.map(|row| {
			let cells: Vec<String> = row
				.iter()
				.zip(&widths)
				.map(|(cell, &width)| format!("{cell:width$}"))
				.collect();
			cells.join("  ").trim_end().to_owned()
		})
		.collect::<Vec<_>>()
		.join("\n")
}

#[cfg(test)]
mod tests {
	use uuid::Uuid;

	use super::*;
	use crate::book::Part;

	fn scene(title: &str) -> Scene {
		Scene {
			id: Uuid::new_v4(),
			title: title.to_owned(),
			path: format!("Manuscript/{title}.md"),
		}
	}

	fn chapter(title: &str, scenes: Vec<Scene>) -> Chapter {
		Chapter {
			id: Uuid::new_v4(),
			title: title.to_owned(),
			scenes,
		}
	}

	/// A `#` in the middle of a 72-column line.
	fn centred_hash() -> String {
		format!("{}#", " ".repeat(35))
	}

	#[test]
	fn a_book_reads_as_titles_and_prose_with_no_markup() {
		let dedication = scene("Dedication");
		let ship = scene("Ship");
		let stranger = scene("Stranger");
		let home = scene("Home");
		let prose = Prose::from([
			(dedication.id, "For *Penelope*.".to_owned()),
			(ship.id, "The ship came in.".to_owned()),
			(stranger.id, "Nobody knew him.".to_owned()),
			(home.id, "He was home.".to_owned()),
		]);
		let compiled = Compiled {
			front: vec![dedication],
			body: vec![
				Division::Part(Part {
					id: Uuid::new_v4(),
					title: "Book One".to_owned(),
					chapters: vec![chapter("Arrival", vec![ship, stranger])],
				}),
				Division::Chapter(chapter("Home", vec![home])),
			],
			back: Vec::new(),
		};
		let mut book = Book::new("Ithaca");
		book.author = "Ada Lovelace".to_owned();

		assert_eq!(
			text(&book, &compiled, &prose),
			format!(
				"Ithaca\n\nby Ada Lovelace\n\n\nDedication\n\nFor Penelope.\n\n\nBook One\n\n\n\
				 Arrival\n\nThe ship came in.\n\n{}\n\nNobody knew him.\n\n\nHome\n\nHe was home.\n",
				centred_hash()
			)
		);
	}

	#[test]
	fn a_book_with_no_title_opens_on_its_first_heading() {
		let home = scene("Home");
		let prose = Prose::from([(home.id, "He was home.".to_owned())]);
		let compiled = Compiled {
			body: vec![Division::Chapter(chapter("Home", vec![home]))],
			..Compiled::default()
		};

		assert_eq!(
			text(&Book::new(""), &compiled, &prose),
			"Home\n\nHe was home.\n"
		);
		assert_eq!(
			text(&Book::new(""), &Compiled::default(), &Prose::new()),
			""
		);
	}

	#[test]
	fn inline_markup_is_taken_out() {
		assert_eq!(
			plain("Some *italic*, **bold**, ~~struck~~ and `set` text,\nand a hard\\\nbreak."),
			"Some italic, bold, struck and set text, and a hard\nbreak."
		);
	}

	#[test]
	fn a_heading_in_a_scene_is_a_line_of_its_own() {
		assert_eq!(
			plain("## A letter\n\nDear Odysseus."),
			"A letter\n\nDear Odysseus."
		);
	}

	#[test]
	fn a_break_inside_a_scene_is_a_centred_hash() {
		assert_eq!(
			plain("Before.\n\n---\n\nAfter."),
			format!("Before.\n\n{}\n\nAfter.", centred_hash())
		);
	}

	#[test]
	fn quotes_and_code_are_indented() {
		assert_eq!(
			plain("> Sing to me.\n>\n> Of the man.\n\n```\nlet x = 1;\n\nlet y = 2;\n```"),
			"    Sing to me.\n\n    Of the man.\n\n    let x = 1;\n\n    let y = 2;"
		);
	}

	#[test]
	fn lists_keep_their_bullets_and_numbers() {
		assert_eq!(
			plain("- Bread\n- Wine\n  - Red\n\n3. Sail\n4. Row"),
			"• Bread\n\n• Wine\n\n  • Red\n\n3. Sail\n\n4. Row"
		);
		assert_eq!(
			plain("1. First\n\n   More of it."),
			"1. First\n\n   More of it."
		);
	}

	#[test]
	fn a_link_keeps_its_address_after_its_text() {
		assert_eq!(
			plain(
				"See [the chart](https://example.com/chart), <https://example.com> or <ada@example.com>."
			),
			"See the chart (https://example.com/chart), https://example.com or ada@example.com."
		);
	}

	#[test]
	fn footnotes_are_numbered_in_the_order_they_are_mentioned() {
		assert_eq!(
			plain("Once[^late], twice[^early].\n\n[^early]: First.\n\n[^late]: Second."),
			"Once[1], twice[2].\n\n[2] First.\n\n[1] Second."
		);
	}

	#[test]
	fn a_table_lines_its_columns_up() {
		assert_eq!(
			plain("| Name | Age |\n| --- | --- |\n| Penelope | 40 |\n| Telemachus | 20 |"),
			"Name        Age\nPenelope    40\nTelemachus  20"
		);
	}

	#[test]
	fn a_comment_is_left_out() {
		assert_eq!(
			plain("<!-- Check the date. -->\n\nThe ship came in."),
			"The ship came in."
		);
	}
}
