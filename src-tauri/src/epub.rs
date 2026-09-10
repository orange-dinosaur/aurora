//! The book as an EPUB 3: a zip of XHTML, with a package document that says
//! what the book is and a contents page that readers build their table of
//! contents from.
//!
//! Every chapter is a file of its own, and so is every part and every piece of
//! front and back matter. A reader starts each file on a fresh page, which is
//! where each of those belongs.

use std::io;
use std::slice;

use pulldown_cmark::{Event, Options, Parser, html};
use time::{OffsetDateTime, UtcOffset};

use crate::archive::Archive;
use crate::book::{Chapter, Compiled, Division, Prose, Scene, broken, told};
use crate::project::Book;

/// Where the package document sits in the zip. Every file it names is named
/// from the folder it is in.
const PACKAGE: &str = "EPUB/package.opf";

const NAV: &str = "EPUB/nav.xhtml";

/// The format fixes where this file is, and it says where everything else is.
const CONTAINER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
	<rootfiles>
		<rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml" />
	</rootfiles>
</container>
"#;

/// How the manifest says a file is XHTML.
const XHTML: &str = r#"media-type="application/xhtml+xml""#;

/// What the Markdown parser is asked to read beyond the core. The editor
/// writes strikethrough, and keeps a pasted table or footnote as it came.
pub fn options() -> Options {
	Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_FOOTNOTES
}

/// One line of the contents: a page, and the pages listed under it.
struct Entry {
	href: String,
	title: String,
	under: Vec<Entry>,
}

/// The book as an EPUB, marked as last changed at `now`.
pub fn epub(
	book: &Book,
	compiled: &Compiled,
	prose: &Prose,
	now: OffsetDateTime,
) -> io::Result<Vec<u8>> {
	let language = language_of(book);
	let piece = |pages: &mut Vec<String>, scene: &Scene| {
		page(
			pages,
			language,
			&scene.title,
			&told(slice::from_ref(scene), prose),
		)
	};
	let chaptered = |pages: &mut Vec<String>, chapter: &Chapter| {
		page(
			pages,
			language,
			&chapter.title,
			&broken(&chapter.scenes, prose),
		)
	};

	let mut pages = Vec::new();
	let mut contents = Vec::new();
	for scene in &compiled.front {
		contents.push(piece(&mut pages, scene));
	}
	for division in &compiled.body {
		contents.push(match division {
			Division::Part(part) => {
				let mut entry = page(&mut pages, language, &part.title, &[]);
				for chapter in &part.chapters {
					entry.under.push(chaptered(&mut pages, chapter));
				}
				entry
			}
			Division::Chapter(chapter) => chaptered(&mut pages, chapter),
		});
	}
	for scene in &compiled.back {
		contents.push(piece(&mut pages, scene));
	}

	let mut archive = Archive::default();
	archive.store("mimetype", b"application/epub+zip")?;
	archive.deflate("META-INF/container.xml", CONTAINER.as_bytes())?;
	archive.deflate(PACKAGE, package(book, pages.len(), now).as_bytes())?;
	archive.deflate(NAV, nav(language, &contents).as_bytes())?;
	for (at, text) in pages.iter().enumerate() {
		archive.deflate(&format!("EPUB/{}.xhtml", id(at)), text.as_bytes())?;
	}
	archive.finish()
}

/// The title the book goes by. A reader has to be given one.
fn title_of(book: &Book) -> &str {
	match book.title.trim() {
		"" => "Untitled",
		title => title,
	}
}

/// The language the book is in. One not yet chosen is left undetermined,
/// rather than claiming a language a reader would then hyphenate the text by.
fn language_of(book: &Book) -> &str {
	match book.language.trim() {
		"" => "und",
		language => language,
	}
}

/// Adds a page holding a heading and the prose under it, and answers with its
/// line in the contents.
fn page(pages: &mut Vec<String>, language: &str, title: &str, blocks: &[String]) -> Entry {
	let heading = escaped(title);
	let prose = rendered(&blocks.join("\n\n"));
	pages.push(xhtml(
		language,
		title,
		&format!("<h1>{heading}</h1>\n{prose}"),
	));

	Entry {
		href: format!("{}.xhtml", id(pages.len() - 1)),
		title: title.to_owned(),
		under: Vec::new(),
	}
}

/// What the page at `at` is called in the manifest, where a name has to start
/// with a letter.
fn id(at: usize) -> String {
	format!("text-{}", at + 1)
}

/// Markdown as XHTML. Raw HTML is left out: the editor keeps what it cannot
/// show as it came, a comment the writer left for themselves among it, and
/// none of it was meant to be read in the book or can be trusted to be
/// well-formed XML.
fn rendered(markdown: &str) -> String {
	let events = Parser::new_ext(markdown, options())
		.filter(|event| !matches!(event, Event::Html(_) | Event::InlineHtml(_)));
	let mut out = String::new();
	html::push_html(&mut out, events);
	out
}

/// A whole XHTML document around `body`.
fn xhtml(language: &str, title: &str, body: &str) -> String {
	let language = escaped(language);
	let title = escaped(title);
	format!(
		r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" lang="{language}" xml:lang="{language}">
<head>
	<meta charset="UTF-8" />
	<title>{title}</title>
</head>
<body>
{body}</body>
</html>
"#
	)
}

/// The contents page, with each part's chapters listed under it.
fn nav(language: &str, contents: &[Entry]) -> String {
	let mut list = String::new();
	listed(&mut list, contents);
	let body = format!("<nav epub:type=\"toc\" id=\"toc\">\n<h1>Contents</h1>\n{list}</nav>\n");
	xhtml(language, "Contents", &body)
}

fn listed(out: &mut String, entries: &[Entry]) {
	out.push_str("<ol>\n");
	for entry in entries {
		let title = escaped(&entry.title);
		out.push_str(&format!("<li><a href=\"{}\">{title}</a>", entry.href));
		if !entry.under.is_empty() {
			out.push('\n');
			listed(out, &entry.under);
		}
		out.push_str("</li>\n");
	}
	out.push_str("</ol>\n");
}

/// The package document: what the book is, every file in it, and the order
/// they are read in.
fn package(book: &Book, pages: usize, now: OffsetDateTime) -> String {
	let identifier = book.identifier;
	let title = escaped(title_of(book));
	let language = escaped(language_of(book));
	let changed = modified(now);

	let mut metadata = vec![
		format!(r#"<dc:identifier id="book-id">urn:uuid:{identifier}</dc:identifier>"#),
		format!("<dc:title>{title}</dc:title>"),
		format!("<dc:language>{language}</dc:language>"),
	];
	let author = book.author.trim();
	if !author.is_empty() {
		metadata.push(format!("<dc:creator>{}</dc:creator>", escaped(author)));
	}
	metadata.push(format!(
		r#"<meta property="dcterms:modified">{changed}</meta>"#
	));

	let nav = format!(r#"<item id="nav" href="nav.xhtml" {XHTML} properties="nav" />"#);
	let mut manifest = vec![nav];
	let mut spine = Vec::new();
	for at in 0..pages {
		let name = id(at);
		manifest.push(format!(
			r#"<item id="{name}" href="{name}.xhtml" {XHTML} />"#
		));
		spine.push(format!(r#"<itemref idref="{name}" />"#));
	}

	let metadata = metadata.join("\n\t\t");
	let manifest = manifest.join("\n\t\t");
	let spine = spine.join("\n\t\t");
	format!(
		r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="book-id" xml:lang="{language}">
	<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
		{metadata}
	</metadata>
	<manifest>
		{manifest}
	</manifest>
	<spine>
		{spine}
	</spine>
</package>
"#
	)
}

/// When the book last changed, written the one way EPUB accepts: in UTC, to
/// the second.
fn modified(now: OffsetDateTime) -> String {
	let now = now.to_offset(UtcOffset::UTC);
	let (year, month, day) = (now.year(), u8::from(now.month()), now.day());
	let (hour, minute, second) = (now.hour(), now.minute(), now.second());
	format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Text made safe to sit in XML, between tags or inside an attribute's quotes.
fn escaped(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	for c in text.chars() {
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

#[cfg(test)]
mod tests {
	use std::io::{Cursor, Read};

	use uuid::Uuid;
	use zip::ZipArchive;

	use super::*;
	use crate::book::{Part, all_scenes};

	/// 2026-09-10 at 12:34:56 in UTC.
	fn now() -> OffsetDateTime {
		OffsetDateTime::from_unix_timestamp(1_789_043_696).unwrap()
	}

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

	fn alone(chapter: Chapter) -> Compiled {
		Compiled {
			body: vec![Division::Chapter(chapter)],
			..Compiled::default()
		}
	}

	/// The EPUB of `compiled`, each scene holding what `said` gives for its
	/// title.
	fn exported(book: &Book, compiled: &Compiled, said: &[(&str, &str)]) -> Vec<u8> {
		let prose = all_scenes(compiled)
			.into_iter()
			.filter_map(|scene| {
				said.iter()
					.find(|(title, _)| *title == scene.title)
					.map(|(_, text)| (scene.id, (*text).to_owned()))
			})
			.collect();
		epub(book, compiled, &prose, now()).unwrap()
	}

	fn read(epub: &[u8], name: &str) -> String {
		let mut archive = ZipArchive::new(Cursor::new(epub)).unwrap();
		let mut text = String::new();
		archive
			.by_name(name)
			.unwrap()
			.read_to_string(&mut text)
			.unwrap();
		text
	}

	#[test]
	fn the_mimetype_comes_first_and_the_container_points_at_the_package() {
		let epub = exported(&Book::new("Ithaca"), &Compiled::default(), &[]);

		let mut archive = ZipArchive::new(Cursor::new(epub.as_slice())).unwrap();
		assert_eq!(archive.by_index(0).unwrap().name(), "mimetype");
		assert!(read(&epub, "META-INF/container.xml").contains(PACKAGE));
	}

	#[test]
	fn the_package_says_what_the_book_is() {
		let mut book = Book::new("War & Peace");
		book.author = "Leo Tolstoy".to_owned();
		book.language = "en-GB".to_owned();

		let package = read(&exported(&book, &Compiled::default(), &[]), PACKAGE);
		assert!(package.contains(&format!("urn:uuid:{}", book.identifier)));
		assert!(package.contains("<dc:title>War &amp; Peace</dc:title>"));
		assert!(package.contains("<dc:creator>Leo Tolstoy</dc:creator>"));
		assert!(package.contains("<dc:language>en-GB</dc:language>"));
		assert!(package.contains(">2026-09-10T12:34:56Z</meta>"));
	}

	#[test]
	fn a_book_with_no_language_or_author_claims_neither() {
		let package = read(
			&exported(&Book::new("Ithaca"), &Compiled::default(), &[]),
			PACKAGE,
		);

		assert!(package.contains("<dc:language>und</dc:language>"));
		assert!(!package.contains("dc:creator"));
	}

	#[test]
	fn every_chapter_is_a_page_and_the_contents_nest_them_under_their_part() {
		let compiled = Compiled {
			front: vec![scene("Dedication")],
			body: vec![
				Division::Part(Part {
					id: Uuid::new_v4(),
					title: "Book One".to_owned(),
					chapters: vec![
						chapter("Arrival", vec![scene("a")]),
						chapter("Harbour", vec![scene("b")]),
					],
				}),
				Division::Chapter(chapter("Home", vec![scene("c")])),
			],
			back: Vec::new(),
		};
		let epub = exported(
			&Book::new("Ithaca"),
			&compiled,
			&[("Dedication", "For P."), ("a", "She landed.")],
		);

		let package = read(&epub, PACKAGE);
		for page in 1..=5 {
			assert!(package.contains(&format!(r#"<itemref idref="text-{page}" />"#)));
		}
		assert!(!package.contains("text-6"));

		let nav = read(&epub, NAV);
		assert!(nav.contains(concat!(
			"<li><a href=\"text-2.xhtml\">Book One</a>\n",
			"<ol>\n",
			"<li><a href=\"text-3.xhtml\">Arrival</a></li>\n",
			"<li><a href=\"text-4.xhtml\">Harbour</a></li>\n",
			"</ol>\n",
			"</li>\n",
			"<li><a href=\"text-5.xhtml\">Home</a></li>\n",
		)));

		let arrival = read(&epub, "EPUB/text-3.xhtml");
		assert!(arrival.contains("<h1>Arrival</h1>\n<p>She landed.</p>"));
	}

	#[test]
	fn scenes_in_a_chapter_are_parted_by_a_rule() {
		let compiled = alone(chapter("Home", vec![scene("a"), scene("b")]));
		let epub = exported(
			&Book::new("Ithaca"),
			&compiled,
			&[("a", "She ran."), ("b", "He stayed.")],
		);

		let page = read(&epub, "EPUB/text-1.xhtml");
		assert!(page.contains("<p>She ran.</p>\n<hr />\n<p>He stayed.</p>"));
	}

	#[test]
	fn text_is_escaped_and_raw_html_is_left_out() {
		let compiled = alone(chapter("Fish & Chips", vec![scene("a")]));
		let epub = exported(
			&Book::new("Ithaca"),
			&compiled,
			&[("a", "Salt & vinegar.\n\n<!-- cut this -->")],
		);

		let page = read(&epub, "EPUB/text-1.xhtml");
		assert!(page.contains("<title>Fish &amp; Chips</title>"));
		assert!(page.contains("<h1>Fish &amp; Chips</h1>"));
		assert!(page.contains("<p>Salt &amp; vinegar.</p>"));
		assert!(!page.contains("cut this"));
	}

	#[test]
	fn the_time_changed_is_utc_to_the_second() {
		let later = now()
			.replace_nanosecond(250_000_000)
			.unwrap()
			.to_offset(UtcOffset::from_hms(2, 0, 0).unwrap());

		assert_eq!(modified(later), "2026-09-10T12:34:56Z");
	}
}
