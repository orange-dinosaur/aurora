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

/// A part a structure names for itself: its weight when chapters are shared
/// out, and its beats as a name and a line on what happens there.
struct Part {
	name: &'static str,
	weight: u32,
	beats: &'static [(&'static str, &'static str)],
}

const THREE_ACTS: &[Part] = &[
	Part {
		name: "Act One",
		weight: 1,
		beats: &[
			("Setup", "We meet the hero, their world and what they want."),
			("Inciting incident", "Something upsets that world."),
			(
				"First turning point",
				"The hero commits, and there is no going back.",
			),
		],
	},
	Part {
		name: "Act Two",
		weight: 2,
		beats: &[
			("Rising action", "Obstacles grow and the stakes climb."),
			(
				"Midpoint",
				"A reveal or reversal changes what the hero is fighting for.",
			),
			("Crisis", "Everything falls apart and the hero must choose."),
		],
	},
	Part {
		name: "Act Three",
		weight: 1,
		beats: &[
			("Climax", "The hero faces the central conflict head on."),
			("Resolution", "We see the world after, and what it cost."),
		],
	},
];

const FOUR_PARTS: &[Part] = &[
	Part {
		name: "Part One",
		weight: 1,
		beats: &[
			("Hook", "An opening that makes the reader ask a question."),
			("Setup", "The hero's life, wants and stakes."),
			(
				"First plot point",
				"The hero is pulled into the story's real conflict.",
			),
		],
	},
	Part {
		name: "Part Two",
		weight: 1,
		beats: &[
			("Reaction", "The hero runs, hides or flails."),
			("Pinch point", "The opposing force shows its strength."),
			(
				"Midpoint",
				"New knowledge turns the hero from reacting to acting.",
			),
		],
	},
	Part {
		name: "Part Three",
		weight: 1,
		beats: &[
			("Attack", "The hero takes the fight to the problem."),
			("Second pinch point", "The opposition strikes back harder."),
			("Second plot point", "The last piece falls into place."),
		],
	},
	Part {
		name: "Part Four",
		weight: 1,
		beats: &[
			(
				"Climax",
				"The hero confronts the opposition and wins or loses.",
			),
			("Resolution", "The story's question is answered."),
		],
	},
];

const SAVE_THE_CAT: &[Part] = &[
	Part {
		name: "Act One",
		weight: 1,
		beats: &[
			(
				"Opening image",
				"A picture of the hero's life before anything changes.",
			),
			(
				"Theme stated",
				"Someone hints at what the story is really about.",
			),
			("Setup", "The hero's world, flaws and what is missing."),
			("Catalyst", "One event knocks that world off course."),
			("Debate", "The hero hesitates over what to do."),
		],
	},
	Part {
		name: "Act Two",
		weight: 2,
		beats: &[
			(
				"Break into two",
				"The hero chooses and steps into a new world.",
			),
			("B story", "A new relationship that carries the theme."),
			("Fun and games", "The promise of the premise, played out."),
			(
				"Midpoint",
				"A false victory or false defeat raises the stakes.",
			),
			(
				"Bad guys close in",
				"Outside pressure and inner doubt tighten.",
			),
			("All is lost", "The lowest point, often with a loss."),
			(
				"Dark night of the soul",
				"The hero sits with the loss and finds what they missed.",
			),
		],
	},
	Part {
		name: "Act Three",
		weight: 1,
		beats: &[
			(
				"Break into three",
				"The hero sees the answer, often through the B story.",
			),
			("Finale", "The hero puts the lesson to work and wins."),
			(
				"Final image",
				"A mirror of the opening that shows the change.",
			),
		],
	},
];

const HEROS_JOURNEY: &[Part] = &[
	Part {
		name: "Departure",
		weight: 1,
		beats: &[
			("Ordinary world", "The hero's life as it is."),
			("Call to adventure", "A problem or invitation arrives."),
			("Refusal of the call", "Fear holds the hero back."),
			(
				"Meeting the mentor",
				"Someone gives advice, a gift or a push.",
			),
			("Crossing the threshold", "The hero leaves the known world."),
		],
	},
	Part {
		name: "Initiation",
		weight: 2,
		beats: &[
			(
				"Tests, allies and enemies",
				"The hero learns the new world's rules.",
			),
			("Approach", "The hero prepares for the greatest danger."),
			("Ordeal", "A brush with death or failure."),
			("Reward", "The hero takes what they came for."),
		],
	},
	Part {
		name: "Return",
		weight: 1,
		beats: &[
			("The road back", "The hero turns for home, pursued."),
			("Resurrection", "A final test proves the hero has changed."),
			(
				"Return with the elixir",
				"The hero comes home with something that heals their world.",
			),
		],
	},
];

impl Structure {
	/// Every structure, in the order the Start from screen offers them. Keep in
	/// step with the enum.
	pub const ALL: &'static [Structure] = &[
		Structure::ThreeActs,
		Structure::FourParts,
		Structure::SaveTheCat,
		Structure::HerosJourney,
		Structure::YourOwn,
	];

	/// The chapters the Start from screen suggests before the writer changes
	/// anything.
	fn default_chapters(self) -> u32 {
		match self {
			Structure::ThreeActs | Structure::FourParts => 24,
			Structure::SaveTheCat | Structure::HerosJourney => 20,
			Structure::YourOwn => 18,
		}
	}

	/// The parts this structure names for itself. Your own has none;
	/// `Start::parts` gives them.
	fn parts(self) -> &'static [Part] {
		match self {
			Structure::ThreeActs => THREE_ACTS,
			Structure::FourParts => FOUR_PARTS,
			Structure::SaveTheCat => SAVE_THE_CAT,
			Structure::HerosJourney => HEROS_JOURNEY,
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
				.map(|part| (part.name.to_owned(), part.weight))
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

const DEFAULT_PARTS: u32 = 3;
const DEFAULT_SCENES: u32 = 2;

/// A structure as the Start from screen needs to describe it, with the numbers
/// its fields start at. Field names follow `Start`, so a layout seeds one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StructureLayout {
	pub structure: Structure,
	/// Empty for Your own, whose parts are numbered as the writer asks.
	pub part_names: Vec<String>,
	pub parts: u32,
	pub chapters: u32,
	pub scenes: u32,
}

/// The structures and their starting numbers, so the frontend does not keep
/// its own copy of them.
#[tauri::command]
pub fn structure_layouts() -> Vec<StructureLayout> {
	Structure::ALL
		.iter()
		.map(|&structure| {
			let part_names: Vec<String> = structure
				.parts()
				.iter()
				.map(|part| part.name.to_owned())
				.collect();
			let parts = match structure {
				Structure::YourOwn => DEFAULT_PARTS,
				_ => part_names.len() as u32,
			};
			StructureLayout {
				structure,
				part_names,
				parts,
				chapters: structure.default_chapters(),
				scenes: DEFAULT_SCENES,
			}
		})
		.collect()
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

/// The project's `Outline.md` for this start: each part's name as a heading
/// with its beats under it. Your own has no beats, so its outline is empty.
pub fn outline(start: &Start) -> String {
	start
		.structure
		.parts()
		.iter()
		.map(|part| {
			let beats: String = part
				.beats
				.iter()
				.map(|(name, line)| format!("- **{name}.** {line}\n"))
				.collect();
			format!("## {}\n\n{beats}", part.name)
		})
		.collect::<Vec<_>>()
		.join("\n")
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
	fn layouts_come_in_order_with_their_defaults() {
		let layouts = structure_layouts();
		let order: Vec<Structure> = layouts.iter().map(|l| l.structure).collect();
		assert_eq!(order, Structure::ALL);
		assert_eq!(
			layouts[0],
			StructureLayout {
				structure: Structure::ThreeActs,
				part_names: vec!["Act One".into(), "Act Two".into(), "Act Three".into()],
				parts: 3,
				chapters: 24,
				scenes: 2,
			}
		);
		let your_own = &layouts[4];
		assert!(your_own.part_names.is_empty());
		assert_eq!((your_own.parts, your_own.chapters), (3, 18));
	}

	#[test]
	fn every_default_is_in_range() {
		for layout in structure_layouts() {
			let start = Start {
				structure: layout.structure,
				parts: layout.parts,
				chapters: layout.chapters,
				scenes: layout.scenes,
				front_matter: false,
				back_matter: false,
			};
			assert!(start.in_range(), "{:?}", layout.structure);
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
