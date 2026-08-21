use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

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

/// Bumped when the on-disk shape changes in a way older builds cannot read.
pub const MANIFEST_VERSION: u32 = 1;

/// The contents of `aurora.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
	pub version: u32,
	pub name: String,
	pub format: Format,
	#[serde(with = "time::serde::rfc3339")]
	pub created_at: OffsetDateTime,
	/// The section folders as they were at creation, so a project keeps its own
	/// layout even if the format's definition changes later.
	pub folders: Vec<String>,
}

impl Manifest {
	pub fn new(name: impl Into<String>, format: Format, created_at: OffsetDateTime) -> Self {
		Self {
			version: MANIFEST_VERSION,
			name: name.into(),
			format,
			created_at,
			folders: format
				.layout()
				.iter()
				.map(|s| s.folder.to_owned())
				.collect(),
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

	fn fixed_time() -> OffsetDateTime {
		OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
	}

	#[test]
	fn manifest_takes_its_folders_from_the_layout() {
		let manifest = Manifest::new("Wuthering Heights", Format::Novel, fixed_time());
		assert_eq!(manifest.version, MANIFEST_VERSION);
		assert_eq!(
			manifest.folders,
			["Manuscript", "Outline", "Characters", "Locations", "Notes"]
		);
	}

	#[test]
	fn manifest_serializes_with_camel_case_keys() {
		let manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());
		let json = serde_json::to_value(&manifest).unwrap();
		assert_eq!(json["version"], 1);
		assert_eq!(json["name"], "Ithaca");
		assert_eq!(json["format"], "novel");
		assert_eq!(json["createdAt"], "2023-11-14T22:13:20Z");
		assert!(json.get("created_at").is_none());
	}

	#[test]
	fn manifest_round_trips() {
		let manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());
		let json = serde_json::to_string(&manifest).unwrap();
		assert_eq!(serde_json::from_str::<Manifest>(&json).unwrap(), manifest);
	}

	#[test]
	fn a_manifest_missing_a_field_is_rejected() {
		let json = r#"{"version":1,"name":"Ithaca","format":"novel","folders":[]}"#;
		assert!(serde_json::from_str::<Manifest>(json).is_err());
	}
}
