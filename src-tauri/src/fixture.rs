//! Builds a throwaway project big enough to measure Aurora against.
//!
//!     cargo run --features fixture --bin fixture -- /tmp/aurora-big 400
//!
//! It goes through the same calls the app makes, so what it leaves behind
//! cannot drift from the manifest format. Two runs of the same arguments
//! produce the same project down to the last word, so two measurements taken a
//! week apart are of the same thing.
//!
//! The binary is behind the `fixture` feature and ships nowhere.

use std::path::PathBuf;

use aurora_lib::document::{
	NodeView, create_document, create_folder, document_tree, write_document,
};
use aurora_lib::project::{Format, create};
use aurora_lib::tree::FolderKind;
use time::OffsetDateTime;
use uuid::Uuid;

/// The moment every fixture project claims to have been created at, so two
/// runs differ in nothing.
const CREATED_AT: i64 = 1_700_000_000;

const PARTS: usize = 4;
const SCENES_PER_CHAPTER: usize = 5;
const CHARACTERS: usize = 40;
const LOCATIONS: usize = 20;

/// Words per scene, before the sentence that takes it over the line.
const SCENE_WORDS: usize = 1_100;
/// Words in the prose under a subject's front matter.
const SUBJECT_WORDS: usize = 200;

// Left as written: one word to a line is two hundred lines of nothing.
#[rustfmt::skip]
const WORDS: &[&str] = &[
	"the", "and", "of", "a", "to", "in", "she", "he", "it", "was", "had", "her", "his", "not",
	"but", "for", "with", "that", "then", "at", "on", "by", "from", "into", "over", "under",
	"through", "against", "before", "after", "again", "still", "never", "always", "morning",
	"evening", "night", "winter", "rain", "smoke", "river", "road", "window", "door", "table",
	"letter", "name", "voice", "hand", "eyes", "water", "stone", "iron", "bread", "candle",
	"thread", "coat", "boat", "field", "hill", "wall", "gate", "shadow", "silence", "answer",
	"question", "promise", "story", "reason", "waited", "watched", "turned", "spoke", "listened",
	"remembered", "forgot", "carried", "opened", "closed", "walked", "stood", "sat", "held",
	"knew", "thought", "wanted", "meant", "asked", "told", "said", "seemed", "looked", "felt",
	"cold", "quiet", "slow", "narrow", "empty", "heavy", "bright", "dark", "long", "small",
];

const FIRST: &[&str] = &[
	"Elena", "Tomas", "Wren", "Ada", "Piet", "Marika", "Josef", "Nell", "Bram", "Ines", "Otto",
	"Sylvie", "Rafe", "Dagny", "Milo", "Corin", "Hesper", "Ansel", "Lucia", "Emrys", "Tova",
	"Gideon", "Runa", "Casper",
];

const LAST: &[&str] = &[
	"Marsh", "Cole", "Vane", "Attwater", "Renn", "Halloway", "Brine", "Sorrel", "Quist", "Lark",
	"Ferrier", "Amble", "Weald", "Crane", "Pike", "Ostrand",
];

const PLACES: &[&str] = &[
	"Ashford", "Kell", "Norwater", "Bray", "Thistle", "Harrow", "Coldbeck", "Merrow", "Salt",
	"Wraye", "Elder", "Fenn", "Marlow", "Stray", "Rook", "Bell", "Corvin", "Nether", "Hollin",
	"Dray",
];

const FEATURES: &[&str] = &[
	"Bridge", "Mill", "Quay", "House", "Yard", "Chapel", "Crossing", "Row", "Field", "Landing",
];

#[rustfmt::skip]
const TAGS: &[&str] = &[
	"viewpoint", "act-one", "act-two", "act-three", "unresolved", "needs-a-pass", "the-fire",
	"the-letter", "house-marsh", "riverside", "flashback", "cut-candidate",
];

const TIES: &[&str] = &[
	"sister",
	"brother-in-law",
	"rival",
	"employer",
	"owes her money",
	"grew up two doors down",
	"keeps a room above the shop",
	"was there the night of the fire",
	"has not spoken to them in years",
	"writes to them and never sends it",
];

/// A person or a place, and the things the front matter of its file will say.
struct Subject {
	title: String,
	/// The other name the prose calls it by, which is what `names` holds.
	alias: String,
	tags: Vec<String>,
}

/// A linear congruential generator, so the fixture is the same every time
/// without a dependency for it.
struct Rng(u64);

impl Rng {
	fn new() -> Self {
		Self(0x5eed_1eaf_c0ff_ee01)
	}

	fn next(&mut self) -> u64 {
		self.0 = self
			.0
			.wrapping_mul(6_364_136_223_846_793_005)
			.wrapping_add(1_442_695_040_888_963_407);
		self.0 >> 33
	}

	/// A number below `bound`, which must not be zero.
	fn upto(&mut self, bound: usize) -> usize {
		(self.next() % bound as u64) as usize
	}

	fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
		&items[self.upto(items.len())]
	}
}

/// One sentence of filler, sometimes opening with someone's name so the
/// recognition sweep has real matching to do rather than empty prose.
fn sentence(rng: &mut Rng, cast: &[Subject]) -> String {
	let length = 8 + rng.upto(9);
	let mut words: Vec<String> = Vec::with_capacity(length);

	if rng.upto(3) == 0 {
		let subject = &cast[rng.upto(cast.len())];
		let named = if rng.upto(2) == 0 {
			&subject.title
		} else {
			&subject.alias
		};
		words.push(named.clone());
	}

	while words.len() < length {
		words.push((*rng.pick(WORDS)).to_string());
	}

	let mut sentence = words.join(" ");
	let first = sentence.remove(0).to_ascii_uppercase();
	sentence.insert(0, first);
	sentence.push('.');
	sentence
}

/// Paragraphs until `target` words have been written, and then the rest of the
/// sentence in hand.
fn prose(rng: &mut Rng, cast: &[Subject], target: usize) -> String {
	let mut text = String::new();
	let mut written = 0;

	while written < target {
		let mut paragraph = String::new();
		for _ in 0..3 + rng.upto(4) {
			if !paragraph.is_empty() {
				paragraph.push(' ');
			}
			paragraph.push_str(&sentence(rng, cast));
		}

		written += paragraph.split_whitespace().count();
		text.push_str(&paragraph);
		text.push_str("\n\n");
	}

	text
}

/// A few tags, without repeating one.
fn tags(rng: &mut Rng) -> Vec<String> {
	let mut chosen: Vec<String> = Vec::new();
	for _ in 0..1 + rng.upto(3) {
		let tag = (*rng.pick(TAGS)).to_string();
		if !chosen.contains(&tag) {
			chosen.push(tag);
		}
	}
	chosen
}

/// The people and places the manuscript is about. Characters first, then
/// locations, which is also the order they are filed in.
fn cast(rng: &mut Rng) -> Vec<Subject> {
	let mut cast = Vec::with_capacity(CHARACTERS + LOCATIONS);

	// The surname turns on every character rather than only when the first
	// names run out, or the cast is forty Marshes. The two banks are coprime in
	// length, so no pair repeats.
	for index in 0..CHARACTERS {
		let first = FIRST[index % FIRST.len()];
		let last = LAST[(index * 5) % LAST.len()];
		cast.push(Subject {
			title: format!("{first} {last}"),
			alias: first.to_string(),
			tags: tags(rng),
		});
	}

	for index in 0..LOCATIONS {
		let place = PLACES[index % PLACES.len()];
		let feature = FEATURES[(index * 3) % FEATURES.len()];
		cast.push(Subject {
			title: format!("{place} {feature}"),
			alias: place.to_string(),
			tags: tags(rng),
		});
	}

	cast
}

/// A subject's file: everything the Info panel would have written, and some
/// prose under it.
fn subject_text(rng: &mut Rng, mine: usize, cast: &[Subject]) -> String {
	let subject = &cast[mine];
	let mut text = String::from("---\nnames:\n");
	text.push_str(&format!("  - {}\n", subject.alias));

	text.push_str("tags:\n");
	for tag in &subject.tags {
		text.push_str(&format!("  - {tag}\n"));
	}

	text.push_str("relationships:\n");
	let mut tied: Vec<usize> = Vec::new();
	for _ in 0..1 + rng.upto(3) {
		let other = rng.upto(cast.len());
		if other != mine && !tied.contains(&other) {
			tied.push(other);
			text.push_str(&format!("  {}: {}\n", cast[other].title, rng.pick(TIES)));
		}
	}

	text.push_str(&format!("remarks: \"{}\"\n---\n\n", sentence(rng, cast)));
	text.push_str(&prose(rng, cast, SUBJECT_WORDS));
	text
}

/// A scene's file. Two scenes in three carry no front matter at all, which is
/// what a manuscript in progress looks like.
fn scene_text(rng: &mut Rng, number: usize, cast: &[Subject]) -> String {
	let mut text = String::new();

	if number.is_multiple_of(3) {
		text.push_str(&format!("---\nsynopsis: \"{}\"\n", sentence(rng, cast)));
		text.push_str("tags:\n");
		for tag in tags(rng) {
			text.push_str(&format!("  - {tag}\n"));
		}
		text.push_str("---\n\n");
	}

	text.push_str(&prose(rng, cast, SCENE_WORDS));
	text
}

/// The words the sidebar would print for the whole project, counted the way
/// `tree.ts` counts them: the sections added up.
fn words(tree: &[NodeView]) -> usize {
	tree.iter()
		.map(|node| match node {
			NodeView::Folder { words, .. } => *words,
			NodeView::Document(_) => 0,
		})
		.sum()
}

/// The id of the section with this name, which a new novel is guaranteed to
/// have.
fn section(tree: &[NodeView], wanted: &str) -> Uuid {
	tree.iter()
		.find_map(|node| match node {
			NodeView::Folder { id, name, .. } if name == wanted => Some(*id),
			_ => None,
		})
		.unwrap_or_else(|| panic!("a new novel has no {wanted} section"))
}

fn main() {
	let mut args = std::env::args().skip(1);
	let Some(destination) = args.next() else {
		eprintln!("usage: fixture <destination> [scenes]");
		std::process::exit(2);
	};
	let scenes: usize = match args.next() {
		Some(count) => count.parse().expect("the scene count must be a number"),
		None => 400,
	};
	assert!(scenes > 0, "a project needs at least one scene");

	let destination = PathBuf::from(destination);
	let name = destination
		.file_name()
		.and_then(|name| name.to_str())
		.expect("the destination must end in the name of the project");
	let parent = destination
		.parent()
		.filter(|parent| !parent.as_os_str().is_empty());
	let parent = std::fs::canonicalize(parent.unwrap_or_else(|| ".".as_ref()))
		.expect("the folder the project goes in must exist");

	let started = std::time::Instant::now();
	let created_at = OffsetDateTime::from_unix_timestamp(CREATED_AT).expect("a valid timestamp");
	let root =
		create(&parent, name, Format::Novel, created_at).expect("could not create the project");

	let tree = document_tree(root.clone()).expect("could not read the new project");
	let manuscript = section(&tree, "Manuscript");
	let characters = section(&tree, "Characters");
	let locations = section(&tree, "Locations");

	let mut rng = Rng::new();
	let cast = cast(&mut rng);

	for (index, subject) in cast.iter().enumerate() {
		let parent = if index < CHARACTERS {
			characters
		} else {
			locations
		};
		let made = create_document(root.clone(), parent, subject.title.clone())
			.expect("could not create a subject");
		let text = subject_text(&mut rng, index, &cast);
		write_document(root.clone(), made.id, text).expect("could not write a subject");
	}

	let chapters = scenes.div_ceil(SCENES_PER_CHAPTER);
	let mut chapter = 0;
	let mut scene = 0;

	for part in 0..PARTS {
		let made = create_folder(
			root.clone(),
			manuscript,
			format!("Part {}", part + 1),
			Some(FolderKind::Part),
		)
		.expect("could not create a part");
		let part_id = made.id();

		// The remainder goes to the parts at the front, so four parts hold
		// every chapter between them however the count divides.
		let mine = chapters / PARTS + usize::from(part < chapters % PARTS);

		for _ in 0..mine {
			chapter += 1;
			let made = create_folder(
				root.clone(),
				part_id,
				format!("Chapter {chapter}"),
				Some(FolderKind::Chapter),
			)
			.expect("could not create a chapter");
			let chapter_id = made.id();

			for _ in 0..SCENES_PER_CHAPTER {
				if scene == scenes {
					break;
				}
				scene += 1;

				let made = create_document(root.clone(), chapter_id, format!("Scene {scene}"))
					.expect("could not create a scene");
				let text = scene_text(&mut rng, scene, &cast);
				write_document(root.clone(), made.id, text).expect("could not write a scene");
			}
		}
	}

	let tree = document_tree(root.clone()).expect("could not read the finished project");

	println!(
		"built {} in {:.1}s",
		root.display(),
		started.elapsed().as_secs_f64()
	);
	println!("  {scene} scenes in {chapter} chapters across {PARTS} parts");
	println!("  {CHARACTERS} characters and {LOCATIONS} locations");
	println!("  {} words", words(&tree));
}
