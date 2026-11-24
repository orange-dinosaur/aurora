//! A screenplay as a PDF on US Letter, drawn from `layout`, so the file has the
//! pages the counts say. Courier Prime goes in whole, so the page looks like
//! the editor.

use std::collections::BTreeSet;

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use ttf_parser::Face;

use super::fountain::{Block, Kind, Span, Style};
use super::layout::{LINES_PER_PAGE, Line, layout, plain};
use crate::book::title_of;
use crate::project::Book;

struct Font {
	/// What the page calls it.
	resource: &'static [u8],
	base: &'static [u8],
	data: &'static [u8],
}

/// Indexed by `font(style)`.
const FONTS: [Font; 4] = [
	Font {
		resource: b"F0",
		base: b"CourierPrime",
		data: include_bytes!("../../../src/assets/fonts/courier-prime-roman.ttf"),
	},
	Font {
		resource: b"F1",
		base: b"CourierPrime-Italic",
		data: include_bytes!("../../../src/assets/fonts/courier-prime-italic.ttf"),
	},
	Font {
		resource: b"F2",
		base: b"CourierPrime-Bold",
		data: include_bytes!("../../../src/assets/fonts/courier-prime-bold.ttf"),
	},
	Font {
		resource: b"F3",
		base: b"CourierPrime-BoldItalic",
		data: include_bytes!("../../../src/assets/fonts/courier-prime-bold-italic.ttf"),
	},
];

fn font(style: Style) -> usize {
	usize::from(style.italic) + 2 * usize::from(style.bold)
}

const INCH: f32 = 72.0;
const SIZE: f32 = 12.0;
/// Courier at 12pt sets ten characters to the inch.
const CHAR: f32 = 7.2;
/// The top of the first line, an inch below the top of the page.
const TOP: f32 = 10.0 * INCH;
const RIGHT: f32 = 7.5 * INCH;
/// Midway between the margins, which is where centred text sits.
const CENTRE: f32 = 4.5 * INCH;

/// Where the text of row `row` stands, counting from the first line.
fn baseline(row: f32) -> f32 {
	TOP - SIZE * row - 9.0
}

fn width(text: &str) -> f32 {
	text.chars().count() as f32 * CHAR
}

/// Where a line starts. The indents are afterwriting's, as the layout's
/// widths are.
fn left(line: &Line) -> f32 {
	match line.kind {
		Kind::Character => 3.5 * INCH,
		Kind::Dialogue => 2.5 * INCH,
		Kind::Parenthetical => 3.0 * INCH,
		Kind::Transition => RIGHT - width(&line.text()),
		Kind::Centered => CENTRE - width(&line.text()) / 2.0,
		Kind::Heading | Kind::Action | Kind::PageBreak => 1.5 * INCH,
	}
}

/// The script with a title page in front, as the bytes of a PDF.
pub fn pdf(book: &Book, scenes: &[Vec<Block>]) -> Vec<u8> {
	let faces: Vec<Face> = FONTS
		.iter()
		.map(|font| Face::parse(font.data, 0).expect("Courier Prime is built in"))
		.collect();
	let mut writer = Writer {
		faces: &faces,
		used: Default::default(),
	};

	let mut pages = vec![writer.title_page(book)];
	for (at, page) in layout(scenes).pages.iter().enumerate() {
		let mut content = Content::new();
		// The first page of the script goes unnumbered.
		if at > 0 {
			let number = format!("{}.", at + 1);
			writer.draw(
				&mut content,
				RIGHT - width(&number),
				baseline(-3.0),
				&[plain(&number)],
			);
		}
		for (row, line) in page.iter().enumerate() {
			writer.draw(&mut content, left(line), baseline(row as f32), &line.spans);
		}
		pages.push(content);
	}

	let mut alloc = Ref::new(1);
	let mut next = || alloc.bump();
	let (catalog, tree, info) = (next(), next(), next());
	let fonts: Vec<Option<Ref>> = writer
		.used
		.iter()
		.map(|chars| (!chars.is_empty()).then(&mut next))
		.collect();
	let ids: Vec<Ref> = pages.iter().map(|_| next()).collect();

	let mut out = Pdf::new();
	out.catalog(catalog).pages(tree);
	out.pages(tree)
		.kids(ids.iter().copied())
		.count(ids.len() as i32);
	out.document_info(info)
		.title(TextStr(title_of(book)))
		.author(TextStr(book.author.trim()))
		.creator(TextStr("Aurora"));

	for (content, &id) in pages.into_iter().zip(&ids) {
		let stream = next();
		let mut page = out.page(id);
		page.media_box(Rect::new(0.0, 0.0, 8.5 * INCH, 11.0 * INCH))
			.parent(tree)
			.contents(stream);
		let mut resources = page.resources();
		let mut names = resources.fonts();
		for (font, id) in FONTS.iter().zip(&fonts) {
			if let Some(id) = id {
				names.pair(Name(font.resource), *id);
			}
		}
		names.finish();
		resources.finish();
		page.finish();
		out.stream(stream, &content.finish());
	}

	for (at, id) in fonts.iter().enumerate() {
		if let Some(id) = id {
			embed(&mut out, at, &faces[at], &writer.used[at], *id, &mut next);
		}
	}
	out.finish()
}

struct Writer<'a> {
	faces: &'a [Face<'a>],
	/// The characters set in each font, which are the only ones a reader
	/// needs mapped back to text.
	used: [BTreeSet<char>; 4],
}

impl Writer<'_> {
	/// Spans from `x` along one line, each in its own font, with a rule under
	/// any that are underlined.
	fn draw(&mut self, content: &mut Content, mut x: f32, y: f32, spans: &[Span]) {
		for span in spans {
			let at = font(span.style);
			let mut glyphs = Vec::new();
			for c in span.text.chars() {
				self.used[at].insert(c);
				// A character the font lacks shows as its empty box.
				let glyph = self.faces[at].glyph_index(c).map_or(0, |glyph| glyph.0);
				glyphs.extend(glyph.to_be_bytes());
			}
			content
				.begin_text()
				.set_font(Name(FONTS[at].resource), SIZE)
				.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, y])
				.show(Str(&glyphs))
				.end_text();
			let wide = width(&span.text);
			if span.style.underline {
				content
					.set_line_width(0.6)
					.move_to(x, y - 1.5)
					.line_to(x + wide, y - 1.5)
					.stroke();
			}
			x += wide;
		}
	}

	/// The title a third of the way down, the credit under it, and the
	/// contact and draft at the foot, left and right.
	fn title_page(&mut self, book: &Book) -> Content {
		let mut content = Content::new();
		let mut centred = |writer: &mut Self, row: f32, text: &str| {
			let x = CENTRE - width(text) / 2.0;
			writer.draw(&mut content, x, baseline(row), &[plain(text)]);
		};
		centred(self, 17.0, title_of(book));
		let author = book.author.trim();
		if !author.is_empty() {
			centred(self, 20.0, "Written by");
			centred(self, 22.0, author);
		}

		let foot = (LINES_PER_PAGE - 1) as f32;
		let contact: Vec<&str> = book
			.contact
			.lines()
			.map(str::trim)
			.filter(|line| !line.is_empty())
			.collect();
		for (at, line) in contact.iter().enumerate() {
			let row = foot - (contact.len() - 1 - at) as f32;
			self.draw(&mut content, 1.5 * INCH, baseline(row), &[plain(line)]);
		}
		let draft = book.draft.trim();
		if !draft.is_empty() {
			self.draw(
				&mut content,
				RIGHT - width(draft),
				baseline(foot),
				&[plain(draft)],
			);
		}
		content
	}
}

/// One font, whole, with a map from its glyphs back to the characters drawn
/// in it, so text copied out of the PDF reads as written.
fn embed(
	out: &mut Pdf,
	at: usize,
	face: &Face,
	used: &BTreeSet<char>,
	id: Ref,
	mut next: impl FnMut() -> Ref,
) {
	let (cid, descriptor, file, cmap) = (next(), next(), next(), next());
	let font = &FONTS[at];
	let base = Name(font.base);
	let identity = SystemInfo {
		registry: Str(b"Adobe"),
		ordering: Str(b"Identity"),
		supplement: 0,
	};

	out.type0_font(id)
		.base_font(base)
		.encoding_predefined(Name(b"Identity-H"))
		.descendant_font(cid)
		.to_unicode(cmap);
	out.cid_font(cid)
		.subtype(CidFontType::Type2)
		.base_font(base)
		.system_info(identity)
		.font_descriptor(descriptor)
		.default_width(CHAR / SIZE * 1000.0)
		.cid_to_gid_map_predefined(Name(b"Identity"));

	let scale = 1000.0 / f32::from(face.units_per_em());
	let units = |value: i16| f32::from(value) * scale;
	let bbox = face.global_bounding_box();
	let mut flags = FontFlags::FIXED_PITCH | FontFlags::SERIF | FontFlags::NON_SYMBOLIC;
	if at % 2 == 1 {
		flags |= FontFlags::ITALIC;
	}
	out.font_descriptor(descriptor)
		.name(base)
		.flags(flags)
		.bbox(Rect::new(
			units(bbox.x_min),
			units(bbox.y_min),
			units(bbox.x_max),
			units(bbox.y_max),
		))
		.italic_angle(face.italic_angle())
		.ascent(units(face.ascender()))
		.descent(units(face.descender()))
		.cap_height(units(face.capital_height().unwrap_or(face.ascender())))
		.stem_v(80.0)
		.font_file2(file);
	// ponytail: the font goes in uncompressed, about 75 KB a style; deflate it
	// if exported scripts feel heavy.
	out.stream(file, font.data)
		.pair(Name(b"Length1"), font.data.len() as i32);

	let mut map = UnicodeCmap::new(Name(b"Custom"), identity);
	for &c in used {
		if let Some(glyph) = face.glyph_index(c) {
			map.pair(glyph.0, c);
		}
	}
	out.cmap(cmap, &map.finish());
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::script::fountain::parse;

	fn count(bytes: &[u8], needle: &[u8]) -> usize {
		bytes
			.windows(needle.len())
			.filter(|window| *window == needle)
			.count()
	}

	#[test]
	fn courier_prime_sets_ten_characters_to_the_inch() {
		for font in &FONTS {
			let face = Face::parse(font.data, 0).unwrap();
			let glyph = face.glyph_index('M').unwrap();
			let advance = f32::from(face.glyph_hor_advance(glyph).unwrap());
			// 1228 of 2048 units: under 0.3pt short across a whole line.
			assert!((advance / f32::from(face.units_per_em()) * SIZE - CHAR).abs() < 0.01);
		}
	}

	#[test]
	fn a_script_has_a_title_page_and_its_layout_pages() {
		let paragraphs: Vec<String> = (1..=40).map(|at| format!("Line {at}.")).collect();
		let scene = parse(&format!("INT. HALL - DAY\n\n{}", paragraphs.join("\n\n")));
		let pages = layout(std::slice::from_ref(&scene)).pages.len();
		assert_eq!(pages, 2);

		let bytes = pdf(&Book::new("Ithaca"), &[scene]);

		assert!(bytes.starts_with(b"%PDF-"));
		assert!(bytes.trim_ascii_end().ends_with(b"%%EOF"));
		assert_eq!(count(&bytes, b"/Type /Page\n"), 1 + pages);
		assert_eq!(count(&bytes, b"/Count 3"), 1);
	}

	#[test]
	fn only_the_styles_drawn_are_embedded() {
		let plain = pdf(&Book::new("Ithaca"), &[parse("Rain.")]);
		let bold = pdf(&Book::new("Ithaca"), &[parse("**Rain.**")]);

		assert_eq!(count(&plain, b"/FontFile2"), 1);
		assert_eq!(count(&bold, b"/FontFile2"), 2);
		assert_eq!(count(&bold, b"/BaseFont /CourierPrime-Bold"), 2);
	}
}
