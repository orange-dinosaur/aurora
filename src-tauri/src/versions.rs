//! Every project keeps its versions as a git repository at its own root.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use git2::{ErrorCode, IndexAddOption, Repository, Signature};

use crate::document::TRASH_DIR;
use crate::history::HISTORY_FILE;
use crate::project::Result;

const FIRST_VERSION: &str = "Versions begin";
const AUTHOR_EMAIL: &str = "aurora@localhost";

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
	let mut index = repo.index()?;
	index.add_all(["*"], IndexAddOption::DEFAULT, None)?;
	index.write()?;
	let tree = repo.find_tree(index.write_tree()?)?;

	let name = if author.trim().is_empty() {
		"Aurora"
	} else {
		author
	};
	// libgit2 refuses an empty email, and a writer has none to give.
	let signature = Signature::now(name, AUTHOR_EMAIL)?;
	let commit = repo.commit(
		Some("HEAD"),
		&signature,
		&signature,
		FIRST_VERSION,
		&tree,
		&[],
	)?;
	repo.note(&signature, &signature, None, commit, FIRST_VERSION, false)?;
	Ok(())
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
		assert_eq!(head.message().unwrap(), FIRST_VERSION);
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
}
