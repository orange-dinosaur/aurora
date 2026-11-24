//! A screenplay set on US Letter pages the way afterwriting sets it
//! (`js/utils/print-profiles.js` in ifrost/afterwriting-labs): 55 lines a page
//! of Courier 12pt, each block wrapped at its own width. The page counts and
//! the PDF both read this, so they always agree.

use super::fountain::{Block, Kind};

pub const LINES_PER_PAGE: usize = 55;

/// How many characters a line of this block holds.
pub fn width(kind: Kind) -> usize {
	match kind {
		Kind::Heading | Kind::Action | Kind::Transition => 61,
		Kind::Character => 33,
		Kind::Dialogue => 36,
		Kind::Parenthetical => 26,
		Kind::Centered => 58,
		Kind::PageBreak => 0,
	}
}

/// One printed line. A blank line between blocks is an empty action line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
	pub kind: Kind,
	pub text: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Layout {
	pub pages: Vec<Vec<Line>>,
	/// How many lines each scene fills, in the order the scenes were given, so
	/// a scene's share of a page is its count over `LINES_PER_PAGE`.
	pub shares: Vec<usize>,
}

/// Lays out the scenes one after another, as they run in the script.
pub fn layout(scenes: &[Vec<Block>]) -> Layout {
	let mut pages = Pages {
		pages: Vec::new(),
		current: Vec::new(),
		shares: vec![0; scenes.len()],
	};
	let pieces: Vec<Piece> = scenes
		.iter()
		.enumerate()
		.flat_map(|(scene, blocks)| pieces(scene, blocks))
		.collect();

	for (at, piece) in pieces.iter().enumerate() {
		let Some(first) = piece.lines.first() else {
			if piece.kind == Kind::PageBreak {
				pages.turn();
			}
			continue;
		};
		match first.kind {
			Kind::Character => pages.speech(piece.scene, piece.lines.clone()),
			Kind::Heading => {
				// A heading keeps a blank line and the start of what follows it
				// on its page, or goes over with them.
				let next = pieces.get(at + 1).map_or(0, |next| next.lines.len());
				let keep = if next == 0 { 0 } else { 1 + next.min(2) };
				pages.place(piece.scene, piece.lines.clone(), keep);
			}
			_ => pages.place(piece.scene, piece.lines.clone(), 0),
		}
	}

	pages.turn();
	Layout {
		pages: pages.pages,
		shares: pages.shares,
	}
}

/// What is laid out as one: a block, or a cue with the speech under it.
struct Piece {
	scene: usize,
	kind: Kind,
	lines: Vec<Line>,
}

fn pieces(scene: usize, blocks: &[Block]) -> Vec<Piece> {
	let mut pieces: Vec<Piece> = Vec::new();
	for block in blocks {
		let lines = wrap(&block.text, width(block.kind))
			.into_iter()
			.map(|text| Line {
				kind: block.kind,
				text,
			});
		let spoken = matches!(block.kind, Kind::Dialogue | Kind::Parenthetical);
		match pieces.last_mut() {
			Some(speech) if spoken && speech.kind == Kind::Character => speech.lines.extend(lines),
			_ => pieces.push(Piece {
				scene,
				kind: block.kind,
				lines: lines.collect(),
			}),
		}
	}
	pieces
}

/// Words filled into lines of `width` characters. A word longer than a line is
/// cut where the line ends.
fn wrap(text: &str, width: usize) -> Vec<String> {
	let mut lines = Vec::new();
	let mut line = String::new();
	for word in text.split_whitespace() {
		let mut word: Vec<char> = word.chars().collect();
		while word.len() > width {
			if !line.is_empty() {
				lines.push(std::mem::take(&mut line));
			}
			lines.push(word.drain(..width).collect());
		}
		if word.is_empty() {
			continue;
		}
		let used = line.chars().count();
		if used > 0 && used + 1 + word.len() > width {
			lines.push(std::mem::take(&mut line));
		}
		if !line.is_empty() {
			line.push(' ');
		}
		line.extend(word);
	}
	if !line.is_empty() {
		lines.push(line);
	}
	lines
}

fn blank() -> Line {
	Line {
		kind: Kind::Action,
		text: String::new(),
	}
}

/// Whether a line of speech ends a sentence, where a speech may break.
fn ends_sentence(line: &Line) -> bool {
	line.kind == Kind::Dialogue
		&& line
			.text
			.trim_end_matches(['"', '\'', ')', '\u{201d}', '\u{2019}'])
			.ends_with(['.', '?', '!', '\u{2026}'])
}

/// The cue again at the top of the next page.
fn continued(cue: &Line) -> Line {
	let text = if cue.text.to_uppercase().contains("(CONT'D)") {
		cue.text.clone()
	} else {
		format!("{} (CONT'D)", cue.text)
	};
	Line {
		kind: Kind::Character,
		text,
	}
}

struct Pages {
	pages: Vec<Vec<Line>>,
	current: Vec<Line>,
	shares: Vec<usize>,
}

impl Pages {
	fn room(&self) -> usize {
		LINES_PER_PAGE.saturating_sub(self.current.len())
	}

	/// The blank line a block needs above it, which the top of a page does not.
	fn gap(&self) -> usize {
		usize::from(!self.current.is_empty())
	}

	fn turn(&mut self) {
		if !self.current.is_empty() {
			self.pages.push(std::mem::take(&mut self.current));
		}
	}

	fn put(&mut self, scene: usize, lines: Vec<Line>) {
		if !self.current.is_empty() {
			self.current.push(blank());
			self.shares[scene] += 1;
		}
		self.shares[scene] += lines.len();
		self.current.extend(lines);
	}

	/// A block that is not speech. It may run over a page with at least two
	/// lines either side, unless `keep` lines must follow it on its page.
	fn place(&mut self, scene: usize, mut lines: Vec<Line>, keep: usize) {
		loop {
			let room = self.room();
			if self.gap() + lines.len() + keep <= room {
				self.put(scene, lines);
				return;
			}
			if self.current.is_empty() {
				if lines.len() <= room {
					self.put(scene, lines);
					return;
				}
				let rest = lines.split_off(room);
				self.put(scene, lines);
				self.turn();
				lines = rest;
				continue;
			}
			let fits = room.saturating_sub(self.gap());
			if keep == 0 && fits >= 2 && lines.len() >= fits + 2 {
				let rest = lines.split_off(fits);
				self.put(scene, lines);
				self.turn();
				lines = rest;
				continue;
			}
			self.turn();
		}
	}

	/// A cue and its speech. A speech that does not fit breaks after a line
	/// that ends a sentence, with two lines of it each side, `(MORE)` at the
	/// foot and the cue again with `(CONT'D)` at the top of the next page.
	fn speech(&mut self, scene: usize, mut lines: Vec<Line>) {
		let cue = lines[0].clone();
		loop {
			let room = self.room().saturating_sub(self.gap());
			if lines.len() <= room {
				self.put(scene, lines);
				return;
			}
			// `at` lines stay on this page, the cue among them, and `(MORE)`
			// takes one more.
			let fits = (3..=lines.len().saturating_sub(2))
				.rev()
				.find(|&at| at < room && ends_sentence(&lines[at - 1]));
			let at = match fits {
				Some(at) => at,
				// ponytail: a speech longer than a page with no sentence end
				// to break at is cut wherever the page ends.
				None if self.current.is_empty() => room - 1,
				None => {
					self.turn();
					continue;
				}
			};
			let rest = lines.split_off(at);
			lines.push(Line {
				kind: Kind::Character,
				text: "(MORE)".to_owned(),
			});
			self.put(scene, lines);
			self.turn();
			lines = std::iter::once(continued(&cue)).chain(rest).collect();
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::script::fountain::parse;

	fn texts(page: &[Line]) -> Vec<&str> {
		page.iter().map(|line| line.text.as_str()).collect()
	}

	/// `count` one-line action paragraphs, which fill `2 * count - 1` lines.
	fn actions(count: usize) -> String {
		(1..=count)
			.map(|at| format!("Line {at}."))
			.collect::<Vec<_>>()
			.join("\n\n")
	}

	#[test]
	fn a_short_scene_is_one_page_with_a_blank_between_blocks() {
		let scene = parse("INT. KITCHEN - NIGHT\n\nRain.\n\nELENA\n(quietly)\nHello.");
		let laid = layout(&[scene]);
		assert_eq!(laid.pages.len(), 1);
		assert_eq!(
			texts(&laid.pages[0]),
			[
				"INT. KITCHEN - NIGHT",
				"",
				"Rain.",
				"",
				"ELENA",
				"(quietly)",
				"Hello."
			]
		);
		assert_eq!(laid.shares, [7]);
	}

	#[test]
	fn each_block_wraps_at_its_own_width() {
		let words = "word ".repeat(20);
		let scene = parse(&format!("{words}\n\nELENA\n{words}"));
		let laid = layout(&[scene]);
		let page = &laid.pages[0];
		assert!(
			page.iter()
				.all(|line| line.text.chars().count() <= width(line.kind))
		);
		// 20 five-character words: 12 to an action line, 7 to a dialogue line.
		assert_eq!(
			page.iter()
				.filter(|line| line.kind == Kind::Action && !line.text.is_empty())
				.count(),
			2
		);
		assert_eq!(
			page.iter()
				.filter(|line| line.kind == Kind::Dialogue)
				.count(),
			3
		);
	}

	#[test]
	fn a_word_longer_than_a_line_is_cut() {
		assert_eq!(wrap(&"x".repeat(70), 61), ["x".repeat(61), "x".repeat(9)]);
	}

	#[test]
	fn pages_hold_55_lines_and_shares_add_up() {
		let scenes: Vec<Vec<Block>> = (0..40)
			.map(|at| parse(&format!("INT. ROOM {at} - DAY\n\n{}", actions(3))))
			.collect();
		let laid = layout(&scenes);
		assert!(laid.pages.iter().all(|page| page.len() <= LINES_PER_PAGE));
		let lines: usize = laid.pages.iter().map(Vec::len).sum();
		assert_eq!(laid.shares.iter().sum::<usize>(), lines);
		// Each scene is 7 lines and a blank, so 40 of them run past five pages.
		assert_eq!(laid.pages.len(), 6);
	}

	#[test]
	fn a_heading_is_never_left_at_the_foot_of_a_page() {
		// 27 paragraphs fill 53 lines: the heading would fit as the 55th, but
		// nothing of its scene would.
		let laid = layout(&[parse(&format!(
			"{}\n\nINT. HALL - DAY\n\nDoors.",
			actions(27)
		))]);
		assert_eq!(laid.pages[0].len(), 53);
		assert_eq!(texts(&laid.pages[1]), ["INT. HALL - DAY", "", "Doors."]);
	}

	#[test]
	fn a_speech_breaks_at_a_sentence_with_more_and_contd() {
		// 25 paragraphs fill 49 lines, leaving 5 after the blank: the cue, three
		// lines of speech and (MORE).
		let speech = "ELENA\nOne.\nTwo.\nThree.\nFour.\nFive.";
		let laid = layout(&[parse(&format!("{}\n\n{speech}", actions(25)))]);
		assert_eq!(laid.pages.len(), 2);
		assert_eq!(laid.pages[0].len(), LINES_PER_PAGE);
		assert_eq!(
			texts(&laid.pages[0][50..]),
			["ELENA", "One.", "Two.", "Three.", "(MORE)"]
		);
		assert_eq!(texts(&laid.pages[1]), ["ELENA (CONT'D)", "Four.", "Five."]);
	}

	#[test]
	fn a_speech_with_nowhere_to_break_goes_over_whole() {
		let speech = "ELENA\nOne and\ntwo and\nthree and\nfour and\nfive";
		let laid = layout(&[parse(&format!("{}\n\n{speech}", actions(25)))]);
		assert_eq!(laid.pages[0].len(), 49);
		assert_eq!(laid.pages[1].len(), 6);
		assert_eq!(laid.pages[1][0].text, "ELENA");
	}

	#[test]
	fn a_page_break_starts_a_new_page() {
		let laid = layout(&[parse("Before.\n\n===\n\nAfter.")]);
		assert_eq!(laid.pages.len(), 2);
		assert_eq!(texts(&laid.pages[1]), ["After."]);
	}

	#[test]
	fn scenes_run_on_and_each_keeps_its_own_share() {
		let laid = layout(&[parse("INT. A - DAY\n\nOne."), parse("INT. B - DAY\n\nTwo.")]);
		assert_eq!(laid.pages.len(), 1);
		// The blank line between the scenes belongs to the second.
		assert_eq!(laid.shares, [3, 4]);
	}
}
