use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use tauri::AppHandle;

use crate::project::{Error, Result, store_path, write_json};

pub const STORE_VERSION: u32 = 5;

/// How many projects are worth offering on the welcome screen.
const MAX_RECENT: usize = 10;

/// How the writer likes to write, which follows them into every project rather
/// than belonging to any one of them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
	/// Whether the bar under the document title is showing.
	pub toolbar: bool,
	/// Whether everything but the paragraph being written is dimmed.
	pub focus: bool,
	/// Whether the line the caret is on is held in place and the page moves
	/// under it. Defaulted on its own, so a store written before this field
	/// existed still reads rather than falling back to an empty one.
	#[serde(default)]
	pub typewriter: bool,
	/// Whether the document's headings are listed in a column beside the text.
	/// Defaulted for the same reason as the field above it.
	#[serde(default)]
	pub outline: bool,
	/// Whether the list of documents is showing beside the writing. Needs a
	/// default of its own: an older store has no such field, and falling back
	/// to `false` would open Aurora with the sidebar gone.
	#[serde(default = "shown")]
	pub sidebar: bool,
	/// The width of the column of text, in characters.
	pub measure: u32,
	/// In pixels.
	pub font_size: u32,
	/// A multiple of the font size, as CSS takes it.
	pub line_height: f32,
}

/// serde takes a field's default from a function, not from the type's `Default`.
fn shown() -> bool {
	true
}

impl Default for Preferences {
	fn default() -> Self {
		Self {
			toolbar: true,
			focus: false,
			typewriter: false,
			outline: false,
			sidebar: shown(),
			measure: 68,
			font_size: 16,
			line_height: 1.7,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
	pub name: String,
	pub root: PathBuf,
	#[serde(with = "time::serde::rfc3339")]
	pub last_opened: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Store {
	pub version: u32,
	/// The project to reopen on launch, which is not simply the newest in the
	/// list: closing a project clears this and leaves the list alone, and
	/// forgetting one has to clear it rather than let the next project inherit
	/// the marker.
	#[serde(default)]
	pub last: Option<PathBuf>,
	pub recent: Vec<RecentProject>,
	/// Absent from a store written before version 3, and defaulted rather than
	/// migrated: there is nothing in an older store to derive them from.
	#[serde(default)]
	pub preferences: Preferences,
	/// The folders the writer has left open in the sidebar, by the root of the
	/// project they belong to. Kept here rather than in the project's manifest
	/// because it is how one writer is looking at the project on one machine,
	/// not part of the story: a copy of the project carries the words, and this
	/// stays behind. Absent from a store written before version 5.
	#[serde(default)]
	pub expanded: BTreeMap<PathBuf, Vec<Uuid>>,
}

impl Default for Store {
	fn default() -> Self {
		Self {
			version: STORE_VERSION,
			last: None,
			recent: Vec::new(),
			preferences: Preferences::default(),
			expanded: BTreeMap::new(),
		}
	}
}

impl Store {
	/// Moves a project to the front of the list, whether or not it was already
	/// there, and drops the oldest once the list is full.
	pub fn remember(&mut self, name: impl Into<String>, root: PathBuf, at: OffsetDateTime) {
		self.last = Some(root.clone());
		self.recent.retain(|project| project.root != root);
		self.recent.insert(
			0,
			RecentProject {
				name: name.into(),
				root,
				last_opened: at
					.replace_nanosecond(0)
					.expect("zero nanoseconds is always in range"),
			},
		);
		self.recent.truncate(MAX_RECENT);
	}

	/// What to reopen on launch, if anything.
	pub fn reopen(&self) -> Option<&RecentProject> {
		let root = self.last.as_ref()?;
		self.recent.iter().find(|project| &project.root == root)
	}

	/// The writer is done for now. The project stays in the list, so it is one
	/// click away, but Aurora starts on the welcome screen next time.
	pub fn close(&mut self) {
		self.last = None;
	}

	pub fn forget(&mut self, root: &Path) {
		self.recent.retain(|project| project.root != root);
		if self.last.as_deref() == Some(root) {
			self.last = None;
		}
		// A project Aurora has been told to forget leaves nothing behind, and
		// a folder it no longer knows about could not be opened again anyway.
		self.expanded.remove(root);
	}
}

/// Reads the store, treating both a missing and an unreadable file as empty.
/// The list is a convenience, so a damaged one must not stop Aurora starting.
pub fn load(path: &Path) -> Result<Store> {
	match fs::read(path) {
		Ok(bytes) => Ok(migrate(serde_json::from_slice(&bytes).unwrap_or_default())),
		Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Store::default()),
		Err(e) => Err(Error::Io(e)),
	}
}

/// Version 1 had no `last`: whatever was at the head of the list was what
/// reopened. Adopting it here means an upgrade does not lose the open project.
/// Version 2 had no preferences, version 3 no `sidebar` and version 4 no
/// `expanded`; serde's defaults are the whole migration for all three.
/// Nothing is written back — the next save carries the new shape.
fn migrate(mut store: Store) -> Store {
	if store.version < STORE_VERSION {
		// Version 1 only. Left ungated, every later version would adopt the
		// head of the list too, turning a project the writer deliberately
		// closed back into the one that reopens on launch.
		if store.version < 2 && store.last.is_none() {
			store.last = store.recent.first().map(|project| project.root.clone());
		}
		store.version = STORE_VERSION;
	}
	store
}

/// Writes through a temporary file, so an interrupted save cannot leave a
/// half-written store behind.
pub fn save(path: &Path, store: &Store) -> Result<()> {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent)?;
	}

	write_json(path, store)
}

/// A damaged store reads as an empty one, so this always answers.
fn preferences(path: &Path) -> Result<Preferences> {
	Ok(load(path)?.preferences)
}

fn set_preferences(path: &Path, preferences: Preferences) -> Result<()> {
	let mut store = load(path)?;
	store.preferences = preferences;
	save(path, &store)
}

#[tauri::command]
pub fn read_preferences(app: AppHandle) -> Result<Preferences> {
	preferences(&store_path(&app)?)
}

#[tauri::command]
pub fn write_preferences(app: AppHandle, preferences: Preferences) -> Result<()> {
	set_preferences(&store_path(&app)?, preferences)
}

fn expanded(path: &Path, root: &Path) -> Result<Vec<Uuid>> {
	Ok(load(path)?.expanded.remove(root).unwrap_or_default())
}

/// An empty list drops the project's entry rather than writing one: a writer
/// who folds everything shut is back where they started, and the store should
/// say so rather than keep a row saying nothing.
fn set_expanded(path: &Path, root: PathBuf, open: Vec<Uuid>) -> Result<()> {
	let mut store = load(path)?;

	if open.is_empty() {
		store.expanded.remove(&root);
	} else {
		store.expanded.insert(root, open);
	}

	save(path, &store)
}

/// The folders left open in one project, which is none at all for a project the
/// writer has not expanded anything in yet.
#[tauri::command]
pub fn read_expanded(app: AppHandle, root: PathBuf) -> Result<Vec<Uuid>> {
	expanded(&store_path(&app)?, &root)
}

#[tauri::command]
pub fn write_expanded(app: AppHandle, root: PathBuf, open: Vec<Uuid>) -> Result<()> {
	set_expanded(&store_path(&app)?, root, open)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn at(seconds: i64) -> OffsetDateTime {
		OffsetDateTime::from_unix_timestamp(seconds).unwrap()
	}

	fn store_path(dir: &tempfile::TempDir) -> PathBuf {
		dir.path().join("config").join("store.json")
	}

	#[test]
	fn a_missing_store_reads_as_empty() {
		let dir = tempfile::tempdir().unwrap();
		assert_eq!(load(&store_path(&dir)).unwrap(), Store::default());
	}

	#[test]
	fn a_damaged_store_reads_as_empty() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("store.json");
		fs::write(&path, "{ this is not json").unwrap();
		assert_eq!(load(&path).unwrap(), Store::default());
	}

	#[test]
	fn saving_creates_the_config_folder() {
		let dir = tempfile::tempdir().unwrap();
		let path = store_path(&dir);
		let mut store = Store::default();
		store.remember(
			"Ithaca",
			PathBuf::from("/writing/Ithaca"),
			at(1_700_000_000),
		);

		save(&path, &store).unwrap();
		assert_eq!(load(&path).unwrap(), store);
	}

	#[test]
	fn saving_leaves_no_temporary_file() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("store.json");
		save(&path, &Store::default()).unwrap();

		let left: Vec<_> = fs::read_dir(dir.path())
			.unwrap()
			.map(|entry| entry.unwrap().file_name())
			.collect();
		assert_eq!(left, ["store.json"]);
	}

	#[test]
	fn the_store_is_written_with_tabs() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("store.json");
		let mut store = Store::default();
		store.remember(
			"Ithaca",
			PathBuf::from("/writing/Ithaca"),
			at(1_700_000_000),
		);
		save(&path, &store).unwrap();

		let json = fs::read_to_string(&path).unwrap();
		assert!(
			json.contains("\n\t\"version\": 5"),
			"expected tab indentation"
		);
		assert!(json.contains("\"lastOpened\": \"2023-11-14T22:13:20Z\""));
		assert!(json.ends_with("\n"));
	}

	#[test]
	fn the_newest_project_comes_first() {
		let mut store = Store::default();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(1));
		store.remember("Penelope", PathBuf::from("/writing/Penelope"), at(2));

		assert_eq!(store.reopen().unwrap().name, "Penelope");
		assert_eq!(store.recent.len(), 2);
	}

	#[test]
	fn reopening_a_project_moves_it_without_duplicating() {
		let mut store = Store::default();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(1));
		store.remember("Penelope", PathBuf::from("/writing/Penelope"), at(2));
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(3));

		assert_eq!(store.recent.len(), 2);
		assert_eq!(store.reopen().unwrap().name, "Ithaca");
		assert_eq!(store.reopen().unwrap().last_opened, at(3));
	}

	#[test]
	fn the_list_stops_growing() {
		let mut store = Store::default();
		for n in 0..MAX_RECENT as i64 + 5 {
			store.remember(
				format!("Book {n}"),
				PathBuf::from(format!("/writing/{n}")),
				at(n),
			);
		}

		assert_eq!(store.recent.len(), MAX_RECENT);
		assert_eq!(store.reopen().unwrap().name, "Book 14");
	}

	#[test]
	fn a_forgotten_project_is_gone() {
		let mut store = Store::default();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(1));
		store.forget(Path::new("/writing/Ithaca"));
		assert!(store.reopen().is_none());
	}

	#[test]
	fn closing_leaves_the_project_in_the_list() {
		let mut store = Store::default();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(1));
		store.close();

		assert!(store.reopen().is_none(), "nothing reopens on launch");
		assert_eq!(store.recent.len(), 1, "but it is still one click away");
		assert_eq!(store.recent[0].name, "Ithaca");
	}

	#[test]
	fn opening_a_project_again_marks_it_for_reopening() {
		let mut store = Store::default();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(1));
		store.close();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(2));

		assert_eq!(store.reopen().unwrap().name, "Ithaca");
	}

	#[test]
	fn forgetting_the_marked_project_does_not_promote_the_next_one() {
		let mut store = Store::default();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(1));
		store.remember("Penelope", PathBuf::from("/writing/Penelope"), at(2));
		store.forget(Path::new("/writing/Penelope"));

		assert!(store.reopen().is_none());
		assert_eq!(store.recent.len(), 1);
	}

	#[test]
	fn forgetting_another_project_leaves_the_marker_alone() {
		let mut store = Store::default();
		store.remember("Penelope", PathBuf::from("/writing/Penelope"), at(1));
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(2));
		store.forget(Path::new("/writing/Penelope"));

		assert_eq!(store.reopen().unwrap().name, "Ithaca");
	}

	#[test]
	fn a_version_one_store_reopens_the_head_of_its_list() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("store.json");
		fs::write(
			&path,
			r#"{
				"version": 1,
				"recent": [
					{
						"name": "Ithaca",
						"root": "/writing/Ithaca",
						"lastOpened": "2023-11-14T22:13:20Z"
					}
				]
			}"#,
		)
		.unwrap();

		let store = load(&path).unwrap();
		assert_eq!(store.version, STORE_VERSION);
		assert_eq!(store.reopen().unwrap().name, "Ithaca");
	}

	#[test]
	fn a_version_two_store_gains_the_default_preferences() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("store.json");
		fs::write(
			&path,
			r#"{
				"version": 2,
				"last": "/writing/Ithaca",
				"recent": [
					{
						"name": "Ithaca",
						"root": "/writing/Ithaca",
						"lastOpened": "2023-11-14T22:13:20Z"
					}
				]
			}"#,
		)
		.unwrap();

		let store = load(&path).unwrap();
		assert_eq!(store.version, STORE_VERSION);
		assert_eq!(store.preferences, Preferences::default());
		assert_eq!(store.reopen().unwrap().name, "Ithaca");
	}

	// The version 1 rule adopts the head of the list as the project to reopen.
	// Running it on a version 2 store would undo a close the writer meant.
	#[test]
	fn a_version_two_store_that_was_closed_stays_closed() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("store.json");
		fs::write(
			&path,
			r#"{
				"version": 2,
				"recent": [
					{
						"name": "Ithaca",
						"root": "/writing/Ithaca",
						"lastOpened": "2023-11-14T22:13:20Z"
					}
				]
			}"#,
		)
		.unwrap();

		let store = load(&path).unwrap();
		assert!(store.reopen().is_none());
		assert_eq!(store.recent.len(), 1);
	}

	// A field added to Preferences without a default would fail the whole
	// Store's deserialization, and the recents list would be lost with it.
	#[test]
	fn preferences_written_before_a_field_existed_still_read() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("store.json");
		fs::write(
			&path,
			r#"{
				"version": 3,
				"recent": [],
				"preferences": {
					"toolbar": false,
					"focus": true,
					"measure": 80,
					"fontSize": 19,
					"lineHeight": 2.0
				}
			}"#,
		)
		.unwrap();

		let store = load(&path).unwrap();
		assert!(!store.preferences.toolbar);
		assert!(!store.preferences.typewriter);
		assert!(!store.preferences.outline);
		// The one field whose default is not its type's: a store that predates
		// it has to come back with the sidebar showing, not hidden.
		assert!(store.preferences.sidebar);
		assert_eq!(store.preferences.measure, 80);
	}

	#[test]
	fn preferences_survive_a_save() {
		let dir = tempfile::tempdir().unwrap();
		let path = store_path(&dir);
		let wanted = Preferences {
			toolbar: false,
			focus: true,
			typewriter: true,
			outline: true,
			sidebar: false,
			measure: 80,
			font_size: 19,
			line_height: 2.0,
		};

		set_preferences(&path, wanted.clone()).unwrap();
		assert_eq!(preferences(&path).unwrap(), wanted);
	}

	#[test]
	fn changing_preferences_leaves_the_project_list_alone() {
		let dir = tempfile::tempdir().unwrap();
		let path = store_path(&dir);
		let mut store = Store::default();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(1));
		save(&path, &store).unwrap();

		set_preferences(
			&path,
			Preferences {
				toolbar: false,
				..Preferences::default()
			},
		)
		.unwrap();

		let read_back = load(&path).unwrap();
		assert_eq!(read_back.reopen().unwrap().name, "Ithaca");
		assert!(!read_back.preferences.toolbar);
	}

	#[test]
	fn a_damaged_store_still_answers_for_preferences() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("store.json");
		fs::write(&path, "{ this is not json").unwrap();
		assert_eq!(preferences(&path).unwrap(), Preferences::default());
	}

	#[test]
	fn a_closed_store_stays_closed_across_a_save() {
		let dir = tempfile::tempdir().unwrap();
		let path = store_path(&dir);
		let mut store = Store::default();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(1));
		store.close();
		save(&path, &store).unwrap();

		let read_back = load(&path).unwrap();
		assert!(read_back.reopen().is_none());
		assert_eq!(read_back.recent.len(), 1);
	}

	#[test]
	fn the_folders_left_open_come_back_project_by_project() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("store.json");
		let ithaca = PathBuf::from("/writing/Ithaca");
		let rooks = PathBuf::from("/writing/Rooks");
		let part = Uuid::new_v4();

		set_expanded(&path, ithaca.clone(), vec![part]).unwrap();

		assert_eq!(expanded(&path, &ithaca).unwrap(), vec![part]);
		assert!(
			expanded(&path, &rooks).unwrap().is_empty(),
			"one project's open folders are not another's"
		);
	}

	#[test]
	fn folding_everything_shut_leaves_no_entry_behind() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("store.json");
		let ithaca = PathBuf::from("/writing/Ithaca");

		set_expanded(&path, ithaca.clone(), vec![Uuid::new_v4()]).unwrap();
		set_expanded(&path, ithaca, Vec::new()).unwrap();

		assert!(load(&path).unwrap().expanded.is_empty());
	}

	#[test]
	fn forgetting_a_project_forgets_which_folders_were_open() {
		let mut store = Store::default();
		let root = PathBuf::from("/writing/Ithaca");
		store.remember("Ithaca", root.clone(), at(1));
		store.expanded.insert(root.clone(), vec![Uuid::new_v4()]);

		store.forget(&root);

		assert!(store.expanded.is_empty());
	}

	#[test]
	fn sub_second_precision_is_dropped() {
		let mut store = Store::default();
		let precise = OffsetDateTime::from_unix_timestamp_nanos(1_700_000_000_297_374_168).unwrap();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), precise);
		assert_eq!(store.reopen().unwrap().last_opened, at(1_700_000_000));
	}
}
