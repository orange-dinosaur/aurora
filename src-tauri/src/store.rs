use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::project::{Error, Result, write_json};

pub const STORE_VERSION: u32 = 1;

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
	pub recent: Vec<RecentProject>,
}

impl Default for Store {
	fn default() -> Self {
		Self {
			version: STORE_VERSION,
			recent: Vec::new(),
		}
	}
}

impl Store {
	/// Moves a project to the front of the list, whether or not it was already
	/// there, and drops the oldest once the list is full.
	pub fn remember(&mut self, name: impl Into<String>, root: PathBuf, at: OffsetDateTime) {
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

	pub fn last(&self) -> Option<&RecentProject> {
		self.recent.first()
	}

	pub fn forget(&mut self, root: &Path) {
		self.recent.retain(|project| project.root != root);
	}
}

/// Reads the store, treating both a missing and an unreadable file as empty.
/// The list is a convenience, so a damaged one must not stop Aurora starting.
pub fn load(path: &Path) -> Result<Store> {
	match fs::read(path) {
		Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
		Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Store::default()),
		Err(e) => Err(Error::Io(e)),
	}
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
			json.contains("\n\t\"version\": 1"),
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

		assert_eq!(store.last().unwrap().name, "Penelope");
		assert_eq!(store.recent.len(), 2);
	}

	#[test]
	fn reopening_a_project_moves_it_without_duplicating() {
		let mut store = Store::default();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(1));
		store.remember("Penelope", PathBuf::from("/writing/Penelope"), at(2));
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(3));

		assert_eq!(store.recent.len(), 2);
		assert_eq!(store.last().unwrap().name, "Ithaca");
		assert_eq!(store.last().unwrap().last_opened, at(3));
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
		assert_eq!(store.last().unwrap().name, "Book 14");
	}

	#[test]
	fn a_forgotten_project_is_gone() {
		let mut store = Store::default();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), at(1));
		store.forget(Path::new("/writing/Ithaca"));
		assert!(store.last().is_none());
	}

	#[test]
	fn sub_second_precision_is_dropped() {
		let mut store = Store::default();
		let precise = OffsetDateTime::from_unix_timestamp_nanos(1_700_000_000_297_374_168).unwrap();
		store.remember("Ithaca", PathBuf::from("/writing/Ithaca"), precise);
		assert_eq!(store.last().unwrap().last_opened, at(1_700_000_000));
	}
}
