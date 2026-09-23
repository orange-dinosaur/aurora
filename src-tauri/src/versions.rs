//! Every project keeps its versions as a git repository at its own root.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use git2::{Commit, ErrorCode, IndexAddOption, Oid, Repository, Signature, Tree};
use serde::Deserialize;

use crate::document::TRASH_DIR;
use crate::history::HISTORY_FILE;
use crate::project::{Error, Result, read_manifest};

const FIRST_VERSION: &str = "Versions begin";
const AUTHOR_EMAIL: &str = "aurora@localhost";

/// Marks a commit as one Aurora kept, so it is never named by its subject.
const KIND_TRAILER: &str = "Aurora-Kind";

/// A version the frontend asks to keep. Every count is in words.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum Keep {
	Named {
		name: String,
	},
	Session {
		minutes: u32,
		written: u32,
		removed: u32,
	},
	Closing {
		written: u32,
		removed: u32,
	},
	/// `label` is the name or time of the version being put back.
	BeforePuttingBack {
		label: String,
	},
}

impl Keep {
	fn kind(&self) -> &'static str {
		match self {
			Keep::Named { .. } => "named",
			Keep::Session { .. } => "session",
			Keep::Closing { .. } => "closing",
			Keep::BeforePuttingBack { .. } => "beforePuttingBack",
		}
	}

	fn name(&self) -> Option<String> {
		match self {
			Keep::Named { name } => Some(name.clone()),
			Keep::BeforePuttingBack { label } => Some(format!("Before putting back “{label}”")),
			Keep::Session { .. } | Keep::Closing { .. } => None,
		}
	}

	/// A subject someone reading `git log` can follow, then the trailers the
	/// Versions tab reads its labels from.
	fn message(&self) -> String {
		let mut trailers = vec![format!("{KIND_TRAILER}: {}", self.kind())];
		let subject = match self {
			Keep::Named { .. } | Keep::BeforePuttingBack { .. } => self.name().unwrap_or_default(),
			Keep::Session {
				minutes,
				written,
				removed,
			} => {
				trailers.push(format!("Aurora-Minutes: {minutes}"));
				trailers.push(format!("Aurora-Written: {written}"));
				trailers.push(format!("Aurora-Removed: {removed}"));
				format!(
					"A session of {minutes} minutes, {written} words written and {removed} removed"
				)
			}
			Keep::Closing { written, removed } => {
				trailers.push(format!("Aurora-Written: {written}"));
				trailers.push(format!("Aurora-Removed: {removed}"));
				format!("Left the project, {written} words written and {removed} removed")
			}
		};
		format!("{subject}\n\n{}\n", trailers.join("\n"))
	}
}

/// Set once the writer has been told about an enclosing repository. It lives
/// in the project's own git config, which is local and never pushed.
pub const NESTED_NOTICE_SHOWN: &str = "aurora.nestedNoticeShown";

/// Kept out of every version, so going back never rolls back Stats or the
/// Trash. Written to `.git/info/exclude` so the project folder gains no file.
fn excluded() -> [String; 2] {
	[format!("/{HISTORY_FILE}"), format!("/{TRASH_DIR}/")]
}

/// Opens the repository at `root`, making one when there is none. Returns the
/// folder of a repository the project sits inside, while the writer has not
/// yet been told about it.
pub fn ensure_repository(root: &Path, author: &str) -> Result<Option<PathBuf>> {
	// `open` looks only at `root`: a repository further up belongs to
	// something else, and the project still gets its own.
	let repo = match Repository::open(root) {
		Ok(repo) => repo,
		Err(e) if e.code() == ErrorCode::NotFound => Repository::init(root)?,
		Err(e) => return Err(e.into()),
	};
	exclude(&repo)?;
	if repo.is_empty()? {
		keep_first(&repo, author)?;
	}
	enclosing(root, &repo)
}

fn exclude(repo: &Repository) -> Result<()> {
	let path = repo.path().join("info").join("exclude");
	let mut text = match fs::read_to_string(&path) {
		Ok(text) => text,
		Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
		Err(e) => return Err(e.into()),
	};
	let missing: Vec<String> = excluded()
		.into_iter()
		.filter(|line| !text.lines().any(|existing| existing.trim() == line))
		.collect();
	if missing.is_empty() {
		return Ok(());
	}

	if !text.is_empty() && !text.ends_with('\n') {
		text.push('\n');
	}
	for line in missing {
		text.push_str(&line);
		text.push('\n');
	}
	fs::create_dir_all(repo.path().join("info"))?;
	fs::write(&path, text)?;
	Ok(())
}

fn keep_first(repo: &Repository, author: &str) -> Result<()> {
	let tree = repo.find_tree(stage(repo)?)?;
	let first = Keep::Named {
		name: FIRST_VERSION.to_owned(),
	};
	commit(repo, &signature(author)?, &first, &tree, &[])?;
	Ok(())
}

fn signature(author: &str) -> Result<Signature<'static>> {
	let name = if author.trim().is_empty() {
		"Aurora"
	} else {
		author
	};
	// libgit2 refuses an empty email, and a writer has none to give.
	Ok(Signature::now(name, AUTHOR_EMAIL)?)
}

/// Stages the whole project, deletions included, and returns its tree.
fn stage(repo: &Repository) -> Result<Oid> {
	let mut index = repo.index()?;
	index.add_all(["*"], IndexAddOption::DEFAULT, None)?;
	index.update_all(["*"], None)?;
	index.write()?;
	Ok(index.write_tree()?)
}

fn commit(
	repo: &Repository,
	signature: &Signature,
	keep: &Keep,
	tree: &Tree,
	parents: &[&Commit],
) -> Result<Oid> {
	let id = repo.commit(
		Some("HEAD"),
		signature,
		signature,
		&keep.message(),
		tree,
		parents,
	)?;
	if let Some(name) = keep.name() {
		repo.note(signature, signature, None, id, &name, true)?;
	}
	Ok(id)
}

/// Keeps the project as it is now and returns the version that holds it. With
/// nothing changed that is the latest version, and only a named version does
/// anything: it names the latest one, or keeps an empty version under the new
/// name when the latest already has one, so no name is lost.
pub fn keep(root: &Path, keep: &Keep, author: &str) -> Result<Oid> {
	let repo = Repository::open(root)?;
	let head = repo.head()?.peel_to_commit()?;
	let tree = repo.find_tree(stage(&repo)?)?;
	let signature = signature(author)?;

	if tree.id() == head.tree_id() {
		match keep {
			Keep::Named { name } if name_of(&repo, &head)?.is_none() => {
				repo.note(&signature, &signature, None, head.id(), name, true)?;
				return Ok(head.id());
			}
			Keep::Named { .. } => {}
			_ => return Ok(head.id()),
		}
	}
	commit(&repo, &signature, keep, &tree, &[&head])
}

/// A version's name. A note always wins, and an empty one means no name. A
/// commit Aurora did not make is otherwise named by its subject line.
pub fn name_of(repo: &Repository, commit: &Commit) -> Result<Option<String>> {
	match repo.find_note(None, commit.id()) {
		Ok(note) => {
			let name = note.message()?.trim();
			return Ok((!name.is_empty()).then(|| name.to_owned()));
		}
		Err(e) if e.code() == ErrorCode::NotFound => {}
		Err(e) => return Err(e.into()),
	}
	let ours = git2::message_trailers_strs(commit.message()?)?
		.iter()
		.any(|(key, _)| key == KIND_TRAILER);
	if ours {
		return Ok(None);
	}
	Ok(commit.summary()?.map(str::to_owned))
}

/// Writes a name onto a version, replacing any it had. An empty name is
/// stored as an empty note, which is what takes a subject-line name away.
fn set_name(root: &Path, id: &str, name: &str, author: &str) -> Result<()> {
	let signature = signature(author)?;
	let repo = Repository::open(root)?;
	let commit = repo.find_commit(Oid::from_str(id)?)?;
	repo.note(&signature, &signature, None, commit.id(), name.trim(), true)?;
	Ok(())
}

/// The book's author, which every version is kept under.
fn author(root: &Path) -> Result<String> {
	// The path arrives from the frontend, so it is not trusted to be sensible.
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Ok(read_manifest(root)?.book.author)
}

#[tauri::command]
pub fn keep_version(root: PathBuf, keep: Keep) -> Result<String> {
	let author = author(&root)?;
	Ok(self::keep(&root, &keep, &author)?.to_string())
}

#[tauri::command]
pub fn rename_version(root: PathBuf, id: String, name: String) -> Result<()> {
	set_name(&root, &id, &name, &author(&root)?)
}

#[tauri::command]
pub fn unname_version(root: PathBuf, id: String) -> Result<()> {
	set_name(&root, &id, "", &author(&root)?)
}

fn enclosing(root: &Path, repo: &Repository) -> Result<Option<PathBuf>> {
	if repo
		.config()?
		.get_bool(NESTED_NOTICE_SHOWN)
		.unwrap_or(false)
	{
		return Ok(None);
	}
	let Some(parent) = root.parent() else {
		return Ok(None);
	};
	// Only ever a notice, so a repository that cannot be read counts as none.
	Ok(Repository::discover(parent)
		.ok()
		.and_then(|outer| outer.workdir().map(Path::to_path_buf)))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn project() -> tempfile::TempDir {
		let dir = tempfile::tempdir().unwrap();
		fs::create_dir(dir.path().join("Manuscript")).unwrap();
		fs::write(dir.path().join("Manuscript/one.md"), "Call me Ishmael.").unwrap();
		fs::write(dir.path().join("aurora.json"), "{}").unwrap();
		dir
	}

	fn versions(repo: &Repository) -> usize {
		let mut walk = repo.revwalk().unwrap();
		walk.push_head().unwrap();
		walk.count()
	}

	#[test]
	fn a_fresh_folder_begins_with_one_version() {
		let dir = project();
		assert_eq!(ensure_repository(dir.path(), "").unwrap(), None);

		let repo = Repository::open(dir.path()).unwrap();
		assert_eq!(versions(&repo), 1);
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		assert_eq!(head.summary().unwrap(), Some(FIRST_VERSION));
		assert_eq!(head.author().name().unwrap(), "Aurora");
		assert_eq!(head.author().email().unwrap(), AUTHOR_EMAIL);
		let tree = head.tree().unwrap();
		assert!(tree.get_path(Path::new("Manuscript/one.md")).is_ok());
		assert!(tree.get_path(Path::new("aurora.json")).is_ok());
		assert_eq!(
			repo.find_note(None, head.id()).unwrap().message().unwrap(),
			FIRST_VERSION
		);
	}

	#[test]
	fn the_first_version_is_by_the_books_author() {
		let dir = project();
		ensure_repository(dir.path(), "Herman Melville").unwrap();

		let repo = Repository::open(dir.path()).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		assert_eq!(head.author().name().unwrap(), "Herman Melville");
	}

	#[test]
	fn opening_again_keeps_no_new_version() {
		let dir = project();
		ensure_repository(dir.path(), "").unwrap();
		ensure_repository(dir.path(), "").unwrap();

		let repo = Repository::open(dir.path()).unwrap();
		assert_eq!(versions(&repo), 1);
		let exclude = fs::read_to_string(repo.path().join("info/exclude")).unwrap();
		assert_eq!(exclude.matches("/history.json").count(), 1);
	}

	#[test]
	fn an_existing_repository_is_adopted_untouched() {
		let dir = project();
		let repo = Repository::init(dir.path()).unwrap();
		let mut index = repo.index().unwrap();
		index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
		let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
		let signature = Signature::now("Writer", "writer@example.com").unwrap();
		let theirs = repo
			.commit(Some("HEAD"), &signature, &signature, "Mine", &tree, &[])
			.unwrap();

		ensure_repository(dir.path(), "").unwrap();

		assert_eq!(repo.head().unwrap().target(), Some(theirs));
		assert_eq!(versions(&repo), 1);
	}

	#[test]
	fn excluded_files_are_never_kept() {
		let dir = project();
		fs::write(dir.path().join(HISTORY_FILE), "{}").unwrap();
		fs::create_dir(dir.path().join(TRASH_DIR)).unwrap();
		fs::write(dir.path().join(TRASH_DIR).join("gone.md"), "gone").unwrap();

		ensure_repository(dir.path(), "").unwrap();

		let repo = Repository::open(dir.path()).unwrap();
		let tree = repo.head().unwrap().peel_to_tree().unwrap();
		assert!(tree.get_path(Path::new(HISTORY_FILE)).is_err());
		assert!(tree.get_path(Path::new(TRASH_DIR)).is_err());
		assert!(tree.get_path(Path::new("aurora.json")).is_ok());
	}

	#[test]
	fn a_project_inside_another_repository_says_so_until_told() {
		let outer = tempfile::tempdir().unwrap();
		Repository::init(outer.path()).unwrap();
		let root = outer.path().join("Ithaca");
		fs::create_dir(&root).unwrap();
		fs::write(root.join("aurora.json"), "{}").unwrap();

		let found = ensure_repository(&root, "").unwrap().unwrap();
		// Compared by name: the platforms disagree about how to spell the path.
		assert_eq!(found.file_name(), outer.path().file_name());
		assert!(root.join(".git").is_dir());

		Repository::open(&root)
			.unwrap()
			.config()
			.unwrap()
			.set_bool(NESTED_NOTICE_SHOWN, true)
			.unwrap();
		assert_eq!(ensure_repository(&root, "").unwrap(), None);
	}

	/// A project with its first version, and a change waiting to be kept.
	fn changed() -> (tempfile::TempDir, Repository) {
		let dir = project();
		ensure_repository(dir.path(), "").unwrap();
		fs::write(
			dir.path().join("Manuscript/one.md"),
			"Call me Ishmael. Some",
		)
		.unwrap();
		let repo = Repository::open(dir.path()).unwrap();
		(dir, repo)
	}

	fn trailers(commit: &Commit) -> Vec<(String, String)> {
		git2::message_trailers_strs(commit.message().unwrap())
			.unwrap()
			.iter()
			.map(|(key, value)| (key.to_owned(), value.to_owned()))
			.collect()
	}

	fn pair(key: &str, value: &str) -> (String, String) {
		(key.to_owned(), value.to_owned())
	}

	#[test]
	fn a_session_keeps_its_counts_and_no_name() {
		let (dir, repo) = changed();
		let session = Keep::Session {
			minutes: 25,
			written: 312,
			removed: 40,
		};
		let id = keep(dir.path(), &session, "").unwrap();

		let commit = repo.find_commit(id).unwrap();
		assert_eq!(commit.parent_count(), 1);
		assert_eq!(
			trailers(&commit),
			[
				pair("Aurora-Kind", "session"),
				pair("Aurora-Minutes", "25"),
				pair("Aurora-Written", "312"),
				pair("Aurora-Removed", "40"),
			]
		);
		assert_eq!(name_of(&repo, &commit).unwrap(), None);
	}

	#[test]
	fn closing_keeps_its_counts_without_minutes() {
		let (dir, repo) = changed();
		let closing = Keep::Closing {
			written: 5,
			removed: 0,
		};
		let id = keep(dir.path(), &closing, "").unwrap();

		assert_eq!(
			trailers(&repo.find_commit(id).unwrap()),
			[
				pair("Aurora-Kind", "closing"),
				pair("Aurora-Written", "5"),
				pair("Aurora-Removed", "0"),
			]
		);
	}

	#[test]
	fn a_named_version_is_named() {
		let (dir, repo) = changed();
		let named = Keep::Named {
			name: "Before the storm".to_owned(),
		};
		let id = keep(dir.path(), &named, "").unwrap();

		let commit = repo.find_commit(id).unwrap();
		assert_eq!(trailers(&commit), [pair("Aurora-Kind", "named")]);
		assert_eq!(
			name_of(&repo, &commit).unwrap().as_deref(),
			Some("Before the storm")
		);
	}

	#[test]
	fn putting_back_names_what_it_saved_the_writer_from() {
		let (dir, repo) = changed();
		let before = Keep::BeforePuttingBack {
			label: "Before the storm".to_owned(),
		};
		let id = keep(dir.path(), &before, "").unwrap();

		let commit = repo.find_commit(id).unwrap();
		assert_eq!(
			trailers(&commit),
			[pair("Aurora-Kind", "beforePuttingBack")]
		);
		assert_eq!(
			name_of(&repo, &commit).unwrap().as_deref(),
			Some("Before putting back “Before the storm”")
		);
	}

	#[test]
	fn a_deleted_document_leaves_the_version() {
		let (dir, repo) = changed();
		fs::remove_file(dir.path().join("Manuscript/one.md")).unwrap();
		let closing = Keep::Closing {
			written: 0,
			removed: 3,
		};
		let id = keep(dir.path(), &closing, "").unwrap();

		let tree = repo.find_commit(id).unwrap().tree().unwrap();
		assert!(tree.get_path(Path::new("Manuscript/one.md")).is_err());
	}

	#[test]
	fn nothing_changed_keeps_nothing() {
		let dir = project();
		ensure_repository(dir.path(), "").unwrap();
		let repo = Repository::open(dir.path()).unwrap();
		let head = repo.head().unwrap().target().unwrap();

		let session = Keep::Session {
			minutes: 1,
			written: 0,
			removed: 0,
		};
		assert_eq!(keep(dir.path(), &session, "").unwrap(), head);
		assert_eq!(versions(&repo), 1);
	}

	#[test]
	fn naming_with_nothing_changed_names_the_latest() {
		let (dir, repo) = changed();
		let session = Keep::Session {
			minutes: 1,
			written: 1,
			removed: 0,
		};
		let latest = keep(dir.path(), &session, "").unwrap();

		let named = Keep::Named {
			name: "Draft one".to_owned(),
		};
		assert_eq!(keep(dir.path(), &named, "").unwrap(), latest);
		assert_eq!(versions(&repo), 2);
		let commit = repo.find_commit(latest).unwrap();
		assert_eq!(
			name_of(&repo, &commit).unwrap().as_deref(),
			Some("Draft one")
		);
	}

	#[test]
	fn naming_an_already_named_latest_keeps_an_empty_version() {
		let dir = project();
		ensure_repository(dir.path(), "").unwrap();
		let repo = Repository::open(dir.path()).unwrap();
		let first = repo.head().unwrap().peel_to_commit().unwrap();

		let named = Keep::Named {
			name: "Draft one".to_owned(),
		};
		let id = keep(dir.path(), &named, "").unwrap();

		let commit = repo.find_commit(id).unwrap();
		assert_ne!(id, first.id());
		assert_eq!(commit.tree_id(), first.tree_id());
		assert_eq!(
			name_of(&repo, &commit).unwrap().as_deref(),
			Some("Draft one")
		);
		assert_eq!(
			name_of(&repo, &first).unwrap().as_deref(),
			Some(FIRST_VERSION)
		);
	}

	#[test]
	fn a_version_can_be_renamed_and_unnamed() {
		let dir = project();
		ensure_repository(dir.path(), "").unwrap();
		let repo = Repository::open(dir.path()).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		let id = head.id().to_string();

		set_name(dir.path(), &id, "The very start", "").unwrap();
		assert_eq!(
			name_of(&repo, &head).unwrap().as_deref(),
			Some("The very start")
		);
		set_name(dir.path(), &id, "", "").unwrap();
		assert_eq!(name_of(&repo, &head).unwrap(), None);
	}

	#[test]
	fn a_foreign_commit_is_named_by_its_subject_until_unnamed() {
		let dir = project();
		let repo = Repository::init(dir.path()).unwrap();
		let mut index = repo.index().unwrap();
		index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
		let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
		let signature = Signature::now("Writer", "writer@example.com").unwrap();
		let id = repo
			.commit(
				Some("HEAD"),
				&signature,
				&signature,
				"Chapter one done\n\nLong notes.",
				&tree,
				&[],
			)
			.unwrap();
		let commit = repo.find_commit(id).unwrap();

		assert_eq!(
			name_of(&repo, &commit).unwrap().as_deref(),
			Some("Chapter one done")
		);
		set_name(dir.path(), &id.to_string(), "", "").unwrap();
		assert_eq!(name_of(&repo, &commit).unwrap(), None);
	}
}
