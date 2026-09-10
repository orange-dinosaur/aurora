//! A project's writing history: one record per session, kept in the project.
//!
//! History is a fact about a novel rather than about this machine, so it
//! travels with the folder instead of sitting beside the preferences. It is a
//! file of its own rather than a key in the manifest, which is read every time
//! a project is listed and has no business carrying a decade of sessions.
//! Nothing is ever rolled up or thrown away: a record is about a hundred bytes,
//! so a decade of them is smaller than a chapter.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::project::{Error, MANIFEST_FILE, Result, write_json};

/// Bumped when the on-disk shape changes in a way older builds cannot read.
pub const HISTORY_VERSION: u32 = 1;

/// The history file at the root of a project, beside the manifest.
pub const HISTORY_FILE: &str = "history.json";

/// Which of the two layers a session came from. Automatic sessions are detected
/// underneath as the writer types; a deliberate one is opened by hand and sits
/// over them. Every number Aurora shows has to say which layer it came from, so
/// the layer is recorded on the session rather than guessed at afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "kebab-case")]
pub enum Layer {
	Automatic,
	Deliberate,
}

/// What a sprint was aiming at. A sprint is a deliberate session carrying one
/// of these; without it, a deliberate session runs until the writer ends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "unit", content = "amount", rename_all = "camelCase")]
pub enum Limit {
	Minutes(u32),
	Words(u32),
}

/// What one document gained and lost during a session, summed from the changes
/// the editor sees. Neither number can know about a document deleted whole.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct Tally {
	pub written: u32,
	pub removed: u32,
}

/// One session, as it is kept forever.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Session {
	/// The record needs a key of its own: a deliberate session can start on the
	/// same keystroke as the automatic one beneath it, so the times do not tell
	/// two records apart. Minted here when a session arrives without one, since
	/// inventing a UUID is not the frontend's job.
	#[serde(default = "Uuid::new_v4")]
	pub id: Uuid,
	pub layer: Layer,
	#[serde(with = "time::serde::rfc3339")]
	#[ts(type = "string")]
	pub start: OffsetDateTime,
	#[serde(with = "time::serde::rfc3339")]
	#[ts(type = "string")]
	pub end: OffsetDateTime,
	/// Absent unless the session was a sprint.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub limit: Option<Limit>,
	/// Whether a sprint reached what it was aiming at. A session with no limit
	/// has nothing to reach, so it writes no key.
	#[serde(default, skip_serializing_if = "not")]
	pub limit_met: bool,
	/// Words added and removed across the project, accumulated from the
	/// editor's per-change deltas.
	pub written: u32,
	pub removed: u32,
	/// The project's word count at the end less its count at the start. This can
	/// legitimately disagree with written less removed, because a deleted
	/// document shows up here and in neither of the other two.
	pub net: i32,
	/// What each document gained and lost, by document id. A document nobody
	/// touched is not listed.
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub documents: BTreeMap<Uuid, Tally>,
}

fn not(flag: &bool) -> bool {
	!flag
}

/// The contents of `history.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
	pub version: u32,
	#[serde(default)]
	pub sessions: Vec<Session>,
}

impl Default for History {
	fn default() -> Self {
		Self {
			version: HISTORY_VERSION,
			sessions: Vec::new(),
		}
	}
}

/// Reads a project's history, oldest session first.
///
/// A project that predates any counting has no file yet, and that is an empty
/// history rather than an error. A file that is there but unreadable is an
/// error: unlike the recents list, this is the only copy of what the writer has
/// done, so a damaged one has to be reported rather than quietly replaced by an
/// empty one at the next save.
pub fn read(root: &Path) -> Result<History> {
	let bytes = match fs::read(root.join(HISTORY_FILE)) {
		Ok(bytes) => bytes,
		Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(History::default()),
		Err(e) => return Err(Error::Io(e)),
	};

	let history: History = serde_json::from_slice(&bytes)?;
	// An older Aurora cannot know what a newer one added, so refuse rather than
	// dropping fields and writing them away on the next append.
	if history.version > HISTORY_VERSION {
		return Err(Error::UnsupportedVersion {
			found: history.version,
			supported: HISTORY_VERSION,
		});
	}
	Ok(history)
}

/// Adds one session to the end of the file, at the current version whatever
/// version it was read as.
pub fn append(root: &Path, session: Session) -> Result<()> {
	let mut history = read(root)?;
	history.version = HISTORY_VERSION;
	history.sessions.push(session);
	write_json(&root.join(HISTORY_FILE), &history)
}

/// One stat, so a folder that is not a project is refused rather than gaining a
/// history file of its own. The manifest itself is not read: keeping the two
/// files apart is the point of having two.
fn in_project(root: &Path) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	if !root.join(MANIFEST_FILE).is_file() {
		return Err(Error::NotAProject);
	}
	Ok(())
}

/// Every session this project has recorded, oldest first.
#[tauri::command]
pub fn read_history(root: PathBuf) -> Result<Vec<Session>> {
	in_project(&root)?;
	Ok(read(&root)?.sessions)
}

/// Records a session that has just closed.
#[tauri::command]
pub fn append_session(root: PathBuf, session: Session) -> Result<()> {
	in_project(&root)?;
	append(&root, session)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn at(offset: i64) -> OffsetDateTime {
		OffsetDateTime::from_unix_timestamp(1_700_000_000 + offset).unwrap()
	}

	/// An automatic session with nothing said about which documents it touched.
	fn session(start: i64) -> Session {
		Session {
			id: Uuid::new_v4(),
			layer: Layer::Automatic,
			start: at(start),
			end: at(start + 600),
			limit: None,
			limit_met: false,
			written: 420,
			removed: 30,
			net: 390,
			documents: BTreeMap::new(),
		}
	}

	#[test]
	fn a_project_with_no_history_file_has_no_sessions() {
		let root = tempfile::tempdir().unwrap();

		let history = read(root.path()).unwrap();
		assert_eq!(history.version, HISTORY_VERSION);
		assert!(history.sessions.is_empty());
	}

	#[test]
	fn appending_to_a_project_with_no_history_file_starts_one() {
		let root = tempfile::tempdir().unwrap();
		let first = session(0);

		append(root.path(), first.clone()).unwrap();

		assert!(root.path().join(HISTORY_FILE).is_file());
		assert_eq!(read(root.path()).unwrap().sessions, vec![first]);
	}

	#[test]
	fn an_empty_history_file_reads_as_no_sessions() {
		let root = tempfile::tempdir().unwrap();
		fs::write(
			root.path().join(HISTORY_FILE),
			br#"{ "version": 1, "sessions": [] }"#,
		)
		.unwrap();

		assert!(read(root.path()).unwrap().sessions.is_empty());
	}

	#[test]
	fn a_session_keeps_its_per_document_tally_across_the_round_trip() {
		let root = tempfile::tempdir().unwrap();
		let scene = Uuid::new_v4();
		let notes = Uuid::new_v4();
		let mut sprint = session(0);
		sprint.layer = Layer::Deliberate;
		sprint.limit = Some(Limit::Words(500));
		sprint.limit_met = true;
		sprint.documents.insert(
			scene,
			Tally {
				written: 380,
				removed: 20,
			},
		);
		sprint.documents.insert(
			notes,
			Tally {
				written: 40,
				removed: 10,
			},
		);

		append(root.path(), sprint.clone()).unwrap();

		let read_back = read(root.path()).unwrap().sessions;
		assert_eq!(read_back, vec![sprint]);
		assert_eq!(read_back[0].documents[&scene].written, 380);
		assert_eq!(read_back[0].documents[&notes].removed, 10);
	}

	#[test]
	fn appending_keeps_the_sessions_already_there_in_order() {
		let root = tempfile::tempdir().unwrap();
		let first = session(0);
		let second = session(3_600);

		append(root.path(), first.clone()).unwrap();
		append(root.path(), second.clone()).unwrap();

		assert_eq!(read(root.path()).unwrap().sessions, vec![first, second]);
	}

	#[test]
	fn a_session_with_no_limit_writes_neither_limit_key() {
		let json = serde_json::to_value(session(0)).unwrap();

		assert_eq!(json["written"], 420);
		assert!(json.get("limit").is_none());
		assert!(json.get("limitMet").is_none());
		assert!(json.get("documents").is_none());
	}

	#[test]
	fn a_session_that_arrives_without_an_id_is_given_one() {
		let json = r#"{
			"layer": "deliberate",
			"start": "2026-09-04T12:00:00Z",
			"end": "2026-09-04T12:25:00Z",
			"limitMet": false,
			"written": 300,
			"removed": 12,
			"net": 288,
			"documents": { "8f4a1c2e-0000-4000-8000-000000000001": {
				"written": 300, "removed": 12
			} }
		}"#;

		let one: Session = serde_json::from_str(json).unwrap();
		let two: Session = serde_json::from_str(json).unwrap();

		assert_ne!(one.id, two.id);
		assert_eq!(one.layer, Layer::Deliberate);
		assert_eq!(one.net, 288);
		assert_eq!(one.documents.len(), 1);
	}

	#[test]
	fn a_history_from_a_newer_aurora_is_refused() {
		let root = tempfile::tempdir().unwrap();
		fs::write(
			root.path().join(HISTORY_FILE),
			br#"{ "version": 99, "sessions": [] }"#,
		)
		.unwrap();

		assert!(matches!(
			read(root.path()),
			Err(Error::UnsupportedVersion {
				found: 99,
				supported: HISTORY_VERSION
			})
		));
	}

	#[test]
	fn a_folder_that_is_not_a_project_records_nothing() {
		let root = tempfile::tempdir().unwrap();

		let refused = append_session(root.path().to_path_buf(), session(0));

		assert!(matches!(refused, Err(Error::NotAProject)));
		assert!(!root.path().join(HISTORY_FILE).exists());
	}

	#[test]
	fn a_relative_root_is_refused() {
		assert!(matches!(
			read_history(PathBuf::from("Ithaca")),
			Err(Error::RelativePath)
		));
	}
}
