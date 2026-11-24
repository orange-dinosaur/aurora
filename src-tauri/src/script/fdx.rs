//! A screenplay as a Final Draft file: one XML document, a paragraph to each
//! Fountain block, with the title page Final Draft keeps beside the script.

use super::fountain::{Block, Kind, Span, spans};
use crate::book::{escaped, title_of};
use crate::project::Book;

/// The rows of the title page the PDF uses, so the two look alike.
const TITLE_ROW: usize = 17;
const FOOT_ROW: usize = 54;

/// The script with its title page, as the text of an `.fdx` file.
pub fn fdx(book: &Book, scenes: &[Vec<Block>]) -> String {
	let mut script = String::new();
	let mut new_page = false;
	for block in scenes.iter().flatten() {
		let kind = match block.kind {
			Kind::PageBreak => {
				new_page = true;
				continue;
			}
			Kind::Heading => "Scene Heading",
			Kind::Action | Kind::Centered => "Action",
			Kind::Character => "Character",
			Kind::Dialogue => "Dialogue",
			Kind::Parenthetical => "Parenthetical",
			Kind::Transition => "Transition",
		};
		let mut attributes = format!(" Type=\"{kind}\"");
		if block.kind == Kind::Centered {
			attributes.push_str(" Alignment=\"Center\"");
		}
		if std::mem::take(&mut new_page) {
			attributes.push_str(" StartsNewPage=\"Yes\"");
		}
		script.push_str(&paragraph(&attributes, &spans(&block.text)));
	}

	format!(
		"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>\n\
		<FinalDraft DocumentType=\"Script\" Template=\"No\" Version=\"1\">\n\
		<Content>\n{script}</Content>\n\
		<TitlePage>\n<Content>\n{}</Content>\n</TitlePage>\n\
		</FinalDraft>\n",
		title_page(book)
	)
}

fn paragraph(attributes: &str, spans: &[Span]) -> String {
	let texts: String = spans
		.iter()
		.map(|span| {
			let style: Vec<&str> = [
				(span.style.bold, "Bold"),
				(span.style.italic, "Italic"),
				(span.style.underline, "Underline"),
			]
			.into_iter()
			.filter_map(|(on, name)| on.then_some(name))
			.collect();
			let style = if style.is_empty() {
				String::new()
			} else {
				format!(" Style=\"{}\"", style.join("+"))
			};
			format!("<Text{style}>{}</Text>", escaped(&span.text))
		})
		.collect();
	format!("<Paragraph{attributes}>{texts}</Paragraph>\n")
}

/// Laid out in rows with empty paragraphs, as Final Draft's own title pages
/// are: the title a third of the way down with the credit under it, and the
/// contact and draft at the foot.
fn title_page(book: &Book) -> String {
	let mut rows: Vec<(&str, String)> = Vec::new();
	let mut put = |row: usize, align: &'static str, text: &str| {
		while rows.len() < row {
			rows.push(("Left", String::new()));
		}
		rows.push((align, text.to_owned()));
	};
	put(TITLE_ROW, "Center", title_of(book));
	let author = book.author.trim();
	if !author.is_empty() {
		put(TITLE_ROW + 3, "Center", "Written by");
		put(TITLE_ROW + 5, "Center", author);
	}
	let contact: Vec<&str> = book
		.contact
		.lines()
		.map(str::trim)
		.filter(|line| !line.is_empty())
		.collect();
	let draft = book.draft.trim();
	let below = contact.len() + usize::from(!draft.is_empty());
	let mut row = FOOT_ROW + 1 - below;
	for line in contact {
		put(row, "Left", line);
		row += 1;
	}
	if !draft.is_empty() {
		put(row, "Right", draft);
	}

	rows.iter()
		.map(|(align, text)| {
			let text = if text.is_empty() {
				Vec::new()
			} else {
				vec![super::layout::plain(text)]
			};
			paragraph(&format!(" Alignment=\"{align}\""), &text)
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::script::fountain::parse;

	#[test]
	fn each_block_is_a_paragraph_of_its_kind() {
		let scene = parse(
			"INT. HALL - DAY\n\nRose & the _old_ **dog** wait.\n\nROSE\n(quietly)\nHello.\n\n===\n\n> THE END <",
		);

		let file = fdx(&Book::new("Ithaca"), &[scene]);
		let script = &file[file.find("<Content>").unwrap()..file.find("<TitlePage>").unwrap()];

		assert_eq!(
			script,
			"<Content>\n\
			<Paragraph Type=\"Scene Heading\"><Text>INT. HALL - DAY</Text></Paragraph>\n\
			<Paragraph Type=\"Action\"><Text>Rose &amp; the </Text>\
			<Text Style=\"Underline\">old</Text><Text> </Text>\
			<Text Style=\"Bold\">dog</Text><Text> wait.</Text></Paragraph>\n\
			<Paragraph Type=\"Character\"><Text>ROSE</Text></Paragraph>\n\
			<Paragraph Type=\"Parenthetical\"><Text>(quietly)</Text></Paragraph>\n\
			<Paragraph Type=\"Dialogue\"><Text>Hello.</Text></Paragraph>\n\
			<Paragraph Type=\"Action\" Alignment=\"Center\" StartsNewPage=\"Yes\">\
			<Text>THE END</Text></Paragraph>\n\
			</Content>\n"
		);
		assert!(file.starts_with("<?xml"));
		assert!(file.ends_with("</FinalDraft>\n"));
	}

	#[test]
	fn the_title_page_puts_the_credit_under_the_title_and_the_contact_at_the_foot() {
		let mut book = Book::new("Ithaca");
		book.author = "Penelope".to_owned();
		book.contact = "Argos\n555 0100".to_owned();
		book.draft = "Second draft".to_owned();

		let page: Vec<String> = title_page(&book).lines().map(str::to_owned).collect();

		assert_eq!(page.len(), FOOT_ROW + 1);
		let row = |at: usize| page[at].as_str();
		assert_eq!(
			row(TITLE_ROW),
			"<Paragraph Alignment=\"Center\"><Text>Ithaca</Text></Paragraph>"
		);
		assert_eq!(
			row(TITLE_ROW + 5),
			"<Paragraph Alignment=\"Center\"><Text>Penelope</Text></Paragraph>"
		);
		assert_eq!(
			row(FOOT_ROW - 2),
			"<Paragraph Alignment=\"Left\"><Text>Argos</Text></Paragraph>"
		);
		assert_eq!(
			row(FOOT_ROW),
			"<Paragraph Alignment=\"Right\"><Text>Second draft</Text></Paragraph>"
		);
		assert_eq!(row(0), "<Paragraph Alignment=\"Left\"></Paragraph>");
	}
}
