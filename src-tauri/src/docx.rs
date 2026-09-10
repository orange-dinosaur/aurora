//! The book as a Word document in standard manuscript format, which is what an
//! agent or an editor expects to be sent: 12pt Times New Roman, double-spaced,
//! one-inch margins, a half-inch indent at the start of every paragraph, the
//! writer's surname, the title and the page number at the top of every page,
//! and each chapter on a fresh page a third of the way down.

use std::collections::BTreeMap;
use std::io;
use std::mem;
use std::slice;

use pulldown_cmark::{Alignment, Event, Parser, Tag, TagEnd};

use crate::archive::Archive;
use crate::book::{Chapter, Compiled, Division, Prose, Scene, broken, told};
use crate::epub::{escaped, options, title_of};
use crate::project::Book;

/// WordprocessingML's namespace, and the one relationships are named in.
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The width of the text on a Letter page inside one-inch margins, which a
/// table's columns share.
const TEXT_WIDTH: usize = 9360;

/// How far each level of a list is indented.
const LIST_INDENT: usize = 720;

/// The bullet for each level of a list, round again from the fourth.
const BULLETS: [&str; 3] = ["•", "◦", "▪"];

/// What each part of the package is. Word will not open a file with a part it
/// cannot put a type to.
const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
	<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
	<Default Extension="xml" ContentType="application/xml"/>
	<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
	<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
	<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>
	<Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
	<Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/>
	<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
</Types>
"#;

/// Where the package starts: the document, and what the file says about itself.
const PACKAGE_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
	<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
	<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
</Relationships>
"#;

/// The manuscript's look. Lengths are in twentieths of a point, so 1440 is an
/// inch. A chapter title sits 3840 below the top margin, which on a Letter
/// page is a third of the way down, and the book's title 6480, halfway. The
/// schema fixes the order of everything inside a `w:pPr` and a `w:rPr`.
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
	<w:docDefaults>
		<w:rPrDefault>
			<w:rPr>
				<w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:eastAsia="Times New Roman" w:cs="Times New Roman"/>
				<w:sz w:val="24"/>
				<w:szCs w:val="24"/>
			</w:rPr>
		</w:rPrDefault>
		<w:pPrDefault>
			<w:pPr>
				<w:spacing w:before="0" w:after="0" w:line="480" w:lineRule="auto"/>
			</w:pPr>
		</w:pPrDefault>
	</w:docDefaults>
	<w:style w:type="paragraph" w:default="1" w:styleId="Normal">
		<w:name w:val="Normal"/>
		<w:pPr>
			<w:ind w:firstLine="720"/>
		</w:pPr>
	</w:style>
	<w:style w:type="paragraph" w:styleId="BookTitle">
		<w:name w:val="Book Title"/>
		<w:basedOn w:val="Normal"/>
		<w:next w:val="Byline"/>
		<w:pPr>
			<w:spacing w:before="6480"/>
			<w:ind w:firstLine="0"/>
			<w:jc w:val="center"/>
		</w:pPr>
	</w:style>
	<w:style w:type="paragraph" w:styleId="Byline">
		<w:name w:val="Byline"/>
		<w:basedOn w:val="Normal"/>
		<w:pPr>
			<w:ind w:firstLine="0"/>
			<w:jc w:val="center"/>
		</w:pPr>
	</w:style>
	<w:style w:type="paragraph" w:styleId="ChapterTitle">
		<w:name w:val="Chapter Title"/>
		<w:basedOn w:val="Normal"/>
		<w:next w:val="Normal"/>
		<w:pPr>
			<w:keepNext/>
			<w:pageBreakBefore/>
			<w:spacing w:before="3840"/>
			<w:ind w:firstLine="0"/>
			<w:jc w:val="center"/>
		</w:pPr>
	</w:style>
	<w:style w:type="paragraph" w:styleId="Subheading">
		<w:name w:val="Subheading"/>
		<w:basedOn w:val="Normal"/>
		<w:next w:val="Normal"/>
		<w:pPr>
			<w:keepNext/>
			<w:ind w:firstLine="0"/>
		</w:pPr>
		<w:rPr>
			<w:b/>
		</w:rPr>
	</w:style>
	<w:style w:type="paragraph" w:styleId="SceneBreak">
		<w:name w:val="Scene Break"/>
		<w:basedOn w:val="Normal"/>
		<w:next w:val="Normal"/>
		<w:pPr>
			<w:ind w:firstLine="0"/>
			<w:jc w:val="center"/>
		</w:pPr>
	</w:style>
	<w:style w:type="paragraph" w:styleId="Quote">
		<w:name w:val="Quote"/>
		<w:basedOn w:val="Normal"/>
		<w:pPr>
			<w:ind w:left="720" w:right="720" w:firstLine="0"/>
		</w:pPr>
	</w:style>
	<w:style w:type="paragraph" w:styleId="ListParagraph">
		<w:name w:val="List Paragraph"/>
		<w:basedOn w:val="Normal"/>
		<w:pPr>
			<w:ind w:firstLine="0"/>
		</w:pPr>
	</w:style>
	<w:style w:type="paragraph" w:styleId="CodeBlock">
		<w:name w:val="Code Block"/>
		<w:basedOn w:val="Normal"/>
		<w:pPr>
			<w:spacing w:line="240" w:lineRule="auto"/>
			<w:ind w:firstLine="0"/>
		</w:pPr>
		<w:rPr>
			<w:rFonts w:ascii="Courier New" w:hAnsi="Courier New" w:cs="Courier New"/>
		</w:rPr>
	</w:style>
	<w:style w:type="paragraph" w:styleId="TableText">
		<w:name w:val="Table Text"/>
		<w:basedOn w:val="Normal"/>
		<w:pPr>
			<w:spacing w:line="240" w:lineRule="auto"/>
			<w:ind w:firstLine="0"/>
		</w:pPr>
	</w:style>
	<w:style w:type="paragraph" w:styleId="FootnoteText">
		<w:name w:val="footnote text"/>
		<w:basedOn w:val="Normal"/>
		<w:pPr>
			<w:spacing w:line="240" w:lineRule="auto"/>
			<w:ind w:firstLine="0"/>
		</w:pPr>
		<w:rPr>
			<w:sz w:val="20"/>
			<w:szCs w:val="20"/>
		</w:rPr>
	</w:style>
	<w:style w:type="paragraph" w:styleId="Header">
		<w:name w:val="header"/>
		<w:basedOn w:val="Normal"/>
		<w:pPr>
			<w:spacing w:line="240" w:lineRule="auto"/>
			<w:ind w:firstLine="0"/>
			<w:jc w:val="right"/>
		</w:pPr>
	</w:style>
	<w:style w:type="character" w:styleId="FootnoteReference">
		<w:name w:val="footnote reference"/>
		<w:rPr>
			<w:vertAlign w:val="superscript"/>
		</w:rPr>
	</w:style>
	<w:style w:type="character" w:styleId="InlineCode">
		<w:name w:val="Inline Code"/>
		<w:rPr>
			<w:rFonts w:ascii="Courier New" w:hAnsi="Courier New" w:cs="Courier New"/>
		</w:rPr>
	</w:style>
	<w:style w:type="character" w:styleId="Hyperlink">
		<w:name w:val="Hyperlink"/>
		<w:rPr>
			<w:u w:val="single"/>
		</w:rPr>
	</w:style>
</w:styles>
"#;

/// How a footnote's number is set, where it is called for and where it opens.
const NOTE_NUMBER: &str = r#"<w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr>"#;

/// How a run of text is set. Each mark is a depth rather than a flag, since
/// emphasis can sit inside emphasis.
#[derive(Debug, Clone, Copy, Default)]
struct Marks {
	italic: u32,
	bold: u32,
	struck: u32,
	link: u32,
	code: bool,
}

/// The book as a manuscript.
pub fn docx(book: &Book, compiled: &Compiled, prose: &Prose) -> io::Result<Vec<u8>> {
	let mut manuscript = Manuscript {
		body: title_page(book),
		..Manuscript::default()
	};
	for scene in &compiled.front {
		manuscript.piece(scene, prose);
	}
	for division in &compiled.body {
		match division {
			Division::Part(part) => {
				manuscript.body.push(heading(&part.title));
				for chapter in &part.chapters {
					manuscript.chapter(chapter, prose);
				}
			}
			Division::Chapter(chapter) => manuscript.chapter(chapter, prose),
		}
	}
	for scene in &compiled.back {
		manuscript.piece(scene, prose);
	}

	let rels = document_rels(&manuscript.links);
	let lists = numbering(&manuscript.lists);
	let notes = footnotes(&manuscript.footnotes);
	let body = document(&manuscript.body);

	let mut archive = Archive::default();
	archive.deflate("[Content_Types].xml", CONTENT_TYPES.as_bytes())?;
	archive.deflate("_rels/.rels", PACKAGE_RELS.as_bytes())?;
	archive.deflate("docProps/core.xml", core(book).as_bytes())?;
	archive.deflate("word/_rels/document.xml.rels", rels.as_bytes())?;
	archive.deflate("word/styles.xml", STYLES.as_bytes())?;
	archive.deflate("word/header1.xml", header(book).as_bytes())?;
	archive.deflate("word/numbering.xml", lists.as_bytes())?;
	archive.deflate("word/footnotes.xml", notes.as_bytes())?;
	archive.deflate("word/document.xml", body.as_bytes())?;
	archive.finish()
}

/// The manuscript as it is written: the body, and what the body points at in
/// the other parts of the package.
#[derive(Default)]
struct Manuscript {
	body: Vec<String>,
	footnotes: Vec<String>,
	lists: Vec<List>,
	links: Vec<String>,
	/// How many footnote numbers have been given out.
	notes_given: usize,
}

/// One list, as the numbering part defines it.
struct List {
	/// Where an ordered list starts. A list of bullets has none.
	start: Option<u64>,
	depth: usize,
}

impl Manuscript {
	/// A chapter: its title on a fresh page, then its scenes parted by breaks.
	fn chapter(&mut self, chapter: &Chapter, prose: &Prose) {
		self.body.push(heading(&chapter.title));
		self.write(&broken(&chapter.scenes, prose).join("\n\n"));
	}

	/// A piece of front or back matter, which is laid out as a chapter is.
	fn piece(&mut self, scene: &Scene, prose: &Prose) {
		self.body.push(heading(&scene.title));
		self.write(&told(slice::from_ref(scene), prose).join("\n\n"));
	}

	/// Adds `markdown` to the body as manuscript paragraphs.
	fn write(&mut self, markdown: &str) {
		let mut converter = Converter::new(self);
		for event in Parser::new_ext(markdown, options()) {
			converter.event(event);
		}
		let paragraphs = converter.finish();
		self.body.extend(paragraphs);
	}

	/// A new list at `depth`, and the number the numbering part knows it by.
	fn list(&mut self, start: Option<u64>, depth: usize) -> usize {
		self.lists.push(List { start, depth });
		self.lists.len()
	}

	/// The relationship a link to `url` goes through.
	fn link(&mut self, url: &str) -> String {
		self.links.push(url.to_owned());
		format!("link{}", self.links.len())
	}

	/// The next footnote's number. Word keeps -1 and 0 for the lines that
	/// part the footnotes from the text.
	fn note(&mut self) -> usize {
		self.notes_given += 1;
		self.notes_given
	}
}

/// A block the converter is inside.
enum Block {
	Quote,
	Heading,
	Code,
	/// A list, known to the numbering part as `num`.
	List {
		num: usize,
		depth: usize,
	},
	/// An item of list `num`, and whether its first paragraph, the one that
	/// carries the bullet or the number, is written yet.
	Item {
		num: usize,
		depth: usize,
		numbered: bool,
	},
	Cell {
		align: &'static str,
	},
	/// A footnote's text, and whether the paragraph that opens with its
	/// number is still to come.
	Footnote {
		id: usize,
		first: bool,
	},
}

/// A footnote label, the number it was given, and whether its text has come.
struct Note {
	id: usize,
	written: bool,
}

/// Turns one run of Markdown into manuscript paragraphs.
struct Converter<'a> {
	manuscript: &'a mut Manuscript,
	/// Where finished paragraphs go, innermost last. The first is the body; a
	/// table cell and a footnote gather their own.
	sinks: Vec<Vec<String>>,
	blocks: Vec<Block>,
	table: Option<Table>,
	notes: BTreeMap<String, Note>,
	/// Whether each link still open became a hyperlink.
	links: Vec<bool>,
	runs: String,
	marks: Marks,
}

impl<'a> Converter<'a> {
	fn new(manuscript: &'a mut Manuscript) -> Self {
		Self {
			manuscript,
			sinks: vec![Vec::new()],
			blocks: Vec::new(),
			table: None,
			notes: BTreeMap::new(),
			links: Vec::new(),
			runs: String::new(),
			marks: Marks::default(),
		}
	}

	fn event(&mut self, event: Event) {
		match event {
			Event::Start(tag) => self.start(tag),
			Event::End(end) => self.end(end),
			Event::Text(text) => self.text(&text),
			Event::Code(code) => {
				let marks = Marks {
					code: true,
					..self.marks
				};
				self.runs.push_str(&run(&code, marks));
			}
			Event::SoftBreak => self.text(" "),
			Event::HardBreak => self.runs.push_str("<w:r><w:br/></w:r>"),
			Event::FootnoteReference(label) => {
				let id = self.note(&label);
				let called =
					format!(r#"<w:r>{NOTE_NUMBER}<w:footnoteReference w:id="{id}"/></w:r>"#);
				self.runs.push_str(&called);
			}
			Event::Rule => {
				self.close();
				let mark = paragraph("SceneBreak", &run("#", Marks::default()));
				self.sink().push(mark);
			}
			// Raw HTML is left out, as it is from the EPUB.
			_ => {}
		}
	}

	fn start(&mut self, tag: Tag) {
		match tag {
			Tag::Emphasis => self.marks.italic += 1,
			Tag::Strong => self.marks.bold += 1,
			Tag::Strikethrough => self.marks.struck += 1,
			Tag::Link { dest_url, .. } => self.open_link(&dest_url),
			// An image keeps its description, which arrives as text.
			Tag::Image { .. } => {}
			block => {
				self.close();
				self.open(block);
			}
		}
	}

	fn end(&mut self, end: TagEnd) {
		match end {
			TagEnd::Emphasis => self.marks.italic -= 1,
			TagEnd::Strong => self.marks.bold -= 1,
			TagEnd::Strikethrough => self.marks.struck -= 1,
			TagEnd::Link => {
				if self.links.pop() == Some(true) {
					self.runs.push_str("</w:hyperlink>");
				}
				self.marks.link -= 1;
			}
			TagEnd::Image => {}
			block => {
				self.close();
				self.shut(block);
			}
		}
	}

	/// Opens a link. One inside a footnote stays text, since a hyperlink there
	/// would need relationships of the footnotes' own.
	fn open_link(&mut self, url: &str) {
		let linked = !url.is_empty() && !self.in_footnote();
		if linked {
			let id = self.manuscript.link(url);
			self.runs.push_str(&format!(r#"<w:hyperlink r:id="{id}">"#));
		}
		self.links.push(linked);
		self.marks.link += 1;
	}

	fn open(&mut self, tag: Tag) {
		let block = match tag {
			Tag::BlockQuote(_) => Block::Quote,
			Tag::Heading { .. } => Block::Heading,
			Tag::CodeBlock(_) => Block::Code,
			Tag::List(start) => {
				let depth = self.depth();
				let num = self.manuscript.list(start, depth);
				Block::List { num, depth }
			}
			Tag::Item => {
				let (num, depth) = self.list().unwrap_or((0, 0));
				Block::Item {
					num,
					depth,
					numbered: false,
				}
			}
			Tag::Table(aligns) => {
				self.table = Some(Table::new(&aligns));
				return;
			}
			Tag::TableHead => {
				if let Some(table) = &mut self.table {
					table.head = true;
				}
				return;
			}
			Tag::TableCell => {
				let align = self.table.as_ref().map_or("left", Table::align);
				self.sinks.push(Vec::new());
				Block::Cell { align }
			}
			Tag::FootnoteDefinition(label) => {
				let id = self.note(&label);
				if let Some(note) = self.notes.get_mut(&*label) {
					note.written = true;
				}
				self.sinks.push(Vec::new());
				Block::Footnote { id, first: true }
			}
			// A paragraph, a table row, and anything the options leave off.
			_ => return,
		};
		self.blocks.push(block);
	}

	fn shut(&mut self, end: TagEnd) {
		match end {
			TagEnd::BlockQuote(_)
			| TagEnd::Heading(_)
			| TagEnd::CodeBlock
			| TagEnd::List(_)
			| TagEnd::Item => {
				self.blocks.pop();
			}
			TagEnd::TableCell => {
				self.blocks.pop();
				let paragraphs = self.sinks.pop().unwrap_or_default();
				if let Some(table) = &mut self.table {
					table.cell(&paragraphs);
				}
			}
			TagEnd::TableHead | TagEnd::TableRow => {
				if let Some(table) = &mut self.table {
					table.row();
				}
			}
			TagEnd::Table => {
				if let Some(table) = self.table.take() {
					let finished = table.finish();
					self.sink().push(finished);
				}
			}
			TagEnd::FootnoteDefinition => {
				let paragraphs = self.sinks.pop().unwrap_or_default();
				if let Some(Block::Footnote { id, .. }) = self.blocks.pop() {
					self.manuscript.footnotes.push(footnote(id, &paragraphs));
				}
			}
			_ => {}
		}
	}

	fn text(&mut self, text: &str) {
		if !matches!(self.blocks.last(), Some(Block::Code)) {
			self.runs.push_str(&run(text, self.marks));
			return;
		}
		// Each line of code is a paragraph of its own, so its breaks survive.
		for line in text.split_inclusive('\n') {
			self.runs
				.push_str(&run(line.trim_end_matches('\n'), self.marks));
			if line.ends_with('\n') {
				self.close();
			}
		}
	}

	/// Ends the paragraph being gathered, if it has anything in it, and sends
	/// it where it belongs.
	fn close(&mut self) {
		if self.runs.is_empty() {
			return;
		}
		let runs = mem::take(&mut self.runs);
		let (properties, opening) = self.shape();
		let paragraph = format!("<w:p><w:pPr>{properties}</w:pPr>{opening}{runs}</w:p>");
		self.sink().push(paragraph);
	}

	/// How the paragraph about to be written is set, from the blocks it sits
	/// in, and anything it has to open with.
	fn shape(&mut self) -> (String, String) {
		let mut opening = String::new();
		let footnote = self.blocks.iter_mut().rev().find_map(|block| match block {
			Block::Footnote { first, .. } => Some(first),
			_ => None,
		});
		if let Some(first) = footnote.filter(|first| **first) {
			*first = false;
			opening = format!(
				"<w:r>{NOTE_NUMBER}<w:footnoteRef/></w:r>{}",
				run(" ", Marks::default())
			);
		}

		for block in self.blocks.iter_mut().rev() {
			let properties = match block {
				Block::Code => style("CodeBlock"),
				Block::Heading => style("Subheading"),
				Block::Cell { align } => cell_text(align),
				Block::Item {
					num,
					depth,
					numbered,
				} => item(*num, *depth, numbered),
				Block::Quote => style("Quote"),
				Block::Footnote { .. } => style("FootnoteText"),
				Block::List { .. } => continue,
			};
			return (properties, opening);
		}
		(style("Normal"), opening)
	}

	/// The number of the footnote `label` names, given out the first time the
	/// label turns up.
	fn note(&mut self, label: &str) -> usize {
		if let Some(note) = self.notes.get(label) {
			return note.id;
		}
		let id = self.manuscript.note();
		self.notes
			.insert(label.to_owned(), Note { id, written: false });
		id
	}

	fn in_footnote(&self) -> bool {
		self.blocks
			.iter()
			.any(|block| matches!(block, Block::Footnote { .. }))
	}

	/// How many lists deep the converter is.
	fn depth(&self) -> usize {
		self.blocks
			.iter()
			.filter(|block| matches!(block, Block::List { .. }))
			.count()
	}

	/// The innermost list the converter is in, as its number and its depth.
	fn list(&self) -> Option<(usize, usize)> {
		self.blocks.iter().rev().find_map(|block| match block {
			Block::List { num, depth } => Some((*num, *depth)),
			_ => None,
		})
	}

	fn sink(&mut self) -> &mut Vec<String> {
		self.sinks.last_mut().expect("the body is always there")
	}

	/// Closes what is still open and answers with the body's paragraphs. A
	/// footnote called for but never written still gets a note, so the call
	/// points at something.
	fn finish(mut self) -> Vec<String> {
		self.close();
		for note in self.notes.values().filter(|note| !note.written) {
			self.manuscript.footnotes.push(footnote(note.id, &[]));
		}
		self.sinks.swap_remove(0)
	}
}

/// A table being gathered, row by row.
struct Table {
	aligns: Vec<&'static str>,
	rows: Vec<String>,
	cells: Vec<String>,
	head: bool,
}

impl Table {
	fn new(aligns: &[Alignment]) -> Self {
		let aligns = aligns
			.iter()
			.map(|align| match align {
				Alignment::Center => "center",
				Alignment::Right => "right",
				Alignment::None | Alignment::Left => "left",
			})
			.collect();
		Self {
			aligns,
			rows: Vec::new(),
			cells: Vec::new(),
			head: false,
		}
	}

	/// How the cell about to open is aligned.
	fn align(&self) -> &'static str {
		self.aligns.get(self.cells.len()).copied().unwrap_or("left")
	}

	/// The width each column is given: the text's width, shared out.
	fn width(&self) -> usize {
		TEXT_WIDTH / self.aligns.len().max(1)
	}

	/// Adds a cell holding `paragraphs`. Word will not open a cell with none.
	fn cell(&mut self, paragraphs: &[String]) {
		let width = self.width();
		let text = if paragraphs.is_empty() {
			let style = style("TableText");
			format!("<w:p><w:pPr>{style}</w:pPr></w:p>")
		} else {
			paragraphs.concat()
		};
		let properties = format!(r#"<w:tcPr><w:tcW w:w="{width}" w:type="dxa"/></w:tcPr>"#);
		self.cells.push(format!("<w:tc>{properties}{text}</w:tc>"));
	}

	/// Ends a row. The head row comes again at the top of every page the
	/// table runs onto.
	fn row(&mut self) {
		let repeat = if self.head {
			"<w:trPr><w:tblHeader/></w:trPr>"
		} else {
			""
		};
		let cells = mem::take(&mut self.cells).concat();
		self.rows.push(format!("<w:tr>{repeat}{cells}</w:tr>"));
		self.head = false;
	}

	/// The whole table, as wide as its columns and ruled with thin lines.
	fn finish(self) -> String {
		let width = self.width();
		let grid = format!(r#"<w:gridCol w:w="{width}"/>"#).repeat(self.aligns.len());
		let rules: String = ["top", "left", "bottom", "right", "insideH", "insideV"]
			.iter()
			.map(|edge| {
				format!(r#"<w:{edge} w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#)
			})
			.collect();
		let wide = r#"<w:tblW w:w="0" w:type="auto"/>"#;
		let properties = format!("<w:tblPr>{wide}<w:tblBorders>{rules}</w:tblBorders></w:tblPr>");
		let rows = self.rows.concat();
		format!("<w:tbl>{properties}<w:tblGrid>{grid}</w:tblGrid>{rows}</w:tbl>")
	}
}

/// The first page: the title halfway down, and who wrote it.
fn title_page(book: &Book) -> Vec<String> {
	let plain = Marks::default();
	let mut page = vec![paragraph("BookTitle", &run(title_of(book), plain))];
	let author = book.author.trim();
	if !author.is_empty() {
		page.push(paragraph("Byline", &run(&format!("by {author}"), plain)));
	}
	page
}

/// A title on a fresh page, a third of the way down.
fn heading(title: &str) -> String {
	paragraph("ChapterTitle", &run(title, Marks::default()))
}

/// Paragraph properties naming `name` as the style.
fn style(name: &str) -> String {
	format!(r#"<w:pStyle w:val="{name}"/>"#)
}

/// A table cell's paragraph, aligned as its column is.
fn cell_text(align: &str) -> String {
	let style = style("TableText");
	format!(r#"{style}<w:jc w:val="{align}"/>"#)
}

/// A list item's paragraph. The first carries the bullet or the number, and
/// any after it sit at the same indent without one.
fn item(num: usize, depth: usize, numbered: &mut bool) -> String {
	let style = style("ListParagraph");
	if *numbered {
		let left = LIST_INDENT * (depth + 1);
		return format!(r#"{style}<w:ind w:left="{left}"/>"#);
	}
	*numbered = true;
	format!(r#"{style}<w:numPr><w:ilvl w:val="{depth}"/><w:numId w:val="{num}"/></w:numPr>"#)
}

/// A footnote holding `paragraphs`, or its number alone when it has none.
fn footnote(id: usize, paragraphs: &[String]) -> String {
	let text = if paragraphs.is_empty() {
		let style = style("FootnoteText");
		format!("<w:p><w:pPr>{style}</w:pPr><w:r>{NOTE_NUMBER}<w:footnoteRef/></w:r></w:p>")
	} else {
		paragraphs.concat()
	};
	format!(r#"<w:footnote w:id="{id}">{text}</w:footnote>"#)
}

/// A paragraph in `name`'s style holding `runs`.
fn paragraph(name: &str, runs: &str) -> String {
	let style = style(name);
	format!("<w:p><w:pPr>{style}</w:pPr>{runs}</w:p>")
}

/// A run of `text`, set as `marks` say.
fn run(text: &str, marks: Marks) -> String {
	// The schema fixes this order too.
	let mut set = String::new();
	if marks.code {
		set.push_str(r#"<w:rStyle w:val="InlineCode"/>"#);
	} else if marks.link > 0 {
		set.push_str(r#"<w:rStyle w:val="Hyperlink"/>"#);
	}
	if marks.bold > 0 {
		set.push_str("<w:b/>");
	}
	if marks.italic > 0 {
		set.push_str("<w:i/>");
	}
	if marks.struck > 0 {
		set.push_str("<w:strike/>");
	}
	if !set.is_empty() {
		set = format!("<w:rPr>{set}</w:rPr>");
	}
	let text = escaped(text);
	format!(r#"<w:r>{set}<w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

/// The document: every paragraph, then the page they are set on.
fn document(paragraphs: &[String]) -> String {
	let body = paragraphs.join("\n");
	format!(
		r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="{W}" xmlns:r="{R}">
<w:body>
{body}
<w:sectPr>
	<w:headerReference w:type="default" r:id="rId2"/>
	<w:pgSz w:w="12240" w:h="15840"/>
	<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>
</w:sectPr>
</w:body>
</w:document>
"#
	)
}

/// What the document draws on: its styles, its header, its lists, its
/// footnotes, and the page behind every link.
fn document_rels(links: &[String]) -> String {
	let mut rels = vec![
		relationship("rId1", "styles", "styles.xml"),
		relationship("rId2", "header", "header1.xml"),
		relationship("rId3", "numbering", "numbering.xml"),
		relationship("rId4", "footnotes", "footnotes.xml"),
	];
	for (at, url) in links.iter().enumerate() {
		rels.push(relationship(&format!("link{}", at + 1), "hyperlink", url));
	}
	let rels = rels.join("\n\t");
	format!(
		r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
	{rels}
</Relationships>
"#
	)
}

/// One of the document's relationships: to another part of the package or,
/// for a link, to a page outside it.
fn relationship(id: &str, kind: &str, target: &str) -> String {
	let target = escaped(target);
	let mode = if kind == "hyperlink" {
		r#" TargetMode="External""#
	} else {
		""
	};
	format!(r#"<Relationship Id="{id}" Type="{R}/{kind}" Target="{target}"{mode}/>"#)
}

/// The numbering part: one definition for bullets and one for numbers, and a
/// list of its own for every list in the book, so each numbered list starts
/// again from its first number.
fn numbering(lists: &[List]) -> String {
	let bullets: String = (0..9)
		.map(|at| level(at, "bullet", BULLETS[at % BULLETS.len()]))
		.collect();
	let numbers: String = (0..9)
		.map(|at| level(at, "decimal", &format!("%{}.", at + 1)))
		.collect();
	let nums: String = lists
		.iter()
		.enumerate()
		.map(|(at, list)| num(at + 1, list))
		.collect();
	format!(
		r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="{W}">
<w:abstractNum w:abstractNumId="0">{bullets}</w:abstractNum>
<w:abstractNum w:abstractNumId="1">{numbers}</w:abstractNum>
{nums}
</w:numbering>
"#
	)
}

/// One level of a list definition, indented a step further than the last.
fn level(at: usize, format: &str, text: &str) -> String {
	let left = LIST_INDENT * (at + 1);
	format!(
		r#"<w:lvl w:ilvl="{at}">
	<w:start w:val="1"/>
	<w:numFmt w:val="{format}"/>
	<w:lvlText w:val="{text}"/>
	<w:lvlJc w:val="left"/>
	<w:pPr><w:ind w:left="{left}" w:hanging="360"/></w:pPr>
</w:lvl>"#
	)
}

/// A list of its own, starting at its first number at the level it sits at.
fn num(id: usize, list: &List) -> String {
	let (abstract_id, start) = match list.start {
		Some(start) => (1, start),
		None => (0, 1),
	};
	let depth = list.depth;
	format!(
		r#"<w:num w:numId="{id}">
	<w:abstractNumId w:val="{abstract_id}"/>
	<w:lvlOverride w:ilvl="{depth}"><w:startOverride w:val="{start}"/></w:lvlOverride>
</w:num>"#
	)
}

/// The footnotes part: Word's two separators, then every note in the book.
fn footnotes(notes: &[String]) -> String {
	let notes = notes.join("\n");
	format!(
		r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:footnotes xmlns:w="{W}" xmlns:r="{R}">
<w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>
<w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>
{notes}
</w:footnotes>
"#
	)
}

/// What the file says about itself, for the title a word processor shows.
fn core(book: &Book) -> String {
	let title = escaped(title_of(book));
	let creator = escaped(book.author.trim());
	format!(
		r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/">
	<dc:title>{title}</dc:title>
	<dc:creator>{creator}</dc:creator>
</cp:coreProperties>
"#
	)
}

/// The line at the top of every page: the writer's surname, the title and the
/// page number, so a page that comes loose can be put back.
fn header(book: &Book) -> String {
	let mut label = String::new();
	if let Some(surname) = surname(&book.author) {
		label.push_str(surname);
		label.push_str(" / ");
	}
	label.push_str(title_of(book));
	label.push_str(" / ");
	let label = run(&label, Marks::default());
	format!(
		r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="{W}">
	<w:p>
		<w:pPr><w:pStyle w:val="Header"/></w:pPr>
		{label}<w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple>
	</w:p>
</w:hdr>
"#
	)
}

/// The last word of the writer's name, which is as much of it as a
/// manuscript's header has room for.
fn surname(author: &str) -> Option<&str> {
	author.split_whitespace().next_back()
}

#[cfg(test)]
mod tests {
	use std::io::{Cursor, Read};

	use uuid::Uuid;
	use zip::ZipArchive;

	use super::*;
	use crate::book::Part;

	/// A run of `text` with nothing set on it.
	fn plain(text: &str) -> String {
		format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
	}

	/// A run of `text` set with `marks`.
	fn set(marks: &str, text: &str) -> String {
		format!(r#"<w:r><w:rPr>{marks}</w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
	}

	/// A manuscript holding nothing but `markdown`.
	fn written(markdown: &str) -> Manuscript {
		let mut manuscript = Manuscript::default();
		manuscript.write(markdown);
		manuscript
	}

	fn read(docx: &[u8], name: &str) -> String {
		let mut archive = ZipArchive::new(Cursor::new(docx)).unwrap();
		let mut text = String::new();
		archive
			.by_name(name)
			.unwrap()
			.read_to_string(&mut text)
			.unwrap();
		text
	}

	#[test]
	fn a_manuscript_holds_every_part_word_needs() {
		let docx = docx(&Book::new("Ithaca"), &Compiled::default(), &Prose::new()).unwrap();

		let mut archive = ZipArchive::new(Cursor::new(docx)).unwrap();
		for name in [
			"[Content_Types].xml",
			"_rels/.rels",
			"docProps/core.xml",
			"word/_rels/document.xml.rels",
			"word/styles.xml",
			"word/header1.xml",
			"word/numbering.xml",
			"word/footnotes.xml",
			"word/document.xml",
		] {
			assert!(archive.by_name(name).is_ok(), "{name} is missing");
		}
	}

	#[test]
	fn each_part_and_chapter_opens_under_its_title_on_a_fresh_page() {
		let chapter = |title: &str| Chapter {
			id: Uuid::new_v4(),
			title: title.to_owned(),
			scenes: Vec::new(),
		};
		let compiled = Compiled {
			body: vec![
				Division::Part(Part {
					id: Uuid::new_v4(),
					title: "Book One".to_owned(),
					chapters: vec![chapter("Arrival")],
				}),
				Division::Chapter(chapter("Home")),
			],
			..Compiled::default()
		};
		let docx = docx(&Book::new("Ithaca"), &compiled, &Prose::new()).unwrap();

		let document = read(&docx, "word/document.xml");
		assert!(document.contains(&paragraph("BookTitle", &plain("Ithaca"))));
		for title in ["Book One", "Arrival", "Home"] {
			let heading = paragraph("ChapterTitle", &plain(title));
			assert!(
				document.contains(&heading),
				"{title} has no page of its own"
			);
		}
	}

	#[test]
	fn scenes_are_parted_by_a_centred_hash() {
		assert_eq!(
			written("She ran.\n\n---\n\nHe sat.").body,
			[
				paragraph("Normal", &plain("She ran.")),
				paragraph("SceneBreak", &plain("#")),
				paragraph("Normal", &plain("He sat.")),
			]
		);
	}

	#[test]
	fn italic_bold_and_strikethrough_are_kept() {
		let text = written("*She* **ran** ~~away~~, ***fast***").body.concat();

		assert!(text.contains(&set("<w:i/>", "She")));
		assert!(text.contains(&set("<w:b/>", "ran")));
		assert!(text.contains(&set("<w:strike/>", "away")));
		assert!(text.contains(&set("<w:b/><w:i/>", "fast")));
	}

	#[test]
	fn a_quote_a_heading_and_code_have_styles_of_their_own() {
		assert_eq!(
			written("> Said.\n\n## Later\n\n```\nlet a;\n\nlet b;\n```").body,
			[
				paragraph("Quote", &plain("Said.")),
				paragraph("Subheading", &plain("Later")),
				paragraph("CodeBlock", &plain("let a;")),
				paragraph("CodeBlock", &plain("")),
				paragraph("CodeBlock", &plain("let b;")),
			]
		);
	}

	#[test]
	fn each_list_is_numbered_on_its_own() {
		let manuscript = written("- milk\n- bread\n\n3. three\n4. four");

		let body = manuscript.body.concat();
		assert_eq!(body.matches(r#"<w:numId w:val="1"/>"#).count(), 2);
		assert_eq!(body.matches(r#"<w:numId w:val="2"/>"#).count(), 2);
		assert_eq!(manuscript.lists[0].start, None);
		assert_eq!(manuscript.lists[1].start, Some(3));
		assert!(numbering(&manuscript.lists).contains(r#"<w:startOverride w:val="3"/>"#));
	}

	#[test]
	fn a_list_inside_an_item_sits_a_level_down() {
		let body = written("- fruit\n  - apples").body.concat();

		assert!(body.contains(r#"<w:ilvl w:val="0"/><w:numId w:val="1"/>"#));
		assert!(body.contains(r#"<w:ilvl w:val="1"/><w:numId w:val="2"/>"#));
	}

	#[test]
	fn a_second_paragraph_in_an_item_keeps_its_indent_without_a_number() {
		let body = written("- one\n\n  more\n- two").body;

		assert!(body[0].contains("<w:numPr>"));
		assert!(body[1].contains(r#"<w:ind w:left="720"/>"#));
		assert!(!body[1].contains("<w:numPr>"));
		assert!(body[2].contains("<w:numPr>"));
	}

	#[test]
	fn a_table_is_ruled_in_columns_with_a_head_row() {
		let body = written("| Name | Age |\n| :--- | ---: |\n| Ann | 30 |").body;

		assert_eq!(body.len(), 1);
		let table = &body[0];
		assert!(table.starts_with("<w:tbl>"));
		assert_eq!(table.matches("<w:tr>").count(), 2);
		assert!(table.contains("<w:tblHeader/>"));
		assert_eq!(table.matches(r#"<w:gridCol w:w="4680"/>"#).count(), 2);
		assert!(table.contains(r#"<w:jc w:val="right"/>"#));
	}

	#[test]
	fn a_footnote_goes_to_the_footnotes_and_leaves_its_number_behind() {
		let manuscript = written("She ran.[^far]\n\n[^far]: Very far.");

		let body = manuscript.body.concat();
		assert!(body.contains(r#"<w:footnoteReference w:id="1"/>"#));
		assert!(!body.contains("Very far"));
		assert_eq!(manuscript.footnotes.len(), 1);
		let note = &manuscript.footnotes[0];
		assert!(note.starts_with(r#"<w:footnote w:id="1">"#));
		assert!(note.contains("<w:footnoteRef/>"));
		assert!(note.contains(&plain("Very far.")));
	}

	#[test]
	fn inline_code_and_links_are_set_as_such() {
		let manuscript = written("Run `ls` at [home](https://example.com).");

		let body = manuscript.body.concat();
		assert!(body.contains(r#"<w:rStyle w:val="InlineCode"/>"#));
		assert!(body.contains(r#"<w:hyperlink r:id="link1">"#));
		assert!(body.contains(r#"<w:rStyle w:val="Hyperlink"/>"#));
		assert_eq!(manuscript.links, ["https://example.com"]);
		let rels = document_rels(&manuscript.links);
		assert!(rels.contains(r#"Target="https://example.com" TargetMode="External""#));
	}

	#[test]
	fn the_header_carries_the_surname_the_title_and_the_page() {
		let mut book = Book::new("Ithaca");
		book.author = "Penelope Weaver".to_owned();

		let line = header(&book);
		assert!(line.contains(&plain("Weaver / Ithaca / ")));
		assert!(line.contains(r#"w:instr=" PAGE ""#));
		assert!(header(&Book::new("Ithaca")).contains(&plain("Ithaca / ")));
	}

	#[test]
	fn a_surname_is_the_last_word_of_a_name() {
		assert_eq!(surname("Penelope Weaver"), Some("Weaver"));
		assert_eq!(surname("Homer"), Some("Homer"));
		assert_eq!(surname("  "), None);
	}
}
