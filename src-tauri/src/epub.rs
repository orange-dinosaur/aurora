//! The book as an EPUB 3: a zip of XHTML, with a package document that says
//! what the book is and a contents page that readers build their table of
//! contents from.
//!
//! Every chapter is a file of its own, and so is every part, every piece of
//! front and back matter, and the title and copyright pages made from the
//! book's details. A reader starts each file on a fresh page, which is where
//! each of those belongs.

use std::io;
use std::slice;

use pulldown_cmark::{Event, Options, Parser, html};
use time::{OffsetDateTime, UtcOffset};

use crate::archive::Archive;
use crate::book::{Chapter, Compiled, Division, Prose, Scene, broken, told};
use crate::project::{Book, Role};

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

const STYLE: &str = "EPUB/style.css";

/// The stylesheet's line in the manifest.
const STYLE_ITEM: &str = r#"<item id="style" href="style.css" media-type="text/css" />"#;

/// Only what the generated pages need. Everything else is left to the reader,
/// where the person reading has chosen their own type and spacing.
const CSS: &str = r#".titlepage {
	margin-top: 25%;
	text-align: center;
}

.titlepage h1 {
	margin-bottom: 0.25em;
}

.subtitle {
	margin-top: 0;
	font-style: italic;
}

.author {
	margin-top: 3em;
	font-size: 1.2em;
}

.copyright-page {
	margin-top: 25%;
	font-size: 0.85em;
}
"#;

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
	let title = title_of(book);
	contents.push(add_page(&mut pages, language, title, &title_page(book)));
	if let Some(notice) = copyright_page(book) {
		contents.push(add_page(&mut pages, language, "Copyright", &notice));
	}
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
	archive.deflate(STYLE, CSS.as_bytes())?;
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
	let body = format!("<h1>{heading}</h1>\n{prose}");
	add_page(pages, language, title, &body)
}

/// Adds a page around `body`, and answers with its line in the contents.
fn add_page(pages: &mut Vec<String>, language: &str, title: &str, body: &str) -> Entry {
	pages.push(xhtml(language, title, body));

	Entry {
		href: format!("{}.xhtml", id(pages.len() - 1)),
		title: title.to_owned(),
		under: Vec::new(),
	}
}

/// The title page: the title, any subtitle, and who wrote it.
fn title_page(book: &Book) -> String {
	let mut lines = vec![format!("<h1>{}</h1>", escaped(title_of(book)))];
	lines.extend(filled("subtitle", &book.subtitle));
	lines.extend(filled("author", &book.author));
	section("titlepage", &lines)
}

/// The copyright page: the writer's notice laid out as they typed it, then the
/// publisher and the ISBN. A book with none of the three has no such page.
fn copyright_page(book: &Book) -> Option<String> {
	let mut lines = paragraphs(&book.copyright);
	lines.extend(filled("publisher", &book.publisher));
	if let Some(isbn) = given(&book.isbn) {
		lines.push(format!("<p>ISBN {isbn}</p>"));
	}

	if lines.is_empty() {
		None
	} else {
		Some(section("copyright-page", &lines))
	}
}

/// A paragraph for each run of lines in `text`, keeping the line breaks
/// inside each one.
fn paragraphs(text: &str) -> Vec<String> {
	text.split("\n\n")
		.map(str::trim)
		.filter(|paragraph| !paragraph.is_empty())
		.map(|paragraph| {
			let lines: Vec<_> = paragraph.lines().map(str::trim).map(escaped).collect();
			format!("<p>{}</p>", lines.join("<br />\n"))
		})
		.collect()
}

/// A paragraph of `text` marked with `class`, or nothing when it is blank.
fn filled(class: &str, text: &str) -> Option<String> {
	let text = given(text)?;
	Some(format!(r#"<p class="{class}">{text}</p>"#))
}

/// A generated page, marked with the kind of page a reader knows it as.
fn section(kind: &str, lines: &[String]) -> String {
	let lines = lines.join("\n");
	format!("<section epub:type=\"{kind}\" class=\"{kind}\">\n{lines}\n</section>\n")
}

/// `text` trimmed and escaped, or nothing when it is blank.
fn given(text: &str) -> Option<String> {
	let text = text.trim();
	(!text.is_empty()).then(|| escaped(text))
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
	<link rel="stylesheet" type="text/css" href="style.css" />
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
	let language = escaped(language_of(book));

	let nav = format!(r#"<item id="nav" href="nav.xhtml" {XHTML} properties="nav" />"#);
	let mut manifest = vec![nav, STYLE_ITEM.to_owned()];
	let mut spine = Vec::new();
	for at in 0..pages {
		let name = id(at);
		manifest.push(format!(
			r#"<item id="{name}" href="{name}.xhtml" {XHTML} />"#
		));
		spine.push(format!(r#"<itemref idref="{name}" />"#));
	}

	let metadata = described(book, now).join("\n\t\t");
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

/// What the package says about the book: every detail the writer has filled
/// in, each under the element or property a reader looks for it by.
fn described(book: &Book, now: OffsetDateTime) -> Vec<String> {
	let identifier = format!("urn:uuid:{}", book.identifier);
	let title = escaped(title_of(book));
	let language = escaped(language_of(book));
	let changed = modified(now);

	let mut metadata = vec![
		element("identifier", "book-id", &identifier),
		element("title", "title", &title),
		refine("title", "title-type", "main"),
		format!("<dc:language>{language}</dc:language>"),
		format!(r#"<meta property="dcterms:modified">{changed}</meta>"#),
	];
	if let Some(subtitle) = given(&book.subtitle) {
		metadata.push(element("title", "subtitle", &subtitle));
		metadata.push(refine("subtitle", "title-type", "subtitle"));
	}
	if let Some(author) = given(&book.author) {
		metadata.push(element("creator", "author", &author));
		metadata.push(role("author", "aut"));
	}
	for (at, contributor) in book.contributors.iter().enumerate() {
		let Some(name) = given(&contributor.name) else {
			continue;
		};
		let id = format!("contributor-{}", at + 1);
		metadata.push(element("contributor", &id, &name));
		metadata.push(role(&id, relator(contributor.role)));
	}
	if let Some(series) = given(&book.series) {
		metadata.push(format!(
			r#"<meta property="belongs-to-collection" id="series">{series}</meta>"#
		));
		metadata.push(refine("series", "collection-type", "series"));
		// Calibre and Kobo read a series from these instead.
		metadata.push(format!(
			r#"<meta name="calibre:series" content="{series}" />"#
		));
		if let Some(number) = position(&book.series_number) {
			metadata.push(refine("series", "group-position", number));
			metadata.push(format!(
				r#"<meta name="calibre:series_index" content="{number}" />"#
			));
		}
	}
	if let Some(isbn) = isbn(&book.isbn) {
		metadata.push(element("identifier", "isbn", &format!("urn:isbn:{isbn}")));
	}
	if let Some(blurb) = given(&book.blurb) {
		metadata.push(format!("<dc:description>{blurb}</dc:description>"));
	}
	if let Some(publisher) = given(&book.publisher) {
		metadata.push(format!("<dc:publisher>{publisher}</dc:publisher>"));
	}
	if let Some(date) = w3c_date(&book.publication_date) {
		metadata.push(format!("<dc:date>{date}</dc:date>"));
	}
	for keyword in &book.keywords {
		if let Some(keyword) = given(keyword) {
			metadata.push(format!("<dc:subject>{keyword}</dc:subject>"));
		}
	}
	if let Some(rights) = given(&book.copyright) {
		metadata.push(format!("<dc:rights>{rights}</dc:rights>"));
	}
	metadata
}

/// A Dublin Core element with an id that properties can refine.
fn element(name: &str, id: &str, text: &str) -> String {
	format!(r#"<dc:{name} id="{id}">{text}</dc:{name}>"#)
}

/// A property of the element with `id`.
fn refine(id: &str, property: &str, value: &str) -> String {
	format!(r##"<meta refines="#{id}" property="{property}">{value}</meta>"##)
}

/// What the person named by the element with `id` did on the book, as a MARC
/// relator code.
fn role(id: &str, code: &str) -> String {
	format!(r##"<meta refines="#{id}" property="role" scheme="marc:relators">{code}</meta>"##)
}

/// The code MARC gives each role. It has none for a copy editor, who is named
/// only as a contributor.
fn relator(role: Role) -> &'static str {
	match role {
		Role::Editor => "edt",
		Role::CopyEditor => "ctb",
		Role::Proofreader => "pfr",
		Role::CoverDesigner => "cov",
		Role::Illustrator => "ill",
		Role::Translator => "trl",
		Role::Narrator => "nrt",
	}
}

/// Where the book falls in its series, when that is a number a reader can
/// sort by.
fn position(number: &str) -> Option<&str> {
	let number = number.trim();
	number
		.parse::<f64>()
		.is_ok_and(f64::is_finite)
		.then_some(number)
}

/// The ISBN as its URN wants it, digits alone, when there are as many as an
/// ISBN has.
fn isbn(typed: &str) -> Option<String> {
	let digits: String = typed
		.chars()
		.filter(|c| c.is_ascii_digit() || matches!(*c, 'x' | 'X'))
		.collect();

	if matches!(digits.len(), 10 | 13) {
		Some(digits.to_uppercase())
	} else {
		None
	}
}

/// The publication date when it is in a shape EPUB reads: a year, a year and a
/// month, or a whole date, written with hyphens. Anything else the writer typed
/// stays on the Book page and out of the file.
fn w3c_date(typed: &str) -> Option<&str> {
	let typed = typed.trim();
	let parts: Vec<&str> = typed.split('-').collect();
	if parts.len() > 3 {
		return None;
	}
	for (part, width) in parts.iter().zip([4, 2, 2]) {
		if part.len() != width || !part.bytes().all(|byte| byte.is_ascii_digit()) {
			return None;
		}
	}
	Some(typed)
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
	use crate::project::Contributor;

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
		assert!(package.contains(">War &amp; Peace</dc:title>"));
		assert!(package.contains(">Leo Tolstoy</dc:creator>"));
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

		// The title page is the first, so the story starts at the second.
		let package = read(&epub, PACKAGE);
		for page in 1..=6 {
			assert!(package.contains(&format!(r#"<itemref idref="text-{page}" />"#)));
		}
		assert!(!package.contains("text-7"));

		let nav = read(&epub, NAV);
		assert!(nav.contains(concat!(
			"<li><a href=\"text-3.xhtml\">Book One</a>\n",
			"<ol>\n",
			"<li><a href=\"text-4.xhtml\">Arrival</a></li>\n",
			"<li><a href=\"text-5.xhtml\">Harbour</a></li>\n",
			"</ol>\n",
			"</li>\n",
			"<li><a href=\"text-6.xhtml\">Home</a></li>\n",
		)));

		let arrival = read(&epub, "EPUB/text-4.xhtml");
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

		let page = read(&epub, "EPUB/text-2.xhtml");
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

		let page = read(&epub, "EPUB/text-2.xhtml");
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

	#[test]
	fn every_detail_the_book_has_goes_in_the_package() {
		let mut book = Book::new("Ithaca");
		book.subtitle = "A Homecoming".to_owned();
		book.author = "Penelope Weaver".to_owned();
		book.contributors = vec![
			Contributor {
				name: "Mentor".to_owned(),
				role: Role::Editor,
			},
			Contributor {
				name: "Phemius".to_owned(),
				role: Role::Narrator,
			},
		];
		book.series = "The Return".to_owned();
		book.series_number = "2".to_owned();
		book.blurb = "Twenty years & home.".to_owned();
		book.publisher = "Scheria Press".to_owned();
		book.publication_date = "2026-09".to_owned();
		book.isbn = "978-0-14-026886-7".to_owned();
		book.keywords = vec!["sea".to_owned(), "home".to_owned()];
		book.copyright = "© 2026 Penelope Weaver".to_owned();

		let package = read(&exported(&book, &Compiled::default(), &[]), PACKAGE);
		for line in [
			r#"<dc:title id="subtitle">A Homecoming</dc:title>"#,
			r##"<meta refines="#subtitle" property="title-type">subtitle</meta>"##,
			r#"<dc:creator id="author">Penelope Weaver</dc:creator>"#,
			r#"<dc:contributor id="contributor-1">Mentor</dc:contributor>"#,
			r##"<meta refines="#contributor-1" property="role" scheme="marc:relators">edt</meta>"##,
			r##"<meta refines="#contributor-2" property="role" scheme="marc:relators">nrt</meta>"##,
			r#"<meta property="belongs-to-collection" id="series">The Return</meta>"#,
			r##"<meta refines="#series" property="group-position">2</meta>"##,
			r#"<meta name="calibre:series_index" content="2" />"#,
			"<dc:description>Twenty years &amp; home.</dc:description>",
			"<dc:publisher>Scheria Press</dc:publisher>",
			"<dc:date>2026-09</dc:date>",
			"<dc:subject>sea</dc:subject>",
			"<dc:subject>home</dc:subject>",
			"<dc:rights>© 2026 Penelope Weaver</dc:rights>",
			r#"<dc:identifier id="isbn">urn:isbn:9780140268867</dc:identifier>"#,
		] {
			assert!(package.contains(line), "{line} is missing");
		}
	}

	#[test]
	fn the_title_page_comes_first_and_the_copyright_page_after_it() {
		let notice = "© 2026 Penelope Weaver\nAll rights reserved.\n\nFirst edition.";
		let mut book = Book::new("Ithaca");
		book.subtitle = "A Homecoming".to_owned();
		book.author = "Penelope Weaver".to_owned();
		book.copyright = notice.to_owned();
		book.isbn = "978-0-14-026886-7".to_owned();
		let epub = exported(&book, &Compiled::default(), &[]);

		let title = read(&epub, "EPUB/text-1.xhtml");
		assert!(title.contains(concat!(
			"<h1>Ithaca</h1>\n",
			"<p class=\"subtitle\">A Homecoming</p>\n",
			"<p class=\"author\">Penelope Weaver</p>",
		)));

		let copyright = read(&epub, "EPUB/text-2.xhtml");
		assert!(copyright.contains(concat!(
			"<p>© 2026 Penelope Weaver<br />\nAll rights reserved.</p>\n",
			"<p>First edition.</p>\n",
			"<p>ISBN 978-0-14-026886-7</p>",
		)));

		let nav = read(&epub, NAV);
		assert!(nav.contains(concat!(
			"<li><a href=\"text-1.xhtml\">Ithaca</a></li>\n",
			"<li><a href=\"text-2.xhtml\">Copyright</a></li>\n",
		)));
	}

	#[test]
	fn a_book_with_no_copyright_details_has_no_copyright_page() {
		let epub = exported(&Book::new("Ithaca"), &Compiled::default(), &[]);

		let nav = read(&epub, NAV);
		assert!(nav.contains(concat!(
			"<li><a href=\"text-1.xhtml\">Ithaca</a></li>\n",
			"</ol>",
		)));
		assert!(!nav.contains("Copyright"));
	}

	#[test]
	fn a_date_is_only_kept_in_a_shape_epub_reads() {
		assert_eq!(w3c_date("2026"), Some("2026"));
		assert_eq!(w3c_date(" 2026-09 "), Some("2026-09"));
		assert_eq!(w3c_date("2026-09-10"), Some("2026-09-10"));
		assert_eq!(w3c_date("September 2026"), None);
		assert_eq!(w3c_date("10/09/2026"), None);
		assert_eq!(w3c_date("2026-9-10"), None);
		assert_eq!(w3c_date(""), None);
	}

	#[test]
	fn an_isbn_is_kept_to_its_digits() {
		assert_eq!(isbn("978-0-14-026886-7").as_deref(), Some("9780140268867"));
		assert_eq!(isbn("0 14 026886 x").as_deref(), Some("014026886X"));
		assert_eq!(isbn("pending"), None);
	}

	#[test]
	fn only_a_number_places_a_book_in_its_series() {
		assert_eq!(position(" 1.5 "), Some("1.5"));
		assert_eq!(position("Two"), None);
		assert_eq!(position(""), None);
	}
}
