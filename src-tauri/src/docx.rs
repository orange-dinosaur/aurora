//! The book as a Word document in standard manuscript format, which is what an
//! agent or an editor expects to be sent: 12pt Times New Roman, double-spaced,
//! one-inch margins, a half-inch indent at the start of every paragraph, the
//! writer's surname, the title and the page number at the top of every page,
//! and each chapter on a fresh page a third of the way down.

use std::io;
use std::slice;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::archive::Archive;
use crate::book::{Chapter, Compiled, Division, Prose, Scene, broken, told};
use crate::epub::{escaped, options, title_of};
use crate::project::Book;

/// WordprocessingML's namespace, and the one relationships are named in.
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// What each part of the package is. Word will not open a file with a part it
/// cannot put a type to.
const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
	<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
	<Default Extension="xml" ContentType="application/xml"/>
	<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
	<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
	<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>
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

/// What the document draws on: its styles, and the header its page names as
/// `rId2`.
const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
	<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
	<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
</Relationships>
"#;

/// The manuscript's look. Lengths are in twentieths of a point, so 1440 is an
/// inch. A chapter title sits 3840 below the top margin, which on a Letter
/// page is a third of the way down, and the book's title 6480, halfway. The
/// schema fixes the order of everything inside a `w:pPr`.
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
	<w:style w:type="paragraph" w:styleId="SceneBreak">
		<w:name w:val="Scene Break"/>
		<w:basedOn w:val="Normal"/>
		<w:next w:val="Normal"/>
		<w:pPr>
			<w:ind w:firstLine="0"/>
			<w:jc w:val="center"/>
		</w:pPr>
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
</w:styles>
"#;

/// How a run of text is set. Each is a depth rather than a flag, since
/// emphasis can sit inside emphasis.
#[derive(Debug, Clone, Copy, Default)]
struct Marks {
	italic: u32,
	bold: u32,
	struck: u32,
}

/// The book as a manuscript.
pub fn docx(book: &Book, compiled: &Compiled, prose: &Prose) -> io::Result<Vec<u8>> {
	let mut body = title_page(book);
	for scene in &compiled.front {
		write_piece(&mut body, scene, prose);
	}
	for division in &compiled.body {
		match division {
			Division::Part(part) => {
				body.push(heading(&part.title));
				for chapter in &part.chapters {
					write_chapter(&mut body, chapter, prose);
				}
			}
			Division::Chapter(chapter) => write_chapter(&mut body, chapter, prose),
		}
	}
	for scene in &compiled.back {
		write_piece(&mut body, scene, prose);
	}

	let mut archive = Archive::default();
	archive.deflate("[Content_Types].xml", CONTENT_TYPES.as_bytes())?;
	archive.deflate("_rels/.rels", PACKAGE_RELS.as_bytes())?;
	archive.deflate("docProps/core.xml", core(book).as_bytes())?;
	archive.deflate("word/_rels/document.xml.rels", DOCUMENT_RELS.as_bytes())?;
	archive.deflate("word/styles.xml", STYLES.as_bytes())?;
	archive.deflate("word/header1.xml", header(book).as_bytes())?;
	archive.deflate("word/document.xml", document(&body).as_bytes())?;
	archive.finish()
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

/// A chapter: its title on a fresh page, then its scenes parted by breaks.
fn write_chapter(body: &mut Vec<String>, chapter: &Chapter, prose: &Prose) {
	body.push(heading(&chapter.title));
	let text = broken(&chapter.scenes, prose).join("\n\n");
	body.extend(converted(&text));
}

/// A piece of front or back matter, which is laid out as a chapter is.
fn write_piece(body: &mut Vec<String>, scene: &Scene, prose: &Prose) {
	body.push(heading(&scene.title));
	let text = told(slice::from_ref(scene), prose).join("\n\n");
	body.extend(converted(&text));
}

/// A title on a fresh page, a third of the way down.
fn heading(title: &str) -> String {
	paragraph("ChapterTitle", &run(title, Marks::default()))
}

/// Markdown as the paragraphs of a manuscript. A scene break is a centred `#`,
/// and italic, bold and strikethrough are kept; everything else comes through
/// as paragraphs of its text.
fn converted(markdown: &str) -> Vec<String> {
	let mut out = Vec::new();
	let mut runs = String::new();
	let mut marks = Marks::default();
	for event in Parser::new_ext(markdown, options()) {
		match event {
			Event::Start(Tag::Emphasis) => marks.italic += 1,
			Event::End(TagEnd::Emphasis) => marks.italic -= 1,
			Event::Start(Tag::Strong) => marks.bold += 1,
			Event::End(TagEnd::Strong) => marks.bold -= 1,
			Event::Start(Tag::Strikethrough) => marks.struck += 1,
			Event::End(TagEnd::Strikethrough) => marks.struck -= 1,
			// A link or an image keeps its words and loses where it points.
			Event::Start(Tag::Link { .. } | Tag::Image { .. }) => {}
			Event::End(TagEnd::Link | TagEnd::Image) => {}
			// Every other start or end is a block's, and ends the paragraph.
			Event::Start(_) | Event::End(_) => close(&mut out, &mut runs),
			Event::Text(text) | Event::Code(text) => runs.push_str(&run(&text, marks)),
			Event::SoftBreak => runs.push_str(&run(" ", marks)),
			Event::HardBreak => runs.push_str("<w:r><w:br/></w:r>"),
			Event::FootnoteReference(label) => {
				let marker = format!("[{label}]");
				runs.push_str(&run(&marker, marks));
			}
			Event::Rule => {
				close(&mut out, &mut runs);
				out.push(paragraph("SceneBreak", &run("#", Marks::default())));
			}
			// Raw HTML is left out, as it is from the EPUB.
			_ => {}
		}
	}
	close(&mut out, &mut runs);
	out
}

/// Ends the paragraph being gathered, if it has any words in it.
fn close(out: &mut Vec<String>, runs: &mut String) {
	if !runs.is_empty() {
		out.push(paragraph("Normal", runs));
		runs.clear();
	}
}

/// A paragraph in `style` holding `runs`.
fn paragraph(style: &str, runs: &str) -> String {
	format!(r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr>{runs}</w:p>"#)
}

/// A run of `text`, set as `marks` say.
fn run(text: &str, marks: Marks) -> String {
	// The schema fixes this order too.
	let mut set = String::new();
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
			converted("She ran.\n\n---\n\nHe sat."),
			[
				paragraph("Normal", &plain("She ran.")),
				paragraph("SceneBreak", &plain("#")),
				paragraph("Normal", &plain("He sat.")),
			]
		);
	}

	#[test]
	fn italic_bold_and_strikethrough_are_kept() {
		let text = converted("*She* **ran** ~~away~~, ***fast***").concat();

		assert!(text.contains(&set("<w:i/>", "She")));
		assert!(text.contains(&set("<w:b/>", "ran")));
		assert!(text.contains(&set("<w:strike/>", "away")));
		assert!(text.contains(&set("<w:b/><w:i/>", "fast")));
	}

	#[test]
	fn anything_else_comes_through_as_paragraphs_of_its_text() {
		assert_eq!(
			converted("- milk\n- bread\n\n> Said."),
			[
				paragraph("Normal", &plain("milk")),
				paragraph("Normal", &plain("bread")),
				paragraph("Normal", &plain("Said.")),
			]
		);
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
