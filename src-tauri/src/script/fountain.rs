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

	#[test]
	fn a_lone_full_stop_is_action() {
		assert_eq!(parse(".")[0].kind, Kind::Action);
	}
}
