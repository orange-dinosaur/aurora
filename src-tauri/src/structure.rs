//! The shapes a new novel can start from: parts, chapters and scenes laid out
//! before the writer has written a word.

use serde::{Deserialize, Serialize};

use crate::project::{BACK_MATTER, FRONT_MATTER};
use crate::tree::{FolderKind, Node};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum Structure {
	ThreeActs,
	FourParts,
	SaveTheCat,
	HerosJourney,
	YourOwn,
}

/// What the writer chose on the Start from screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Start {
	pub structure: Structure,
	/// Read for Your own only. Every other structure names its own parts.
	pub parts: u32,
	/// The total across the book, not per part.
	pub chapters: u32,
	/// Per chapter. Zero makes each chapter a single document.
	pub scenes: u32,
	pub front_matter: bool,
	pub back_matter: bool,
}

pub const MAX_PARTS: u32 = 10;
pub const MAX_CHAPTERS: u32 = 99;
pub const MAX_SCENES: u32 = 20;

const ORDINALS: [&str; MAX_PARTS as usize] = [
	"One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten",
];

impl Structure {
	/// Each part's name and its weight when chapters are shared out. Your own
	/// has no parts of its own; `Start::parts` gives them.
	fn parts(self) -> &'static [(&'static str, u32)] {
		match self {
			Structure::ThreeActs | Structure::SaveTheCat => {
				&[("Act One", 1), ("Act Two", 2), ("Act Three", 1)]
			}
			Structure::FourParts => &[
				("Part One", 1),
				("Part Two", 1),
				("Part Three", 1),
				("Part Four", 1),
			],
			Structure::HerosJourney => &[("Departure", 1), ("Initiation", 2), ("Return", 1)],
			Structure::YourOwn => &[],
		}
	}
}

impl Start {
	/// The parts this start makes, with their weights.
	fn parts(&self) -> Vec<(String, u32)> {
		match self.structure {
			Structure::YourOwn => ORDINALS
				.iter()
				.take(self.parts as usize)
				.map(|n| (format!("Part {n}"), 1))
				.collect(),
			fixed => fixed
				.parts()
				.iter()
				.map(|&(name, weight)| (name.to_owned(), weight))
				.collect(),
		}
	}

	/// Whether every number is one the Start from screen could have offered.
	/// The dialog keeps to the same ranges, but a command argument is not
	/// trusted to.
	pub fn in_range(&self) -> bool {
		let parts = match self.structure {
			Structure::YourOwn => self.parts,
			fixed => fixed.parts().len() as u32,
		};
		(1..=MAX_PARTS).contains(&parts)
			&& (parts..=MAX_CHAPTERS).contains(&self.chapters)
			&& self.scenes <= MAX_SCENES
	}
}

/// Shares `chapters` out across parts in proportion to their weights. Each part
/// takes the whole chapters of its share, and what is left goes one at a time
/// to the largest remainders, ties to the heavier part, then the earlier one.
pub fn split(chapters: u32, weights: &[u32]) -> Vec<u32> {
	let total: u32 = weights.iter().sum();
	let mut counts: Vec<u32> = weights.iter().map(|w| chapters * w / total).collect();
	let left = chapters - counts.iter().sum::<u32>();

	let mut order: Vec<usize> = (0..weights.len()).collect();
	// A stable sort keeps equal parts left to right.
	order.sort_by_key(|&i| std::cmp::Reverse((chapters * weights[i] % total, weights[i])));
	for &i in order.iter().take(left as usize) {
		counts[i] += 1;
	}
	counts
}

/// The Manuscript's children for this start: front matter if asked, the parts,
/// then back matter if asked. Chapters number straight through the book and
/// scenes restart in each chapter. Assumes `in_range`.
pub fn manuscript(start: &Start) -> Vec<Node> {
	let mut nodes = Vec::new();
	if start.front_matter {
		nodes.push(matter(
			FolderKind::FrontMatter,
			FRONT_MATTER,
			&["Dedication.md", "Epigraph.md"],
		));
	}
	nodes.extend(parts(start));
	if start.back_matter {
		nodes.push(matter(
			FolderKind::BackMatter,
			BACK_MATTER,
			&["Acknowledgements.md", "About the author.md"],
		));
	}
	nodes
}

fn matter(kind: FolderKind, name: &str, documents: &[&str]) -> Node {
	let documents = documents.iter().map(|name| Node::document(name)).collect();
	Node::folder_of(Some(kind), name, documents)
}

fn parts(start: &Start) -> Vec<Node> {
	let parts = start.parts();
	let weights: Vec<u32> = parts.iter().map(|&(_, w)| w).collect();
	let mut number = 0;

	parts
		.iter()
		.zip(split(start.chapters, &weights))
		.map(|((name, _), count)| {
			let chapters = (0..count)
				.map(|_| {
					number += 1;
					if start.scenes == 0 {
						return Node::document(&format!("Chapter {number}.md"));
					}
					let scenes = (1..=start.scenes)
						.map(|s| Node::document(&format!("Scene {s}.md")))
						.collect();
					Node::folder_of(
						Some(FolderKind::Chapter),
						&format!("Chapter {number}"),
						scenes,
					)
				})
				.collect();
			Node::folder_of(Some(FolderKind::Part), name, chapters)
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn start(structure: Structure, parts: u32, chapters: u32, scenes: u32) -> Start {
		Start {
			structure,
			parts,
			chapters,
			scenes,
			front_matter: false,
			back_matter: false,
		}
	}

	#[test]
	fn three_acts_split_by_largest_remainder() {
		assert_eq!(split(24, &[1, 2, 1]), [6, 12, 6]);
		assert_eq!(split(25, &[1, 2, 1]), [6, 13, 6]);
		assert_eq!(split(26, &[1, 2, 1]), [7, 13, 6]);
	}

	#[test]
	fn four_parts_give_leftovers_left_to_right() {
		assert_eq!(split(26, &[1, 1, 1, 1]), [7, 7, 6, 6]);
	}

	#[test]
	fn chapters_equal_to_parts_give_one_each() {
		assert_eq!(split(3, &[1, 2, 1]), [1, 1, 1]);
		assert_eq!(split(4, &[1, 1, 1, 1]), [1, 1, 1, 1]);
		assert_eq!(split(10, &[1; 10]), [1; 10]);
	}

	#[test]
	fn your_own_names_as_many_parts_as_asked() {
		let names: Vec<String> = manuscript(&start(Structure::YourOwn, 3, 3, 0))
			.iter()
			.map(|n| n.name().to_owned())
			.collect();
		assert_eq!(names, ["Part One", "Part Two", "Part Three"]);
	}

	#[test]
	fn ranges_are_checked() {
		assert!(start(Structure::ThreeActs, 0, 3, 0).in_range());
		assert!(start(Structure::ThreeActs, 0, 99, 20).in_range());
		assert!(!start(Structure::ThreeActs, 0, 2, 2).in_range());
		assert!(!start(Structure::ThreeActs, 0, 100, 2).in_range());
		assert!(!start(Structure::ThreeActs, 0, 24, 21).in_range());
		assert!(!start(Structure::YourOwn, 0, 18, 2).in_range());
		assert!(!start(Structure::YourOwn, 11, 18, 2).in_range());
		assert!(!start(Structure::YourOwn, 4, 3, 2).in_range());
	}
}
