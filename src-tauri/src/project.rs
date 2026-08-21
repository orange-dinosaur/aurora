use serde::{Deserialize, Serialize};

/// A folder inside a project, together with the document it starts life with.
pub struct Section {
	pub folder: &'static str,
	pub seed: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
	Novel,
	Screenplay,
	ShortStories,
	StagePlay,
}

const NOVEL: &[Section] = &[
	Section {
		folder: "Manuscript",
		seed: "Chapter 1.md",
	},
	Section {
		folder: "Outline",
		seed: "Outline.md",
	},
	Section {
		folder: "Characters",
		seed: "Characters.md",
	},
	Section {
		folder: "Locations",
		seed: "Locations.md",
	},
	Section {
		folder: "Notes",
		seed: "Notes.md",
	},
];

impl Format {
	/// The sections a new project of this format is created with. An empty
	/// layout means the format cannot be created yet.
	pub fn layout(self) -> &'static [Section] {
		match self {
			Format::Novel => NOVEL,
			Format::Screenplay | Format::ShortStories | Format::StagePlay => &[],
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn format_names_match_the_frontend() {
		assert_eq!(
			serde_json::to_string(&Format::ShortStories).unwrap(),
			"\"short-stories\""
		);
		assert_eq!(
			serde_json::from_str::<Format>("\"stage-play\"").unwrap(),
			Format::StagePlay
		);
	}

	#[test]
	fn unknown_format_is_rejected() {
		assert!(serde_json::from_str::<Format>("\"poetry\"").is_err());
	}

	#[test]
	fn novel_has_the_five_sections() {
		let folders: Vec<_> = Format::Novel.layout().iter().map(|s| s.folder).collect();
		assert_eq!(
			folders,
			["Manuscript", "Outline", "Characters", "Locations", "Notes"]
		);
	}

	#[test]
	fn every_section_seeds_a_markdown_file() {
		for section in Format::Novel.layout() {
			assert!(
				section.seed.ends_with(".md"),
				"{} seeds {}",
				section.folder,
				section.seed
			);
		}
	}

	#[test]
	fn the_other_formats_have_no_layout_yet() {
		for format in [Format::Screenplay, Format::ShortStories, Format::StagePlay] {
			assert!(
				format.layout().is_empty(),
				"{format:?} should not be creatable"
			);
		}
	}
}
