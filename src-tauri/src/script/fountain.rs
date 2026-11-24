//! A screenplay's body as blocks, one to each non-blank line. The frontend
//! holds the same reading in `src/fountain.ts`, and both are held to
//! `src/fountain.fixture.json`, so the page count and the exports see the
//! script the editor shows.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
	Heading,
	Action,
	Character,
	Dialogue,
	Parenthetical,
	Transition,
	Centered,
	PageBreak,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Block {
	pub kind: Kind,
	/// The line without its forcing mark.
	pub text: String,
	/// Whether a mark in the text decided the kind rather than Fountain's guess.
	pub forced: bool,
}

/// Which emphasis a stretch of a line is in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Style {
	pub bold: bool,
	pub italic: bool,
	pub underline: bool,
}

/// A stretch of a line in one emphasis, with its marks taken out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Span {
	pub text: String,
	#[serde(flatten)]
	pub style: Style,
}

// Checked in this order, so `**` is read as bold before `*` can be italic.
const MARKERS: [&str; 3] = ["**", "*", "_"];

fn flag<'a>(style: &'a mut Style, marker: &str) -> &'a mut bool {
	match marker {
		"**" => &mut style.bold,
		"*" => &mut style.italic,
		_ => &mut style.underline,
	}
}

/// Whether an unescaped `marker` comes at or after byte `from`.
fn closes(text: &str, from: usize, marker: &str) -> bool {
	let bytes = text.as_bytes();
	let mut i = from;
	while i < bytes.len() {
		if bytes[i] == b'\\' {
			i += 2;
		} else if bytes[i..].starts_with(marker.as_bytes()) {
			return true;
		} else {
			i += 1;
		}
	}
	false
}

/// Reads a line's emphasis as `spans` in `src/fountain.ts` does: `**bold**`,
/// `*italic*` and `_underline_`, nested as `***both***`, with `\*`, `\_` and
/// `\\` for the characters themselves. A marker with nothing to close it is
/// only a character.
pub fn spans(text: &str) -> Vec<Span> {
	let mut found = Vec::new();
	let mut style = Style::default();
	let mut run = String::new();
	let mut i = 0;
	while let Some(c) = text[i..].chars().next() {
		let rest = &text[i..];
		let escaped = rest
			.strip_prefix('\\')
			.and_then(|after| after.chars().next())
			.filter(|next| "*_\\".contains(*next));
		if let Some(escaped) = escaped {
			run.push(escaped);
			i += 2;
			continue;
		}
		let hit = MARKERS.into_iter().find(|marker| {
			let mut now = style;
			rest.starts_with(marker)
				&& (*flag(&mut now, marker) || closes(text, i + marker.len(), marker))
		});
		let Some(marker) = hit else {
			run.push(c);
			i += c.len_utf8();
			continue;
		};
		if !run.is_empty() {
			found.push(Span {
				text: std::mem::take(&mut run),
				style,
			});
		}
		let on = flag(&mut style, marker);
		*on = !*on;
		i += marker.len();
	}
	if !run.is_empty() {
		found.push(Span { text: run, style });
	}
	found
}

/// The ways a scene heading starts, in lower case. Each must be followed by a
/// full stop or a space.
const HEADINGS: [&str; 6] = ["int./ext", "int/ext", "int", "ext", "est", "i/e"];

fn heading(line: &str) -> bool {
	let lower = line.to_lowercase();
	HEADINGS.iter().any(|start| {
		lower
			.strip_prefix(start)
			.is_some_and(|rest| rest.starts_with(['.', ' ']))
	})
}

/// Every letter in capitals, and at least one letter. An extension such as
/// "(cont'd)" may be in any case, so only what comes before it counts.
fn shouting(line: &str) -> bool {
	let name = line.split('(').next().unwrap_or_default();
	name.chars().any(char::is_alphabetic) && name == name.to_uppercase()
}

fn parenthetical(line: &str) -> bool {
	line.len() >= 2 && line.starts_with('(') && line.ends_with(')')
}

fn centered(line: &str) -> Option<&str> {
	(line.len() >= 2 && line.starts_with('>') && line.ends_with('<'))
		.then(|| line[1..line.len() - 1].trim())
}

fn page_break(line: &str) -> bool {
	line.len() >= 3 && line.chars().all(|c| c == '=')
}

/// What follows a forcing mark. The trimmed line starts with the mark, so its
/// first appearance in the line is the mark itself.
fn after(line: &str, mark: char) -> String {
	line[line.find(mark).map_or(0, |at| at + 1)..].to_owned()
}

/// Reads a body into blocks. Each line is judged with the lines around it, as
/// Fountain does: a heading and a transition stand between blank lines, a cue
/// is a capitalised line with text straight under it, and what follows a cue up
/// to the next blank line is its dialogue.
pub fn parse(body: &str) -> Vec<Block> {
	let lines: Vec<&str> = body
		.split('\n')
		.map(|line| line.strip_suffix('\r').unwrap_or(line))
		.collect();
	let blank = |i: Option<usize>| {
		i.and_then(|i| lines.get(i))
			.is_none_or(|line| line.trim().is_empty())
	};
	let mut blocks = Vec::new();
	let mut speaking = false;

	for (i, &line) in lines.iter().enumerate() {
		let trimmed = line.trim();
		if trimmed.is_empty() {
			speaking = false;
			continue;
		}
		let before = blank(i.checked_sub(1));
		let next = blank(Some(i + 1));
		let mut block = |kind, text: String, forced| {
			blocks.push(Block { kind, text, forced });
		};

		if speaking {
			let kind = if parenthetical(trimmed) {
				Kind::Parenthetical
			} else {
				Kind::Dialogue
			};
			block(kind, line.to_owned(), false);
			continue;
		}

		if page_break(trimmed) {
			block(Kind::PageBreak, String::new(), false);
		} else if let Some(inner) = centered(trimmed) {
			block(Kind::Centered, inner.to_owned(), false);
		} else if trimmed.starts_with('!') {
			block(Kind::Action, after(line, '!'), true);
		} else if trimmed.starts_with('@') {
			block(Kind::Character, after(line, '@'), true);
			speaking = true;
		} else if trimmed.starts_with('>') {
			block(Kind::Transition, after(line, '>'), true);
		} else if trimmed.len() > 1 && trimmed.starts_with('.') && !trimmed[1..].starts_with('.') {
			block(Kind::Heading, after(line, '.'), true);
		} else if before && heading(trimmed) {
			block(Kind::Heading, line.to_owned(), false);
		} else if before && next && shouting(trimmed) && trimmed.ends_with("TO:") {
			block(Kind::Transition, line.to_owned(), false);
		} else if before && !next && shouting(trimmed) {
			block(Kind::Character, line.to_owned(), false);
			speaking = true;
		} else {
			block(Kind::Action, line.to_owned(), false);
		}
	}

	blocks
}

#[cfg(test)]
mod tests {
	use super::*;

	#[derive(Deserialize)]
	struct Case {
		name: String,
		text: String,
		blocks: Vec<Block>,
	}

	#[test]
	fn every_fixture_case_reads_as_the_frontend_reads_it() {
		let cases: Vec<Case> =
			serde_json::from_str(include_str!("../../../src/fountain.fixture.json")).unwrap();
		assert!(!cases.is_empty());
		for case in cases {
			assert_eq!(parse(&case.text), case.blocks, "{}", case.name);
		}
	}

	#[derive(Deserialize)]
	struct Emphasis {
		text: String,
		spans: Vec<Span>,
	}

	#[test]
	fn every_emphasis_case_reads_as_the_frontend_reads_it() {
		let cases: Vec<Emphasis> =
			serde_json::from_str(include_str!("../../../src/emphasis.fixture.json")).unwrap();
		assert!(!cases.is_empty());
		for case in cases {
			assert_eq!(spans(&case.text), case.spans, "{}", case.text);
		}
	}

	#[test]
	fn a_lone_full_stop_is_action() {
		assert_eq!(parse(".")[0].kind, Kind::Action);
	}
}
