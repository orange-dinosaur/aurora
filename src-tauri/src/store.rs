use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::project::{Error, Result, write_json};

pub const STORE_VERSION: u32 = 2;

/// How many projects are worth offering on the welcome screen.
const MAX_RECENT: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
	pub name: String,
	pub root: PathBuf,
	#[serde(with = "time::serde::rfc3339")]
	pub last_opened: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

impl Default for Store {
	fn default() -> Self {
		Self {
			version: STORE_VERSION,
			last: None,
			recent: Vec::new(),
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
/// Nothing is written back — the next save carries the new shape.
fn migrate(mut store: Store) -> Store {
	if store.version < STORE_VERSION {
		if store.last.is_none() {
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
			json.contains("\n\t\"version\": 2"),
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
	fn sub_second_precision_is_dropped() {
		let mut store = Store::default();
		let precise = OffsetDateTime::from_unix_timestamp_nanos(1_700_000_000_297_374_168).unwrap();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), precise);
		assert_eq!(store.reopen().unwrap().last_opened, at(1_700_000_000));
	}
}
