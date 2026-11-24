//! Every project keeps its versions as a git repository at its own root.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use git2::build::CheckoutBuilder;
use git2::{
	BranchType, Commit, ErrorCode, Index, IndexAddOption, IndexEntry, IndexTime, MergeFileInput,
	MergeFileOptions, Oid, Repository, RepositoryInitOptions, ResetType, Signature, Sort,
	StatusOptions, Tree,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::document::{
	TRASH_DIR, body, delete_document, document_path, move_node, recreate_document, rename_document,
	write_document,
};
use crate::history::HISTORY_FILE;
use crate::merge;
use crate::project::{
	Book, COVER_NAMES, Error, Format, MANIFEST_FILE, Manifest, Result, read_manifest,
	remove_if_there, to_json, write_atomic, write_manifest,
};
use crate::takes::{TAKES_DIR, in_order, restore};
use crate::tree::{self, Node, Take};

mod branches;
mod changes;
mod history;
mod merging;
mod put_back;
mod repo;
mod staging;

pub use branches::*;
pub use changes::*;
pub use history::*;
pub use merging::*;
pub use put_back::*;
pub use repo::*;
pub use staging::*;

const AUTHOR_EMAIL: &str = "aurora@localhost";

/// The book's author, which every version is kept under.
fn author(root: &Path) -> Result<String> {
	// The path arrives from the frontend, so it is not trusted to be sensible.
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Ok(read_manifest(root)?.book.author)
}

#[tauri::command]
pub fn add_checkpoint(root: PathBuf, message: String) -> Result<String> {
	let author = author(&root)?;
	Ok(checkpoint(&root, &message, &author)?.to_string())
}

#[tauri::command]
pub fn list_checkpoints(
	root: PathBuf,
	version: Option<String>,
	document: Option<Uuid>,
) -> Result<Vec<Checkpoint>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	checkpoints(&root, version.as_deref(), document)
}

/// Main first, then the others by name.
#[tauri::command]
pub fn list_branches(root: PathBuf) -> Result<Vec<Branch>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	branches(&root)
}

#[tauri::command]
pub fn create_branch(root: PathBuf, name: String) -> Result<String> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	new_branch(&root, &name)
}

#[tauri::command]
pub fn rename_branch(root: PathBuf, slug: String, name: String) -> Result<String> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	renamed_branch(&root, &slug, &name)
}

#[tauri::command]
pub fn delete_branch(root: PathBuf, slug: String) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	dropped_branch(&root, &slug)
}

/// Asked before switching, so the writer can decide what happens to changes
/// not yet in a checkpoint.
#[tauri::command]
pub fn changes_pending(root: PathBuf) -> Result<bool> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	pending(&root)
}

#[tauri::command]
pub fn switch_branch(root: PathBuf, slug: String) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	switch(&root, &slug)
}

/// Merges `slug` into the version the writer is in. None when it finished
/// with its own checkpoint, otherwise the conflicts left to settle.
#[tauri::command]
pub fn merge_branch(root: PathBuf, slug: String) -> Result<Option<Merging>> {
	let author = author(&root)?;
	merge(&root, &slug, &author)
}

/// The merge under way, if there is one.
#[tauri::command]
pub fn merge_state(root: PathBuf) -> Result<Option<Merging>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	merging(&root)
}

#[tauri::command]
pub fn abort_merge(root: PathBuf) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	abort(&root)
}

/// A conflicted document as clean text and spots, or none once it is settled.
#[tauri::command]
pub fn merge_spots(root: PathBuf, id: Uuid) -> Result<Option<Vec<Piece>>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	spots(&root, id)
}

#[tauri::command]
pub fn settle_document(root: PathBuf, id: Uuid, text: String) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	settle(&root, id, &text)
}

/// Settles every conflict but the text: `theirs` are the ones the writer took
/// the other version's side of, and the rest keep this version's.
#[tauri::command]
pub fn settle_binder(root: PathBuf, theirs: Vec<merge::Conflict>) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	settle_rest(&root, &theirs)
}

#[tauri::command]
pub fn merge_file(root: PathBuf, path: String, side: String) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	settle_file(&root, &path, &side)
}

/// One side's cover while a merge is under way, as raw bytes like
/// `read_cover`.
#[tauri::command]
pub fn merge_cover(root: PathBuf, theirs: bool) -> Result<tauri::ipc::Response> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Ok(tauri::ipc::Response::new(side_cover(&root, theirs)?))
}

/// Records that the writer has seen the note about an enclosing repository.
#[tauri::command]
pub fn nested_notice_shown(root: PathBuf) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Repository::open(&root)?
		.config()?
		.set_bool(NESTED_NOTICE_SHOWN, true)?;
	Ok(())
}

/// Either side left out is the project as it stands now.
#[tauri::command]
pub fn version_changes(root: PathBuf, a: Option<String>, b: Option<String>) -> Result<Changes> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	changes(&root, a.as_deref(), b.as_deref())
}

#[tauri::command]
pub fn version_status(root: PathBuf) -> Result<Status> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	status(&root)
}

#[tauri::command]
pub fn stage_row(root: PathBuf, row: Row) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	carry(&root, row, true)
}

#[tauri::command]
pub fn unstage_row(root: PathBuf, row: Row) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	carry(&root, row, false)
}

#[tauri::command]
pub fn stage_all(root: PathBuf) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	let repo = Repository::open(&root)?;
	refuse_while_merging(&repo)?;
	stage(&repo)?;
	Ok(())
}

#[tauri::command]
pub fn unstage_all(root: PathBuf) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	let repo = Repository::open(&root)?;
	refuse_while_merging(&repo)?;
	let mut index = repo.index()?;
	match head_commit(&repo)? {
		Some(head) => index.read_tree(&head.tree()?)?,
		None => index.clear()?,
	}
	index.write()?;
	Ok(())
}

#[tauri::command]
pub fn version_text(root: PathBuf, version: String, id: Uuid) -> Result<Option<String>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	text(&root, &version, id)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::document::{
		create_document, create_folder, delete_document, delete_folder, list_trash, move_node,
		read_document, rename_document, rename_folder, write_document,
	};
	use crate::project::{Format, create, set_cover, write_book, write_manifest};
	use crate::takes::{
		TakeView, choose_take, delete_take, list_takes, open_as_take, rename_take, take_text,
		try_something,
	};
	use crate::tree::Node;
	use git2::RepositoryState;

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

	const FIRST: &str = "First";

	/// Opens the project and adds a first checkpoint, `FIRST`, so it has one
	/// to go back to.
	fn begun(root: &Path) {
		ensure_repository(root).unwrap();
		checkpoint(root, FIRST, "").unwrap();
	}

	fn unborn_main(repo: &Repository) -> bool {
		let head = repo.find_reference("HEAD").unwrap();
		head.symbolic_target().unwrap() == Some("refs/heads/main")
			&& repo.head().err().map(|e| e.code()) == Some(ErrorCode::UnbornBranch)
	}

	#[test]
	fn a_fresh_folder_begins_on_main_with_nothing_kept() {
		let dir = project();
		assert_eq!(ensure_repository(dir.path()).unwrap(), None);

		let repo = Repository::open(dir.path()).unwrap();
		assert!(unborn_main(&repo));
		assert!(checkpoints(dir.path(), None, None).unwrap().is_empty());
	}

	#[test]
	fn opening_again_keeps_no_new_version() {
		let dir = project();
		ensure_repository(dir.path()).unwrap();
		ensure_repository(dir.path()).unwrap();

		let repo = Repository::open(dir.path()).unwrap();
		assert!(unborn_main(&repo));
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

		ensure_repository(dir.path()).unwrap();

		assert_eq!(repo.head().unwrap().target(), Some(theirs));
		assert_eq!(versions(&repo), 1);
		// The one setting Aurora adds, so checkouts never rewrite line endings.
		assert_eq!(own_autocrlf(&repo), Some(false));
	}

	/// The setting in the repository's own config, whatever the machine says.
	fn own_autocrlf(repo: &Repository) -> Option<bool> {
		let config = repo.config().unwrap();
		let own = config.open_level(git2::ConfigLevel::Local).unwrap();
		own.get_bool("core.autocrlf").ok()
	}

	#[test]
	fn a_new_repository_keeps_line_endings_as_they_were_kept() {
		let dir = project();
		ensure_repository(dir.path()).unwrap();

		let repo = Repository::open(dir.path()).unwrap();
		assert_eq!(own_autocrlf(&repo), Some(false));
	}

	#[test]
	fn excluded_files_are_never_kept() {
		let dir = project();
		fs::write(dir.path().join(HISTORY_FILE), "{}").unwrap();
		fs::create_dir(dir.path().join(TRASH_DIR)).unwrap();
		fs::write(dir.path().join(TRASH_DIR).join("gone.md"), "gone").unwrap();

		begun(dir.path());

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

		let found = ensure_repository(&root).unwrap().unwrap();
		// Compared by name: the platforms disagree about how to spell the path.
		assert_eq!(found.file_name(), outer.path().file_name());
		assert!(root.join(".git").is_dir());

		Repository::open(&root)
			.unwrap()
			.config()
			.unwrap()
			.set_bool(NESTED_NOTICE_SHOWN, true)
			.unwrap();
		assert_eq!(ensure_repository(&root).unwrap(), None);
	}

	#[test]
	fn a_document_is_followed_through_renames_and_moves() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(
			parent.path(),
			"Ithaca",
			Format::Novel,
			OffsetDateTime::UNIX_EPOCH,
		)
		.unwrap();
		begun(&root);
		let manifest = read_manifest(&root).unwrap();
		let seeded = &manifest.documents()[0];
		let document = seeded.id;
		let elsewhere = manifest
			.nodes
			.iter()
			.find_map(|node| match node {
				Node::Folder { id, name, .. } if !seeded.path.starts_with(&format!("{name}/")) => {
					Some(*id)
				}
				_ => None,
			})
			.unwrap();
		let mut expected = vec![repo_head(&root).to_string()];

		write_document(root.clone(), document, "One".to_owned()).unwrap();
		expected.push(kept(&root));

		let other = create_document(root.clone(), elsewhere, "Other".to_owned())
			.unwrap()
			.id;
		let other_added = kept(&root);

		rename_document(root.clone(), document, "Renamed".to_owned()).unwrap();
		kept(&root);

		write_document(root.clone(), document, "One two".to_owned()).unwrap();
		expected.push(kept(&root));

		move_node(root.clone(), document, elsewhere, 0).unwrap();
		write_document(root.clone(), document, "One two three".to_owned()).unwrap();
		expected.push(kept(&root));

		let ids = |document| -> Vec<String> {
			checkpoints(&root, None, Some(document))
				.unwrap()
				.into_iter()
				.map(|row| row.id)
				.collect()
		};
		expected.reverse();
		assert_eq!(ids(document), expected);
		let words: Vec<_> = checkpoints(&root, None, Some(document))
			.unwrap()
			.iter()
			.map(|v| v.words)
			.collect();
		assert_eq!(words, [Some(3), Some(2), Some(1), Some(0)]);
		assert_eq!(ids(other), [other_added]);
	}

	fn repo_head(root: &Path) -> Oid {
		Repository::open(root)
			.unwrap()
			.head()
			.unwrap()
			.target()
			.unwrap()
	}

	#[test]
	fn a_checkpoint_needs_a_message() {
		let dir = project();
		ensure_repository(dir.path()).unwrap();

		let refused = checkpoint(dir.path(), " \n\t", "");
		assert!(matches!(refused, Err(Error::EmptyMessage)));
		assert!(unborn_main(&Repository::open(dir.path()).unwrap()));
	}

	#[test]
	fn the_first_checkpoint_on_main_has_no_parent() {
		let dir = project();
		ensure_repository(dir.path()).unwrap();
		let message = "Opening pages\n\nStill rough.";
		let id = checkpoint(dir.path(), message, "Herman Melville").unwrap();

		let repo = Repository::open(dir.path()).unwrap();
		let commit = repo.find_commit(id).unwrap();
		assert_eq!(commit.parent_count(), 0);
		assert_eq!(commit.message().unwrap(), message);
		assert_eq!(repo.head().unwrap().shorthand().unwrap(), "main");
		assert!(
			commit
				.tree()
				.unwrap()
				.get_path(Path::new("Manuscript/one.md"))
				.is_ok()
		);

		let rows = checkpoints(dir.path(), None, None).unwrap();
		assert_eq!(rows.len(), 1);
		assert_eq!(rows[0].id, id.to_string());
		assert_eq!(rows[0].message, message);
		assert_eq!(rows[0].author, "Herman Melville");
		assert_eq!(rows[0].words, None);
	}

	#[test]
	fn a_document_lists_only_the_checkpoints_that_changed_its_text() {
		let (_parent, root, document, elsewhere, _) = novel();
		let first = repo_head(&root);

		write_document(root.clone(), document, "One".to_owned()).unwrap();
		let one = checkpoint(&root, "One word", "").unwrap();
		create_document(root.clone(), elsewhere, "Other".to_owned()).unwrap();
		checkpoint(&root, "Another document", "").unwrap();
		write_document(root.clone(), document, "One two".to_owned()).unwrap();
		let two = checkpoint(&root, "Two words", "").unwrap();

		let rows = checkpoints(&root, None, Some(document)).unwrap();
		let ids: Vec<Oid> = rows.iter().map(|r| Oid::from_str(&r.id).unwrap()).collect();
		assert_eq!(ids, [two, one, first]);
		assert_eq!(rows[0].words, Some(2));
		assert_eq!(rows[1].words, Some(1));
		assert_eq!(
			checkpoints(&root, Some("main"), Some(document)).unwrap(),
			rows
		);
		assert!(
			checkpoints(&root, Some("elsewhere"), None)
				.unwrap()
				.is_empty()
		);
	}

	#[test]
	fn a_foreign_commit_lists_with_its_own_message() {
		let dir = project();
		let repo = Repository::init(dir.path()).unwrap();
		let mut index = repo.index().unwrap();
		index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
		let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
		let signature = Signature::now("Writer", "writer@example.com").unwrap();
		repo.commit(
			Some("HEAD"),
			&signature,
			&signature,
			"Chapter one done\n\nLong notes.\n",
			&tree,
			&[],
		)
		.unwrap();

		let rows = checkpoints(dir.path(), None, None).unwrap();
		assert_eq!(rows[0].message, "Chapter one done\n\nLong notes.");
		assert_eq!(rows[0].author, "Writer");
	}

	#[test]
	fn a_slug_is_lowercase_letters_and_digits_joined_by_single_hyphens() {
		assert_eq!(slug("Second Draft").unwrap(), "second-draft");
		assert_eq!(slug("  Tom's -- ending!! ").unwrap(), "tom-s-ending");
		assert_eq!(slug("Café 2").unwrap(), "café-2");
		assert!(matches!(slug(" -!- "), Err(Error::VersionName)));
		assert!(matches!(slug(""), Err(Error::VersionName)));
	}

	fn head_target(root: &Path) -> String {
		let repo = Repository::open(root).unwrap();
		let head = repo.find_reference("HEAD").unwrap();
		head.symbolic_target().unwrap().unwrap().to_owned()
	}

	fn typed_name(root: &Path, slug: &str) -> Option<String> {
		let config = Repository::open(root).unwrap().config().unwrap();
		config.get_string(&name_key(slug)).ok()
	}

	#[test]
	fn a_new_version_starts_here_and_brings_the_changes_along() {
		let (_parent, root, document, _, _) = novel();
		let first = repo_head(&root);
		write_document(root.clone(), document, "Not yet kept".to_owned()).unwrap();
		let index = fs::read(root.join(".git/index")).unwrap();

		assert_eq!(new_branch(&root, " Second Draft ").unwrap(), "second-draft");
		assert_eq!(head_target(&root), "refs/heads/second-draft");
		assert_eq!(repo_head(&root), first);
		assert_eq!(body_now(&root, document), "Not yet kept");
		assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);

		let rows = branches(&root).unwrap();
		let expected = [
			Branch {
				slug: "main".to_owned(),
				name: "Main".to_owned(),
				current: false,
				unmerged: 0,
				fixed: true,
			},
			Branch {
				slug: "second-draft".to_owned(),
				name: "Second Draft".to_owned(),
				current: true,
				unmerged: 0,
				fixed: false,
			},
		];
		assert_eq!(rows, expected);

		let taken = new_branch(&root, "second draft!");
		assert!(matches!(taken, Err(Error::VersionExists)));
		assert!(matches!(
			new_branch(&root, "Main"),
			Err(Error::VersionExists)
		));
	}

	#[test]
	fn a_new_version_waits_for_the_first_checkpoint() {
		let dir = project();
		ensure_repository(dir.path()).unwrap();

		let refused = new_branch(dir.path(), "Second Draft");
		assert!(matches!(refused, Err(Error::NothingKept)));
		let rows = branches(dir.path()).unwrap();
		assert_eq!(rows.len(), 1);
		assert_eq!(rows[0].name, "Main");
		assert!(rows[0].current);
	}

	#[test]
	fn a_renamed_version_takes_its_typed_name_along() {
		let (_parent, root, _, _, _) = novel();
		new_branch(&root, "Draft Two").unwrap();

		assert_eq!(
			renamed_branch(&root, "draft-two", "Draft Three").unwrap(),
			"draft-three"
		);
		assert_eq!(head_target(&root), "refs/heads/draft-three");
		assert_eq!(typed_name(&root, "draft-two"), None);
		assert_eq!(
			typed_name(&root, "draft-three").as_deref(),
			Some("Draft Three")
		);

		assert_eq!(
			renamed_branch(&root, "draft-three", "DRAFT three").unwrap(),
			"draft-three"
		);
		assert_eq!(
			typed_name(&root, "draft-three").as_deref(),
			Some("DRAFT three")
		);

		new_branch(&root, "Other").unwrap();
		let taken = renamed_branch(&root, "other", "Draft three");
		assert!(matches!(taken, Err(Error::VersionExists)));
		let main = renamed_branch(&root, "main", "Trunk");
		assert!(matches!(main, Err(Error::MainFixed)));
	}

	#[test]
	fn neither_main_nor_the_version_the_writer_is_in_can_be_deleted() {
		let (_parent, root, _, _, _) = novel();
		new_branch(&root, "Aside").unwrap();

		let current = dropped_branch(&root, "aside");
		assert!(matches!(current, Err(Error::CurrentVersion)));
		let main = dropped_branch(&root, "main");
		assert!(matches!(main, Err(Error::MainFixed)));

		Repository::open(&root)
			.unwrap()
			.set_head("refs/heads/main")
			.unwrap();
		dropped_branch(&root, "aside").unwrap();
		assert_eq!(typed_name(&root, "aside"), None);
		let slugs: Vec<String> = branches(&root)
			.unwrap()
			.into_iter()
			.map(|row| row.slug)
			.collect();
		assert_eq!(slugs, ["main"]);
	}

	/// The project as a repository made elsewhere leaves it, on `master`.
	fn adopted(root: &Path) {
		let repo = Repository::open(root).unwrap();
		repo.find_branch("main", BranchType::Local)
			.unwrap()
			.rename("master", false)
			.unwrap();
		repo.set_head("refs/heads/master").unwrap();
	}

	fn listed(root: &Path) -> Vec<(String, String, bool, bool)> {
		branches(root)
			.unwrap()
			.into_iter()
			.map(|row| (row.slug, row.name, row.current, row.fixed))
			.collect()
	}

	#[test]
	fn an_adopted_master_is_main_from_any_other_version() {
		let (_parent, root, _, _, _) = novel();
		adopted(&root);
		new_branch(&root, "Feature").unwrap();
		let repo = Repository::open(&root).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		repo.branch("zebra", &head, false).unwrap();

		assert_eq!(
			listed(&root),
			[
				("master".to_owned(), "Main".to_owned(), false, true),
				("feature".to_owned(), "Feature".to_owned(), true, false),
				("zebra".to_owned(), "zebra".to_owned(), false, false),
			]
		);
		let renamed = renamed_branch(&root, "master", "Trunk");
		assert!(matches!(renamed, Err(Error::MainFixed)));
		let dropped = dropped_branch(&root, "master");
		assert!(matches!(dropped, Err(Error::MainFixed)));
	}

	#[test]
	fn a_version_named_main_never_takes_mains_place() {
		let (_parent, root, _, _, _) = novel();
		adopted(&root);

		assert!(matches!(
			new_branch(&root, "Main"),
			Err(Error::VersionExists)
		));
		let draft = new_branch(&root, "Draft").unwrap();
		assert!(matches!(
			renamed_branch(&root, &draft, "Master"),
			Err(Error::VersionExists)
		));
	}

	#[test]
	fn any_author_can_sign_a_checkpoint() {
		for author in ["A <b>", "...", "<>", "Line\nbreak"] {
			assert!(signature(author).is_ok(), "{author:?}");
		}
		assert_eq!(signature("<>").unwrap().name().ok(), Some("Aurora"));
	}

	#[test]
	fn a_checkpoint_with_nothing_new_is_refused() {
		let (_parent, root, _, _, _) = novel();
		let refused = checkpoint(&root, "Again", "");
		assert!(matches!(refused, Err(Error::NothingToKeep)));
	}

	#[test]
	fn a_desktop_file_never_counts_as_a_change() {
		let (_parent, root, _, _, _) = novel();
		fs::write(root.join(".DS_Store"), "junk").unwrap();
		fs::write(root.join("Manuscript/Thumbs.db"), "junk").unwrap();
		assert!(!pending(&root).unwrap());
	}

	#[cfg(unix)]
	#[test]
	fn a_switch_that_fails_partway_still_names_the_version_reached() {
		use std::os::unix::fs::PermissionsExt;
		let (_parent, root, _, here, _) = novel();
		new_branch(&root, "Draft").unwrap();
		create_folder(root.clone(), here, "Empty".to_owned(), None).unwrap();
		checkpoint(&root, "On Draft", "").unwrap();
		switch(&root, "main").unwrap();
		let folder = root.join(tree::path(&read_manifest(&root).unwrap().nodes, here).unwrap());
		fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();

		let failed = switch(&root, "draft");

		fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
		assert!(failed.is_err());
		let repo = Repository::open(&root).unwrap();
		assert_eq!(current_branch(&repo).unwrap().as_deref(), Some("draft"));
	}

	#[test]
	fn master_beside_main_is_a_version_like_any_other() {
		let (_parent, root, _, _, _) = novel();
		let repo = Repository::open(&root).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		repo.branch("master", &head, false).unwrap();

		assert_eq!(
			listed(&root),
			[
				("main".to_owned(), "Main".to_owned(), true, true),
				("master".to_owned(), "master".to_owned(), false, false),
			]
		);
		dropped_branch(&root, "master").unwrap();
	}

	#[test]
	fn a_detached_checkout_can_be_left_but_not_deleted() {
		let (_parent, root, _, _, _) = novel();
		let repo = Repository::open(&root).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		repo.set_head_detached(head.id()).unwrap();
		fs::write(root.join("notes.txt"), "Only here").unwrap();
		let kept = checkpoint(&root, "Only here", "").unwrap();

		let found = branches(&root).unwrap();
		let main = found.iter().find(|row| row.slug == "main").unwrap();
		assert!(!main.current);
		let loose = found.iter().find(|row| row.current).unwrap();
		assert!(kept.to_string().starts_with(&loose.slug));
		assert_eq!(loose.name, loose.slug);
		assert!(loose.fixed);
		assert_eq!(loose.unmerged, 1);

		let refused = dropped_branch(&root, &loose.slug);
		assert!(matches!(refused, Err(Error::CurrentVersion)));

		switch(&root, "main").unwrap();
		assert_eq!(head_target(&root), "refs/heads/main");
		assert_eq!(
			listed(&root),
			[("main".to_owned(), "Main".to_owned(), true, true)]
		);
	}

	#[test]
	fn unmerged_counts_the_checkpoints_only_one_version_reaches() {
		let (_parent, root, _, _, _) = novel();
		new_branch(&root, "Draft").unwrap();
		fs::write(root.join("notes.txt"), "One").unwrap();
		checkpoint(&root, "One", "").unwrap();
		fs::write(root.join("notes.txt"), "Two").unwrap();
		let draft = checkpoint(&root, "Two", "").unwrap();

		let unmerged = |root: &Path| -> Vec<(String, u32)> {
			branches(root)
				.unwrap()
				.into_iter()
				.map(|row| (row.slug, row.unmerged))
				.collect()
		};
		assert_eq!(
			unmerged(&root),
			[("main".to_owned(), 0), ("draft".to_owned(), 2)]
		);

		let repo = Repository::open(&root).unwrap();
		repo.set_head("refs/heads/main").unwrap();
		let main = repo.head().unwrap().peel_to_commit().unwrap();
		let draft = repo.find_commit(draft).unwrap();
		let signature = Signature::now("Writer", "writer@example.com").unwrap();
		repo.commit(
			Some("HEAD"),
			&signature,
			&signature,
			"Merged Draft",
			&draft.tree().unwrap(),
			&[&main, &draft],
		)
		.unwrap();

		assert_eq!(
			unmerged(&root),
			[("main".to_owned(), 1), ("draft".to_owned(), 0)]
		);
	}

	#[test]
	fn switching_waits_while_changes_are_not_in_a_checkpoint() {
		let (_parent, root, document, _, _) = novel();
		new_branch(&root, "Draft").unwrap();
		fs::write(root.join(HISTORY_FILE), "[]").unwrap();
		assert!(!pending(&root).unwrap());

		write_document(root.clone(), document, "Not yet kept".to_owned()).unwrap();
		assert!(pending(&root).unwrap());
		let refused = switch(&root, "main");
		assert!(matches!(refused, Err(Error::ChangesPending)));
		assert_eq!(head_target(&root), "refs/heads/draft");
		assert_eq!(body_now(&root, document), "Not yet kept");
	}

	#[test]
	fn switching_brings_the_whole_project_to_the_other_version() {
		let (_parent, root, edited, here, there) = novel();
		let moved = create_document(root.clone(), here, "Moved".to_owned())
			.unwrap()
			.id;
		let deleted = create_document(root.clone(), here, "Deleted".to_owned())
			.unwrap()
			.id;
		write_document(root.clone(), edited, "Main's text".to_owned()).unwrap();
		let main = checkpoint(&root, "On Main", "").unwrap().to_string();

		new_branch(&root, "Draft").unwrap();
		write_document(root.clone(), edited, "Draft's text".to_owned()).unwrap();
		create_document(root.clone(), there, "Added".to_owned()).unwrap();
		move_node(root.clone(), moved, there, 0).unwrap();
		delete_document(root.clone(), deleted).unwrap();
		let empty = create_folder(root.clone(), here, "Empty".to_owned(), None)
			.unwrap()
			.id();
		let empty_path = tree::path(&read_manifest(&root).unwrap().nodes, empty).unwrap();
		let draft = checkpoint(&root, "On Draft", "").unwrap().to_string();
		fs::write(root.join(HISTORY_FILE), "[]").unwrap();

		switch(&root, "main").unwrap();
		assert_eq!(head_target(&root), "refs/heads/main");
		assert!(changes(&root, Some(&main), None).unwrap().nodes.is_empty());
		assert!(!pending(&root).unwrap());
		assert_eq!(body_now(&root, edited), "Main's text");
		assert!(!root.join(&empty_path).exists());
		assert_eq!(fs::read_to_string(root.join(HISTORY_FILE)).unwrap(), "[]");
		assert_eq!(list_trash(root.clone()).unwrap().len(), 1);

		switch(&root, "draft").unwrap();
		assert!(changes(&root, Some(&draft), None).unwrap().nodes.is_empty());
		assert!(!pending(&root).unwrap());
		assert!(root.join(&empty_path).is_dir());
	}

	const BASE: &str = "One.\n\nTwo.\n\nThree.\n";

	/// A novel whose first document has `BASE`, then a version "Act two"
	/// where it reads `draft`, and Main where it reads `main`. Act two also
	/// edits a second document. Returns the two documents and both checkpoints.
	fn forked(main: &str, draft: &str) -> (tempfile::TempDir, PathBuf, Uuid, Uuid, Oid, Oid) {
		let (parent, root, seeded, here, _) = novel();
		let other = create_document(root.clone(), here, "Other".to_owned())
			.unwrap()
			.id;
		write_document(root.clone(), seeded, BASE.to_owned()).unwrap();
		checkpoint(&root, "Base", "").unwrap();

		new_branch(&root, "Act two").unwrap();
		write_document(root.clone(), seeded, draft.to_owned()).unwrap();
		write_document(root.clone(), other, "Only in Act two".to_owned()).unwrap();
		let draft = checkpoint(&root, "In Act two", "").unwrap();

		switch(&root, "main").unwrap();
		let main = match main == BASE {
			true => repo_head(&root),
			false => {
				write_document(root.clone(), seeded, main.to_owned()).unwrap();
				checkpoint(&root, "On Main", "").unwrap()
			}
		};
		(parent, root, seeded, other, main, draft)
	}

	fn parents(root: &Path) -> Vec<Oid> {
		let repo = Repository::open(root).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		head.parent_ids().collect()
	}

	fn merge_state_of(root: &Path) -> RepositoryState {
		Repository::open(root).unwrap().state()
	}

	#[test]
	fn a_clean_merge_adds_its_own_checkpoint() {
		let (_parent, root, seeded, other, main, draft) = forked(
			"One, on Main.\n\nTwo.\n\nThree.\n",
			"One.\n\nTwo.\n\nThree, in Act two.\n",
		);

		assert_eq!(merge(&root, "act-two", "").unwrap(), None);

		let repo = Repository::open(&root).unwrap();
		let head = repo.head().unwrap().peel_to_commit().unwrap();
		assert_eq!(head.message().unwrap(), "Merged Act two");
		assert_eq!(parents(&root), [main, draft]);
		assert_eq!(
			read_document(root.clone(), seeded).unwrap(),
			"One, on Main.\n\nTwo.\n\nThree, in Act two.\n"
		);
		assert_eq!(
			read_document(root.clone(), other).unwrap(),
			"Only in Act two"
		);
		assert!(!pending(&root).unwrap());
		assert_eq!(merging(&root).unwrap(), None);
		assert_eq!(merge_state_of(&root), RepositoryState::Clean);

		let again = merge(&root, "act-two", "");
		assert!(matches!(again, Err(Error::AlreadyMerged)));
	}

	#[test]
	fn a_merge_git_could_fast_forward_still_adds_a_checkpoint() {
		let (_parent, root, seeded, _, main, draft) = forked(BASE, "One, in Act two.\n");

		assert_eq!(merge(&root, "act-two", "").unwrap(), None);

		assert_eq!(parents(&root), [main, draft]);
		assert_eq!(
			read_document(root.clone(), seeded).unwrap(),
			"One, in Act two.\n"
		);
	}

	#[test]
	fn merging_waits_while_changes_are_not_in_a_checkpoint() {
		let (_parent, root, seeded, _, main, _) = forked(BASE, "One, in Act two.\n");
		write_document(root.clone(), seeded, "Not yet kept".to_owned()).unwrap();

		let refused = merge(&root, "act-two", "");

		assert!(matches!(refused, Err(Error::ChangesPending)));
		assert_eq!(repo_head(&root), main);
		assert_eq!(merge_state_of(&root), RepositoryState::Clean);
	}

	const MAINS: &str = "One, on Main.\n\nTwo.\n\nThree.\n";

	/// Main and Act two both rewrote the first paragraph.
	fn conflicted() -> (tempfile::TempDir, PathBuf, Uuid, Uuid, Oid, Oid) {
		let forked = forked(MAINS, "One, in Act two.\n\nTwo.\n\nThree.\n");
		let merging = merge(&forked.1, "act-two", "").unwrap().unwrap();
		let path = tree::path(&read_manifest(&forked.1).unwrap().nodes, forked.2).unwrap();
		assert_eq!(
			merging,
			Merging {
				name: Some("Act two".to_owned()),
				message: "Merged Act two".to_owned(),
				conflicts: vec![merge::Conflict::Text {
					id: Some(forked.2),
					path,
				}],
			}
		);
		forked
	}

	#[test]
	fn a_conflicting_paragraph_leaves_this_versions_text_and_waits() {
		let (_parent, root, seeded, other, main, _) = conflicted();

		assert_eq!(repo_head(&root), main);
		assert_eq!(merge_state_of(&root), RepositoryState::Merge);
		assert_eq!(read_document(root.clone(), seeded).unwrap(), MAINS);
		assert_eq!(
			read_document(root.clone(), other).unwrap(),
			"Only in Act two"
		);
		assert_eq!(merging(&root).unwrap().unwrap().conflicts.len(), 1);

		let repo = Repository::open(&root).unwrap();
		let index = repo.index().unwrap();
		let conflict = index.conflicts().unwrap().next().unwrap().unwrap();
		assert!(conflict.ancestor.is_some() && conflict.our.is_some() && conflict.their.is_some());
		assert!(index.get_path(Path::new(MANIFEST_FILE), 0).is_some());
	}

	#[test]
	fn aborting_returns_to_the_last_checkpoint() {
		let (_parent, root, seeded, other, main, _) = conflicted();

		abort(&root).unwrap();

		assert_eq!(repo_head(&root), main);
		assert_eq!(merge_state_of(&root), RepositoryState::Clean);
		assert_eq!(merging(&root).unwrap(), None);
		assert!(!pending(&root).unwrap());
		assert_eq!(read_document(root.clone(), seeded).unwrap(), MAINS);
		assert_eq!(read_document(root.clone(), other).unwrap(), "");
	}

	#[test]
	fn aborting_with_no_merge_keeps_changes() {
		let (_parent, root, seeded, ..) = forked(BASE, "One, in Act two.\n");
		write_document(root.clone(), seeded, "Not yet kept".to_owned()).unwrap();

		abort(&root).unwrap();

		assert_eq!(body_now(&root, seeded), "Not yet kept");
	}

	#[test]
	fn a_checkpoint_finishes_the_merge_once_conflicts_are_settled() {
		let (_parent, root, seeded, _, main, draft) = conflicted();

		let refused = checkpoint(&root, "Merged Act two", "");
		assert!(matches!(refused, Err(Error::ConflictsRemain)));

		assert!(matches!(stage_all(root.clone()), Err(Error::MergeUnderway)));
		assert!(!merging(&root).unwrap().unwrap().conflicts.is_empty());
		settle(&root, seeded, "One, settled.\n").unwrap();
		checkpoint(&root, "Merged Act two", "").unwrap();

		assert_eq!(parents(&root), [main, draft]);
		assert_eq!(merge_state_of(&root), RepositoryState::Clean);
		assert_eq!(merging(&root).unwrap(), None);
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn a_conflicted_document_comes_apart_into_clean_text_and_spots() {
		let (_parent, root, seeded, other, ..) = forked(
			"One, on Main.\n\nTwo.\n\n<<<<<<< typed\n\nThree, on Main.\n",
			"One, in Act two.\n\nTwo.\n\n<<<<<<< typed\n\nThree, in Act two.\n",
		);
		merge(&root, "act-two", "").unwrap().unwrap();

		assert_eq!(
			spots(&root, seeded).unwrap(),
			Some(vec![
				Piece::Spot {
					ours: "One, on Main.\n".to_owned(),
					theirs: "One, in Act two.\n".to_owned(),
					field: None,
				},
				Piece::Clean {
					text: "\nTwo.\n\n<<<<<<< typed\n\n".to_owned(),
				},
				Piece::Spot {
					ours: "Three, on Main.\n".to_owned(),
					theirs: "Three, in Act two.\n".to_owned(),
					field: None,
				},
			])
		);
		assert_eq!(spots(&root, other).unwrap(), None);
	}

	#[test]
	fn a_spot_inside_the_front_matter_is_named_for_its_field() {
		let open = format!("{} x", "<".repeat(MARKER_SIZE));
		let middle = "=".repeat(MARKER_SIZE);
		let close = format!("{} x", ">".repeat(MARKER_SIZE));
		let merged = format!(
			"---\n{open}\nmood: wary\n{middle}\nmood: tense\n{close}\n---\n\n{open}\nOne.\n{middle}\nUno.\n{close}\n"
		);

		assert_eq!(
			pieces(&merged, "x"),
			[
				Piece::Clean {
					text: "---\n".to_owned()
				},
				Piece::Spot {
					ours: "mood: wary\n".to_owned(),
					theirs: "mood: tense\n".to_owned(),
					field: Some("mood".to_owned()),
				},
				Piece::Clean {
					text: "---\n\n".to_owned()
				},
				Piece::Spot {
					ours: "One.\n".to_owned(),
					theirs: "Uno.\n".to_owned(),
					field: None,
				},
			]
		);
	}

	#[test]
	fn settling_a_document_takes_it_off_the_conflicts() {
		let (_parent, root, seeded, _, main, draft) = conflicted();

		settle(&root, seeded, "One, settled.\n\nTwo.\n\nThree.\n").unwrap();

		assert_eq!(spots(&root, seeded).unwrap(), None);
		assert_eq!(merging(&root).unwrap().unwrap().conflicts, []);
		assert_eq!(
			read_document(root.clone(), seeded).unwrap(),
			"One, settled.\n\nTwo.\n\nThree.\n"
		);
		checkpoint(&root, "Merged Act two", "").unwrap();
		assert_eq!(parents(&root), [main, draft]);
		assert!(!pending(&root).unwrap());
	}

	/// A novel with versions, its first document, and two top-level folders
	/// that do not hold that document.
	fn novel() -> (tempfile::TempDir, PathBuf, Uuid, Uuid, Uuid) {
		let parent = tempfile::tempdir().unwrap();
		let root = create(
			parent.path(),
			"Ithaca",
			Format::Novel,
			OffsetDateTime::UNIX_EPOCH,
		)
		.unwrap();
		begun(&root);
		let manifest = read_manifest(&root).unwrap();
		let seeded = manifest.documents()[0].clone();
		let others: Vec<Uuid> = manifest
			.nodes
			.iter()
			.filter_map(|node| match node {
				Node::Folder { id, name, .. } if !seeded.path.starts_with(&format!("{name}/")) => {
					Some(*id)
				}
				_ => None,
			})
			.collect();
		(parent, root, seeded.id, others[0], others[1])
	}

	fn kept(root: &Path) -> String {
		held(root, "Kept").to_string()
	}

	/// A checkpoint, or the last one when nothing has changed since.
	fn held(root: &Path, message: &str) -> Oid {
		match checkpoint(root, message, "") {
			Ok(id) => id,
			Err(Error::NothingToKeep) => repo_head(root),
			Err(e) => panic!("{e}"),
		}
	}

	fn change_of(changes: &Changes, id: Uuid) -> Option<&Change> {
		changes.nodes.iter().find(|change| change.id == id)
	}

	#[test]
	fn documents_are_matched_by_id_through_every_kind_of_change() {
		let (_parent, root, seeded, here, there) = novel();
		write_document(root.clone(), seeded, "One".to_owned()).unwrap();
		let add = |name: &str| {
			create_document(root.clone(), here, name.to_owned())
				.unwrap()
				.id
		};
		let (moving, doomed, untouched) = (add("Moving"), add("Doomed"), add("Untouched"));
		let start = kept(&root);

		rename_document(root.clone(), seeded, "Renamed".to_owned()).unwrap();
		write_document(root.clone(), seeded, "One two".to_owned()).unwrap();
		move_node(root.clone(), moving, there, 0).unwrap();
		delete_document(root.clone(), doomed).unwrap();
		let added = add("Added");
		let end = kept(&root);

		let changes = changes(&root, Some(&start), Some(&end)).unwrap();
		assert_eq!(changes.nodes.len(), 4);
		assert!(change_of(&changes, untouched).is_none());

		let renamed = change_of(&changes, seeded).unwrap();
		assert!(renamed.renamed && renamed.edited && !renamed.moved);
		assert!(renamed.after.as_deref().unwrap().ends_with("Renamed.md"));

		let moved = change_of(&changes, moving).unwrap();
		assert!(moved.moved && !moved.renamed && !moved.edited);

		let deleted = change_of(&changes, doomed).unwrap();
		assert!(deleted.before.is_some() && deleted.after.is_none());

		let added = change_of(&changes, added).unwrap();
		assert!(added.before.is_none() && added.after.is_some() && !added.folder);
		assert!(!changes.book && !changes.cover);
	}

	#[test]
	fn a_renamed_folder_does_not_move_what_is_inside_it() {
		let (_parent, root, _, here, there) = novel();
		let folder = |parent: Uuid, name: &str| {
			create_folder(root.clone(), parent, name.to_owned(), None)
				.unwrap()
				.id()
		};
		let (old, moving, doomed) = (
			folder(here, "Old"),
			folder(here, "Moving"),
			folder(here, "Doomed"),
		);
		let inside = create_document(root.clone(), old, "Inside".to_owned())
			.unwrap()
			.id;
		let start = kept(&root);

		rename_folder(root.clone(), old, "New".to_owned()).unwrap();
		move_node(root.clone(), moving, there, 0).unwrap();
		delete_folder(root.clone(), doomed).unwrap();
		let fresh = folder(here, "Fresh");
		let end = kept(&root);

		let changes = changes(&root, Some(&start), Some(&end)).unwrap();
		assert!(change_of(&changes, inside).is_none());
		let renamed = change_of(&changes, old).unwrap();
		assert!(renamed.folder && renamed.renamed && !renamed.moved);
		assert!(change_of(&changes, moving).unwrap().moved);
		assert!(change_of(&changes, doomed).unwrap().after.is_none());
		assert!(change_of(&changes, fresh).unwrap().before.is_none());
	}

	#[test]
	fn a_folder_reordered_is_one_change_and_an_addition_is_not_a_reorder() {
		let (_parent, root, _, here, there) = novel();
		let add = |parent: Uuid, name: &str| {
			create_document(root.clone(), parent, name.to_owned())
				.unwrap()
				.id
		};
		let (_, _, last) = (add(here, "One"), add(here, "Two"), add(here, "Three"));
		let start = kept(&root);

		move_node(root.clone(), last, here, 0).unwrap();
		add(there, "New");
		let end = kept(&root);

		let changes = changes(&root, Some(&start), Some(&end)).unwrap();
		let folder = change_of(&changes, here).unwrap();
		assert!(!folder.renamed && !folder.moved);
		assert_eq!(folder.reordered.len(), 1);
		assert!(folder.reordered[0].name.starts_with("Three"));
		assert_eq!(folder.reordered[0].after, None);
		assert!(change_of(&changes, last).is_none());
		assert!(change_of(&changes, there).is_none());

		// To the end, now after the last of the others.
		let held = read_manifest(&root)
			.unwrap()
			.nodes
			.iter()
			.find_map(|node| match node {
				Node::Folder { id, children, .. } if *id == here => Some(children.len()),
				_ => None,
			})
			.unwrap();
		move_node(root.clone(), last, here, held - 1).unwrap();
		let later = kept(&root);
		let changes = super::changes(&root, Some(&end), Some(&later)).unwrap();
		let moved = &change_of(&changes, here).unwrap().reordered;
		assert_eq!(moved.len(), 1);
		assert!(moved[0].name.starts_with("Three"));
		assert!(moved[0].after.as_deref().unwrap().starts_with("Two"));
	}

	#[test]
	fn book_details_and_the_cover_are_told_apart() {
		let (_parent, root, ..) = novel();
		let start = kept(&root);
		let mut book = read_manifest(&root).unwrap().book;
		book.title = "Odyssey".to_owned();
		write_book(root.clone(), book).unwrap();
		let retitled = kept(&root);

		let image = root.parent().unwrap().join("cover.png");
		fs::write(&image, b"\x89PNG\r\n\x1a\none").unwrap();
		set_cover(root.clone(), image.clone()).unwrap();
		let covered = kept(&root);
		fs::write(&image, b"\x89PNG\r\n\x1a\ntwo").unwrap();
		set_cover(root.clone(), image).unwrap();
		let recovered = kept(&root);

		let book = changes(&root, Some(&start), Some(&retitled)).unwrap();
		assert!(book.book && !book.cover && book.nodes.is_empty());
		let cover = changes(&root, Some(&retitled), Some(&covered)).unwrap();
		assert!(cover.cover && !cover.book);
		// The same file name, with other bytes in it.
		assert!(
			changes(&root, Some(&covered), Some(&recovered))
				.unwrap()
				.cover
		);
	}

	#[test]
	fn now_is_the_folder_on_disk_and_keeps_nothing() {
		let (_parent, root, seeded, ..) = novel();
		let head = repo_head(&root);
		write_document(root.clone(), seeded, "Unkept".to_owned()).unwrap();

		let changes = changes(&root, Some(&head.to_string()), None).unwrap();
		assert!(change_of(&changes, seeded).unwrap().edited);
		assert_eq!(repo_head(&root), head);
	}

	#[test]
	fn before_anything_is_kept_every_document_reads_as_added() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(
			parent.path(),
			"Ithaca",
			Format::Novel,
			OffsetDateTime::UNIX_EPOCH,
		)
		.unwrap();
		ensure_repository(&root).unwrap();

		let changes = changes(&root, Some("HEAD"), None).unwrap();
		let documents = read_manifest(&root).unwrap().documents();
		assert!(!documents.is_empty());
		for document in documents {
			let change = change_of(&changes, document.id).unwrap();
			assert_eq!(change.before, None);
			assert!(change.after.is_some());
		}
		assert!(!root.join(".git/index").exists());
	}

	fn loose_objects(root: &Path) -> usize {
		fs::read_dir(root.join(".git/objects"))
			.unwrap()
			.flatten()
			.filter(|entry| entry.file_name().len() == 2)
			.map(|entry| fs::read_dir(entry.path()).unwrap().count())
			.sum()
	}

	#[test]
	fn looking_at_the_changes_writes_nothing_to_the_object_store() {
		let (_parent, root, seeded, ..) = novel();
		write_document(root.clone(), seeded, "Unkept".to_owned()).unwrap();
		status(&root).unwrap();
		let before = loose_objects(&root);

		for round in 0..10 {
			write_document(root.clone(), seeded, format!("Unkept {round}")).unwrap();
			status(&root).unwrap();
			changes(&root, Some("HEAD"), None).unwrap();
		}

		assert_eq!(loose_objects(&root), before);
	}

	#[test]
	fn comparing_against_now_leaves_what_is_staged_alone() {
		let (_parent, root, seeded, ..) = novel();
		write_document(root.clone(), seeded, "Unkept".to_owned()).unwrap();
		let index = root.join(".git/index");
		let staged = fs::read(&index).unwrap();

		let changes = changes(&root, Some("HEAD"), None).unwrap();
		assert!(change_of(&changes, seeded).unwrap().edited);
		assert_eq!(fs::read(&index).unwrap(), staged);
	}

	#[test]
	fn before_the_first_checkpoint_nothing_is_staged_and_everything_is_added() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(
			parent.path(),
			"Ithaca",
			Format::Novel,
			OffsetDateTime::UNIX_EPOCH,
		)
		.unwrap();
		ensure_repository(&root).unwrap();

		let status = status(&root).unwrap();
		assert_eq!(status.staged.nodes, Vec::new());
		assert!(!status.staged.book);
		assert!(!status.staged.cover);
		for document in read_manifest(&root).unwrap().documents() {
			assert_eq!(
				change_of(&status.unstaged, document.id).unwrap().before,
				None
			);
		}
		assert!(status.unstaged.book);
		assert!(!root.join(".git/index").exists());
	}

	#[test]
	fn after_a_checkpoint_every_kind_of_change_is_unstaged() {
		let (_parent, root, seeded, here, there) = novel();
		let add = |parent: Uuid, name: &str| {
			create_document(root.clone(), parent, name.to_owned())
				.unwrap()
				.id
		};
		let (moving, doomed, renaming) = (
			add(here, "Moving"),
			add(here, "Doomed"),
			add(here, "Renaming"),
		);
		let (_, two) = (add(here, "One"), add(here, "Two"));
		let part = create_folder(root.clone(), there, "Part".to_owned(), None)
			.unwrap()
			.id();
		checkpoint(&root, "Set up", "").unwrap();

		write_document(root.clone(), seeded, "Edited".to_owned()).unwrap();
		rename_document(root.clone(), renaming, "Renamed".to_owned()).unwrap();
		move_node(root.clone(), moving, there, 0).unwrap();
		delete_document(root.clone(), doomed).unwrap();
		let added = add(here, "Added");
		move_node(root.clone(), two, here, 0).unwrap();
		rename_folder(root.clone(), part, "Act".to_owned()).unwrap();
		let mut book = read_manifest(&root).unwrap().book;
		book.title = "Odyssey".to_owned();
		write_book(root.clone(), book).unwrap();
		let image = root.parent().unwrap().join("cover.png");
		fs::write(&image, b"\x89PNG\r\n\x1a\none").unwrap();
		set_cover(root.clone(), image).unwrap();

		let status = status(&root).unwrap();
		assert!(status.staged.nodes.is_empty() && !status.staged.book && !status.staged.cover);
		let unstaged = &status.unstaged;
		assert_eq!(*unstaged, changes(&root, Some("HEAD"), None).unwrap());
		assert!(change_of(unstaged, seeded).unwrap().edited);
		assert!(change_of(unstaged, renaming).unwrap().renamed);
		assert!(change_of(unstaged, moving).unwrap().moved);
		assert_eq!(change_of(unstaged, doomed).unwrap().after, None);
		assert_eq!(change_of(unstaged, added).unwrap().before, None);
		assert!(change_of(unstaged, part).unwrap().renamed);
		let reordered = &change_of(unstaged, here).unwrap().reordered;
		assert_eq!(reordered.len(), 1);
		assert!(reordered[0].name.starts_with("Two"));
		assert!(unstaged.book && unstaged.cover);
	}

	#[test]
	fn a_document_staged_by_hand_moves_to_the_next_checkpoint() {
		let (_parent, root, seeded, here, _) = novel();
		let other = create_document(root.clone(), here, "Other".to_owned())
			.unwrap()
			.id;
		checkpoint(&root, "Set up", "").unwrap();
		write_document(root.clone(), seeded, "Staged".to_owned()).unwrap();
		write_document(root.clone(), other, "Not staged".to_owned()).unwrap();

		let documents = read_manifest(&root).unwrap().documents();
		let path = &documents.iter().find(|d| d.id == seeded).unwrap().path;
		let repo = Repository::open(&root).unwrap();
		let mut index = repo.index().unwrap();
		index.add_path(Path::new(path)).unwrap();
		index.write().unwrap();

		let status = super::status(&root).unwrap();
		assert!(change_of(&status.staged, seeded).unwrap().edited);
		assert!(change_of(&status.unstaged, seeded).is_none());
		assert!(change_of(&status.staged, other).is_none());
		assert!(change_of(&status.unstaged, other).unwrap().edited);

		// Written again after staging, so partly in the next checkpoint.
		write_document(root.clone(), seeded, "Staged, then more".to_owned()).unwrap();
		let status = super::status(&root).unwrap();
		assert!(change_of(&status.staged, seeded).unwrap().edited);
		assert!(change_of(&status.unstaged, seeded).unwrap().edited);
	}

	fn rows(changes: &Changes) -> Vec<Row> {
		let mut rows: Vec<Row> = changes
			.nodes
			.iter()
			.map(|change| Row::Node { id: change.id })
			.collect();
		if changes.book {
			rows.push(Row::Book);
		}
		if changes.cover {
			rows.push(Row::Cover);
		}
		rows
	}

	fn nothing_in(changes: &Changes) -> bool {
		changes.nodes.is_empty() && !changes.book && !changes.cover
	}

	fn checkpoint_manifest(root: &Path, id: Oid) -> Manifest {
		let repo = Repository::open(root).unwrap();
		side(&repo, Some(&id.to_string())).unwrap().1
	}

	/// How many take files of a document the next checkpoint holds.
	fn take_files_held(root: &Path, id: Uuid) -> usize {
		let repo = Repository::open(root).unwrap();
		let tree = staged_side(&repo).unwrap().0;
		take_files(&repo, &tree, id).len()
	}

	/// A kept document with three takes, the last of them chosen.
	fn three_takes(root: &Path, id: Uuid) -> Vec<Uuid> {
		try_something(root.to_path_buf(), id).unwrap();
		let takes = try_something(root.to_path_buf(), id).unwrap();
		kept(root);
		takes.iter().map(|take| take.id).collect()
	}

	#[test]
	fn a_take_started_is_a_row_though_the_text_is_the_same() {
		let (_parent, root, seeded, ..) = novel();

		try_something(root.clone(), seeded).unwrap();

		let status = status(&root).unwrap();
		let change = change_of(&status.unstaged, seeded).unwrap();
		assert!(change.takes);
		assert!(!change.edited);
	}

	#[test]
	fn a_waiting_take_deleted_is_a_row_though_the_text_is_the_same() {
		let (_parent, root, seeded, ..) = novel();
		let takes = three_takes(&root, seeded);

		delete_take(root.clone(), seeded, takes[0]).unwrap();

		let status = status(&root).unwrap();
		let change = change_of(&status.unstaged, seeded).unwrap();
		assert!(change.takes);
		assert!(!change.edited);
	}

	#[test]
	fn typing_in_the_chosen_take_changes_only_the_text() {
		let (_parent, root, seeded, ..) = novel();
		three_takes(&root, seeded);

		write_document(root.clone(), seeded, "Rewritten".to_owned()).unwrap();

		let status = status(&root).unwrap();
		let change = change_of(&status.unstaged, seeded).unwrap();
		assert!(change.edited);
		assert!(!change.takes);
	}

	#[test]
	fn choosing_a_take_changes_the_text_and_the_takes() {
		let (_parent, root, seeded, ..) = novel();
		write_document(root.clone(), seeded, "One".to_owned()).unwrap();
		let takes = try_something(root.clone(), seeded).unwrap();
		write_document(root.clone(), seeded, "Two".to_owned()).unwrap();
		kept(&root);

		choose_take(root.clone(), seeded, takes[0].id).unwrap();

		let status = status(&root).unwrap();
		let change = change_of(&status.unstaged, seeded).unwrap();
		assert!(change.edited);
		assert!(change.takes);
	}

	#[test]
	fn a_row_brings_its_take_files_in_and_takes_them_out_again() {
		let (_parent, root, seeded, ..) = novel();
		try_something(root.clone(), seeded).unwrap();
		let row = Row::Node { id: seeded };

		carry(&root, row, true).unwrap();

		let picked = status(&root).unwrap();
		assert!(change_of(&picked.staged, seeded).unwrap().takes);
		assert!(nothing_in(&picked.unstaged));
		assert_eq!(take_files_held(&root, seeded), 2);

		carry(&root, row, false).unwrap();

		let dropped = status(&root).unwrap();
		assert!(nothing_in(&dropped.staged));
		assert!(change_of(&dropped.unstaged, seeded).unwrap().takes);
		assert_eq!(take_files_held(&root, seeded), 0);
	}

	#[test]
	fn a_row_carries_a_deleted_take_file_both_ways() {
		let (_parent, root, seeded, ..) = novel();
		let takes = three_takes(&root, seeded);
		delete_take(root.clone(), seeded, takes[0]).unwrap();
		let row = Row::Node { id: seeded };

		carry(&root, row, true).unwrap();
		assert_eq!(take_files_held(&root, seeded), 2);
		assert!(nothing_in(&status(&root).unwrap().unstaged));

		carry(&root, row, false).unwrap();
		assert_eq!(take_files_held(&root, seeded), 3);
		assert!(nothing_in(&status(&root).unwrap().staged));
	}

	#[test]
	fn picking_another_row_leaves_take_files_out() {
		let (_parent, root, seeded, here, _) = novel();
		let other = create_document(root.clone(), here, "Other".to_owned())
			.unwrap()
			.id;
		kept(&root);
		try_something(root.clone(), seeded).unwrap();
		write_document(root.clone(), other, "Changed".to_owned()).unwrap();

		carry(&root, Row::Node { id: other }, true).unwrap();

		assert_eq!(take_files_held(&root, seeded), 0);
		let status = status(&root).unwrap();
		assert!(change_of(&status.staged, seeded).is_none());
		assert!(change_of(&status.unstaged, seeded).unwrap().takes);
	}

	#[test]
	fn a_checkpoint_of_a_picked_row_leaves_nothing_behind() {
		let (_parent, root, seeded, ..) = novel();
		try_something(root.clone(), seeded).unwrap();

		carry(&root, Row::Node { id: seeded }, true).unwrap();
		let id = checkpoint(&root, "Tried something", "").unwrap();

		let status = status(&root).unwrap();
		assert!(nothing_in(&status.staged));
		assert!(nothing_in(&status.unstaged));
		let repo = Repository::open(&root).unwrap();
		let tree = repo.find_commit(id).unwrap().tree().unwrap();
		assert_eq!(take_files(&repo, &tree, seeded).len(), 2);
	}

	#[test]
	fn a_folder_row_takes_out_the_take_files_of_what_it_held() {
		let (_parent, root, seeded, ..) = novel();
		let manifest = read_manifest(&root).unwrap();
		let section = tree::trail(&manifest.nodes, seeded).unwrap()[0].id();
		let part = create_folder(
			root.clone(),
			section,
			"Part".to_owned(),
			Some(tree::FolderKind::Chapter),
		)
		.unwrap()
		.id();
		let inside = create_document(root.clone(), part, "Inside".to_owned())
			.unwrap()
			.id;
		try_something(root.clone(), inside).unwrap();
		kept(&root);
		delete_folder(root.clone(), part).unwrap();

		carry(&root, Row::Node { id: part }, true).unwrap();

		assert_eq!(take_files_held(&root, inside), 0);
	}

	/// A folder with one document, kept, then the document moved out to
	/// `there` and the folder deleted.
	fn moved_out_then_deleted(root: &Path, here: Uuid, there: Uuid) -> (Uuid, Uuid) {
		let act = create_folder(root.to_owned(), here, "Act".to_owned(), None)
			.unwrap()
			.id();
		let scene = create_document(root.to_owned(), act, "Scene".to_owned())
			.unwrap()
			.id;
		checkpoint(root, "Set up", "").unwrap();
		move_node(root.to_owned(), scene, there, 0).unwrap();
		delete_folder(root.to_owned(), act).unwrap();
		(act, scene)
	}

	#[test]
	fn a_deleted_folder_brings_the_move_of_a_document_that_left_it() {
		let (_parent, root, _, here, there) = novel();
		let (act, scene) = moved_out_then_deleted(&root, here, there);

		carry(&root, Row::Node { id: act }, true).unwrap();

		let repo = Repository::open(&root).unwrap();
		let (tree, staged) = staged_side(&repo).unwrap();
		assert!(tree::find(&staged.nodes, act).is_none());
		assert_eq!(
			tree::parent(&staged.nodes, scene).map(Node::id),
			Some(there)
		);
		let path = tree::path(&staged.nodes, scene).unwrap();
		assert!(blob_at(&tree, &path).is_some());
		assert!(nothing_in(&status(&root).unwrap().unstaged));
	}

	#[test]
	fn unstaging_the_move_out_puts_the_deleted_folder_back() {
		let (_parent, root, _, here, there) = novel();
		let (act, scene) = moved_out_then_deleted(&root, here, there);
		carry(&root, Row::Node { id: act }, true).unwrap();

		carry(&root, Row::Node { id: scene }, false).unwrap();

		let status = status(&root).unwrap();
		assert!(nothing_in(&status.staged));
		assert!(change_of(&status.unstaged, act).unwrap().after.is_none());
		assert!(change_of(&status.unstaged, scene).unwrap().moved);
	}

	#[test]
	fn staging_one_of_two_moved_documents_checkpoints_only_that_one() {
		let (_parent, root, _, here, there) = novel();
		let add = |name: &str| {
			create_document(root.clone(), here, name.to_owned())
				.unwrap()
				.id
		};
		let (staged, left) = (add("Staged"), add("Left"));
		checkpoint(&root, "Set up", "").unwrap();
		move_node(root.clone(), staged, there, 0).unwrap();
		move_node(root.clone(), left, there, 0).unwrap();

		carry(&root, Row::Node { id: staged }, true).unwrap();
		let status = status(&root).unwrap();
		assert!(change_of(&status.staged, staged).unwrap().moved);
		assert!(change_of(&status.staged, left).is_none());
		assert!(change_of(&status.unstaged, left).unwrap().moved);
		assert!(change_of(&status.unstaged, staged).is_none());

		let id = checkpoint(&root, "Moved one", "").unwrap();
		let kept = checkpoint_manifest(&root, id);
		let parent = |id: Uuid| tree::parent(&kept.nodes, id).map(Node::id);
		assert_eq!(parent(staged), Some(there));
		assert_eq!(parent(left), Some(here));

		// The checkpoint, written out on its own, reads back with every id.
		let out = tempfile::tempdir().unwrap();
		let repo = Repository::open(&root).unwrap();
		let tree = repo.find_commit(id).unwrap().tree().unwrap();
		repo.checkout_tree(
			tree.as_object(),
			Some(CheckoutBuilder::new().target_dir(out.path()).force()),
		)
		.unwrap();
		let mut read = crate::document::refresh(out.path()).unwrap().documents();
		let mut held = kept.documents();
		read.sort_by_key(|document| document.id);
		held.sort_by_key(|document| document.id);
		assert_eq!(read, held);
		assert!(
			change_of(&super::status(&root).unwrap().unstaged, left)
				.unwrap()
				.moved
		);
	}

	#[test]
	fn a_document_brings_its_new_folder_and_the_folder_takes_it_back() {
		let (_parent, root, _, here, _) = novel();
		held(&root, "Set up");
		let act = create_folder(root.clone(), here, "Act".to_owned(), None)
			.unwrap()
			.id();
		let add = |name: &str| {
			create_document(root.clone(), act, name.to_owned())
				.unwrap()
				.id
		};
		let (scene, other) = (add("Scene"), add("Other"));

		carry(&root, Row::Node { id: scene }, true).unwrap();
		let status = status(&root).unwrap();
		assert_eq!(change_of(&status.staged, act).unwrap().before, None);
		assert_eq!(change_of(&status.staged, scene).unwrap().before, None);
		assert!(change_of(&status.staged, other).is_none());
		assert!(change_of(&status.unstaged, other).is_some());

		carry(&root, Row::Node { id: act }, false).unwrap();
		assert!(nothing_in(&super::status(&root).unwrap().staged));
	}

	#[test]
	fn unstaging_writes_back_what_the_last_checkpoint_had() {
		let (_parent, root, seeded, _, there) = novel();
		held(&root, "Set up");
		write_document(root.clone(), seeded, "Edited".to_owned()).unwrap();
		rename_document(root.clone(), seeded, "Renamed".to_owned()).unwrap();
		move_node(root.clone(), seeded, there, 0).unwrap();

		carry(&root, Row::Node { id: seeded }, true).unwrap();
		let staged = status(&root).unwrap().staged;
		let change = change_of(&staged, seeded).unwrap();
		assert!(change.edited && change.renamed && change.moved);

		carry(&root, Row::Node { id: seeded }, false).unwrap();
		let status = super::status(&root).unwrap();
		assert!(nothing_in(&status.staged));
		let change = change_of(&status.unstaged, seeded).unwrap();
		assert!(change.edited && change.renamed && change.moved);
	}

	#[test]
	fn the_book_the_cover_and_a_folders_order_stage_on_their_own() {
		let (_parent, root, _, here, _) = novel();
		let add = |name: &str| {
			create_document(root.clone(), here, name.to_owned())
				.unwrap()
				.id
		};
		let (_, last) = (add("One"), add("Two"));
		checkpoint(&root, "Set up", "").unwrap();
		move_node(root.clone(), last, here, 0).unwrap();
		let mut book = read_manifest(&root).unwrap().book;
		book.title = "Odyssey".to_owned();
		write_book(root.clone(), book).unwrap();
		let image = root.parent().unwrap().join("cover.png");
		fs::write(&image, b"\x89PNG\r\n\x1a\none").unwrap();
		set_cover(root.clone(), image).unwrap();

		carry(&root, Row::Book, true).unwrap();
		let status = status(&root).unwrap();
		assert!(status.staged.book && !status.staged.cover && status.staged.nodes.is_empty());
		assert!(status.unstaged.cover && !status.unstaged.book);

		carry(&root, Row::Cover, true).unwrap();
		carry(&root, Row::Node { id: here }, true).unwrap();
		let status = super::status(&root).unwrap();
		assert!(status.staged.cover);
		assert_eq!(change_of(&status.staged, here).unwrap().reordered.len(), 1);
		assert!(nothing_in(&status.unstaged));
	}

	/// Every kind of change, so staging them one at a time has to arrive at
	/// the project as it is.
	fn everything_changed(root: &Path, seeded: Uuid, here: Uuid, there: Uuid) {
		let add = |parent: Uuid, name: &str| {
			create_document(root.to_path_buf(), parent, name.to_owned())
				.unwrap()
				.id
		};
		let (moving, doomed, renaming) = (
			add(here, "Moving"),
			add(here, "Doomed"),
			add(here, "Renaming"),
		);
		let two = add(here, "Two");
		let part = create_folder(root.to_path_buf(), there, "Part".to_owned(), None)
			.unwrap()
			.id();
		checkpoint(root, "Set up", "").unwrap();

		write_document(root.to_path_buf(), seeded, "Edited".to_owned()).unwrap();
		rename_document(root.to_path_buf(), renaming, "Renamed".to_owned()).unwrap();
		move_node(root.to_path_buf(), moving, part, 0).unwrap();
		delete_document(root.to_path_buf(), doomed).unwrap();
		move_node(root.to_path_buf(), two, here, 0).unwrap();
		rename_folder(root.to_path_buf(), part, "Act".to_owned()).unwrap();
		let act = create_folder(root.to_path_buf(), there, "New".to_owned(), None)
			.unwrap()
			.id();
		add(act, "Inside");
		add(here, "Added");
		let mut book = read_manifest(root).unwrap().book;
		book.title = "Odyssey".to_owned();
		write_book(root.to_path_buf(), book).unwrap();
		let image = root.parent().unwrap().join("cover.png");
		fs::write(&image, b"\x89PNG\r\n\x1a\none").unwrap();
		set_cover(root.to_path_buf(), image).unwrap();
	}

	#[test]
	fn staging_every_row_one_at_a_time_arrives_at_the_project_as_it_is() {
		let (_parent, root, seeded, here, there) = novel();
		everything_changed(&root, seeded, here, there);

		for row in rows(&status(&root).unwrap().unstaged) {
			carry(&root, row, true).unwrap();
		}
		assert!(nothing_in(&status(&root).unwrap().unstaged));
		let repo = Repository::open(&root).unwrap();
		let index = repo.index().unwrap();
		let manifest = index.get_path(Path::new(MANIFEST_FILE), 0).unwrap();
		assert_eq!(
			repo.find_blob(manifest.id).unwrap().content(),
			fs::read(root.join(MANIFEST_FILE)).unwrap()
		);

		checkpoint(&root, "All of it", "").unwrap();
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn unstaging_every_row_one_at_a_time_leaves_nothing_staged() {
		let (_parent, root, seeded, here, there) = novel();
		everything_changed(&root, seeded, here, there);
		stage_all(root.clone()).unwrap();

		for row in rows(&status(&root).unwrap().staged) {
			carry(&root, row, false).unwrap();
		}
		let status = super::status(&root).unwrap();
		assert!(nothing_in(&status.staged));
		assert_eq!(status.unstaged, changes(&root, Some("HEAD"), None).unwrap());
	}

	#[test]
	fn unstaging_all_and_checkpointing_takes_everything() {
		let (_parent, root, seeded, here, there) = novel();
		everything_changed(&root, seeded, here, there);
		carry(&root, Row::Book, true).unwrap();
		unstage_all(root.clone()).unwrap();
		assert!(nothing_in(&status(&root).unwrap().staged));

		checkpoint(&root, "All of it", "").unwrap();
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn the_first_checkpoint_can_hold_one_document() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(
			parent.path(),
			"Ithaca",
			Format::Novel,
			OffsetDateTime::UNIX_EPOCH,
		)
		.unwrap();
		ensure_repository(&root).unwrap();
		let documents = read_manifest(&root).unwrap().documents();
		let first = documents[0].id;

		carry(&root, Row::Node { id: first }, true).unwrap();
		let status = status(&root).unwrap();
		assert_eq!(change_of(&status.staged, first).unwrap().before, None);
		assert!(!status.staged.book);

		let id = checkpoint(&root, "First", "").unwrap();
		let kept = checkpoint_manifest(&root, id);
		let ids: Vec<Uuid> = kept.documents().iter().map(|d| d.id).collect();
		assert_eq!(ids, vec![first]);
		assert!(super::status(&root).unwrap().unstaged.book);
	}

	#[test]
	fn a_version_gives_back_a_documents_text_wherever_it_went() {
		let (_parent, root, seeded, _, there) = novel();
		write_document(root.clone(), seeded, "First".to_owned()).unwrap();
		let first = kept(&root);
		rename_document(root.clone(), seeded, "Renamed".to_owned()).unwrap();
		move_node(root.clone(), seeded, there, 0).unwrap();
		write_document(root.clone(), seeded, "Second".to_owned()).unwrap();
		let second = kept(&root);

		let body_of = |version: &str| {
			text(&root, version, seeded)
				.unwrap()
				.map(|t| body(&t).to_owned())
		};
		assert_eq!(body_of(&first).as_deref(), Some("First"));
		assert_eq!(body_of(&second).as_deref(), Some("Second"));
		assert_eq!(text(&root, &first, Uuid::new_v4()).unwrap(), None);
	}

	fn body_now(root: &Path, id: Uuid) -> String {
		body(&read_document(root.to_path_buf(), id).unwrap()).to_owned()
	}

	/// The last checkpoint and what the next one holds.
	fn checkpoint_and_index(root: &Path) -> (Oid, Oid) {
		let repo = Repository::open(root).unwrap();
		let index = repo.index().unwrap().write_tree().unwrap();
		(repo_head(root), index)
	}

	/// Putting back kept the changes it was about to overwrite in checkpoints
	/// of their own, one a put back, straight after the one before.
	fn kept_first(root: &Path, (head, _): (Oid, Oid)) {
		let repo = Repository::open(root).unwrap();
		let mut now = repo.head().unwrap().peel_to_commit().unwrap();
		assert_eq!(now.message().ok(), Some("Before putting back"));
		while now.message().ok() == Some("Before putting back") {
			now = now.parent(0).unwrap();
		}
		assert_eq!(now.id(), head);
	}

	#[test]
	fn a_document_comes_back_with_its_text_name_and_folder() {
		let (_parent, root, seeded, _, there) = novel();
		write_document(root.clone(), seeded, "First".to_owned()).unwrap();
		let first = kept(&root);
		rename_document(root.clone(), seeded, "Renamed".to_owned()).unwrap();
		move_node(root.clone(), seeded, there, 0).unwrap();
		write_document(root.clone(), seeded, "Second".to_owned()).unwrap();
		stage_all(root.clone()).unwrap();
		let before = checkpoint_and_index(&root);

		put_back_document(root.clone(), first.clone(), seeded).unwrap();

		assert!(changes(&root, Some(&first), None).unwrap().nodes.is_empty());
		assert_eq!(body_now(&root, seeded), "First");
		kept_first(&root, before);
	}

	#[test]
	fn a_deleted_document_returns_under_its_id_and_a_newer_one_is_trashed() {
		let (_parent, root, _, here, _) = novel();
		let add = |name: &str| {
			create_document(root.clone(), here, name.to_owned())
				.unwrap()
				.id
		};
		let (_, doomed, _) = (add("One"), add("Doomed"), add("Three"));
		write_document(root.clone(), doomed, "Kept".to_owned()).unwrap();
		let start = kept(&root);
		delete_document(root.clone(), doomed).unwrap();
		let newer = add("Newer");

		let before = checkpoint_and_index(&root);

		put_back_document(root.clone(), start.clone(), doomed).unwrap();
		put_back_document(root.clone(), start.clone(), newer).unwrap();

		// Nothing differs, so the id and the place among the others are back.
		assert!(changes(&root, Some(&start), None).unwrap().nodes.is_empty());
		assert_eq!(body_now(&root, doomed), "Kept");
		assert_eq!(list_trash(root.clone()).unwrap().len(), 2);
		kept_first(&root, before);
	}

	fn takes_on_disk(root: &Path, id: Uuid) -> PathBuf {
		root.join(TAKES_DIR).join(id.to_string())
	}

	fn take_names(root: &Path, id: Uuid) -> Vec<String> {
		list_takes(root.to_path_buf(), id)
			.unwrap()
			.into_iter()
			.map(|take| take.name)
			.collect()
	}

	#[test]
	fn putting_back_to_before_any_takes_leaves_an_ordinary_document() {
		let (_parent, root, seeded, ..) = novel();
		let then = "---\nsynopsis: Then.\n---\nFirst".to_owned();
		write_document(root.clone(), seeded, then).unwrap();
		let first = kept(&root);
		try_something(root.clone(), seeded).unwrap();
		let now = "---\nsynopsis: Now.\n---\nSecond".to_owned();
		write_document(root.clone(), seeded, now).unwrap();
		kept(&root);

		put_back_document(root.clone(), first.clone(), seeded).unwrap();

		// No row means the front matter, the keys and the files are all as then.
		assert!(changes(&root, Some(&first), None).unwrap().nodes.is_empty());
		assert!(list_takes(root.clone(), seeded).unwrap().is_empty());
		assert!(!takes_on_disk(&root, seeded).exists());
	}

	#[test]
	fn putting_back_brings_the_takes_of_that_checkpoint() {
		let (_parent, root, seeded, ..) = novel();
		let takes = try_something(root.clone(), seeded).unwrap();
		write_document(root.clone(), seeded, "Second take".to_owned()).unwrap();
		choose_take(root.clone(), seeded, takes[0].id).unwrap();
		let two = kept(&root);
		let third = try_something(root.clone(), seeded).unwrap()[2].id;
		delete_take(root.clone(), seeded, takes[1].id).unwrap();
		kept(&root);

		put_back_document(root.clone(), two.clone(), seeded).unwrap();

		assert!(changes(&root, Some(&two), None).unwrap().nodes.is_empty());
		let back = list_takes(root.clone(), seeded).unwrap();
		assert_eq!(
			back.iter().map(|take| take.id).collect::<Vec<_>>(),
			[takes[0].id, takes[1].id]
		);
		assert!(back[0].chosen);
		assert_eq!(
			take_text(root.clone(), seeded, takes[1].id).unwrap(),
			"Second take"
		);
		assert!(
			!takes_on_disk(&root, seeded)
				.join(format!("{third}.md"))
				.exists()
		);
	}

	#[test]
	fn a_deleted_document_returns_with_its_takes() {
		let (_parent, root, seeded, ..) = novel();
		try_something(root.clone(), seeded).unwrap();
		let start = kept(&root);
		delete_document(root.clone(), seeded).unwrap();
		assert!(!takes_on_disk(&root, seeded).exists());

		put_back_document(root.clone(), start.clone(), seeded).unwrap();

		assert!(changes(&root, Some(&start), None).unwrap().nodes.is_empty());
		assert_eq!(take_names(&root, seeded), ["Take 1", "Take 2"]);
	}

	#[test]
	fn put_back_on_a_changes_row_undoes_a_try_something() {
		let (_parent, root, seeded, ..) = novel();
		write_document(root.clone(), seeded, "Kept".to_owned()).unwrap();
		kept(&root);
		try_something(root.clone(), seeded).unwrap();
		write_document(root.clone(), seeded, "Rewritten".to_owned()).unwrap();
		let before = checkpoint_and_index(&root);

		put_back_document(root.clone(), "HEAD".to_owned(), seeded).unwrap();

		assert!(changes(&root, Some("HEAD"), None).unwrap().nodes.is_empty());
		assert_eq!(body_now(&root, seeded), "Kept");
		assert!(!takes_on_disk(&root, seeded).exists());
		assert_eq!(checkpoint_and_index(&root), before);
	}

	#[test]
	fn putting_back_keeps_unkept_changes_in_a_checkpoint_first() {
		let put_backs: [fn(PathBuf, String, Uuid) -> Result<()>; 3] = [
			put_back_document,
			|root, version, _| put_back_book(root, version),
			|root, version, _| put_back_everything(root, version),
		];
		for put_back in put_backs {
			let (_parent, root, seeded, ..) = novel();
			let first = repo_head(&root).to_string();
			write_document(root.clone(), seeded, "Not yet kept".to_owned()).unwrap();

			put_back(root.clone(), first, seeded).unwrap();

			let repo = Repository::open(&root).unwrap();
			let head = repo.head().unwrap().peel_to_commit().unwrap();
			assert_eq!(head.message().unwrap(), "Before putting back");
			let then = text(&root, &head.id().to_string(), seeded)
				.unwrap()
				.unwrap();
			assert_eq!(body(&then), "Not yet kept");
		}
	}

	#[test]
	fn put_back_removes_names_the_takes_that_would_go() {
		let (_parent, root, seeded, ..) = novel();
		let first = kept(&root);
		try_something(root.clone(), seeded).unwrap();
		let two = kept(&root);
		try_something(root.clone(), seeded).unwrap();
		let removes =
			|version: &str| put_back_removes(root.clone(), version.to_owned(), seeded).unwrap();

		assert_eq!(removes(&first), ["Take 1", "Take 2", "Take 3"]);
		assert_eq!(removes(&two), ["Take 3"]);
		assert_eq!(removes("HEAD"), ["Take 3"]);

		// Going to the Trash takes the takes with it, so nothing is lost.
		let manifest = read_manifest(&root).unwrap();
		let folder = tree::parent(&manifest.nodes, seeded).unwrap().id();
		let newer = create_document(root.clone(), folder, "Newer".to_owned())
			.unwrap()
			.id;
		try_something(root.clone(), newer).unwrap();
		assert!(
			put_back_removes(root.clone(), first.clone(), newer)
				.unwrap()
				.is_empty()
		);
	}

	#[test]
	fn put_back_everything_removes_names_the_takes_across_the_project() {
		let (_parent, root, seeded, ..) = novel();
		let first = kept(&root);
		try_something(root.clone(), seeded).unwrap();
		let two = kept(&root);

		let manifest = read_manifest(&root).unwrap();
		let name = manifest
			.documents()
			.into_iter()
			.find(|document| document.id == seeded)
			.unwrap()
			.title();
		let folder = tree::parent(&manifest.nodes, seeded).unwrap().id();
		let newer = create_document(root.clone(), folder, "Newer".to_owned())
			.unwrap()
			.id;
		try_something(root.clone(), newer).unwrap();
		let removes =
			|version: &str| put_back_everything_removes(root.clone(), version.to_owned()).unwrap();

		// The document the version lacks counts, because it does not go to the Trash.
		assert_eq!(removes(&two), ["Take 1 of Newer", "Take 2 of Newer"]);

		let all = removes(&first);
		assert_eq!(all.len(), 4);
		assert!(all.contains(&format!("Take 1 of {name}")));
		assert!(all.contains(&format!("Take 2 of {name}")));
	}

	#[test]
	fn put_back_everything_takes_take_files_away() {
		let (_parent, root, seeded, ..) = novel();
		let first = kept(&root);
		try_something(root.clone(), seeded).unwrap();
		let two = kept(&root);
		try_something(root.clone(), seeded).unwrap();

		put_back_everything(root.clone(), two.clone()).unwrap();
		assert!(changes(&root, Some(&two), None).unwrap().nodes.is_empty());
		assert_eq!(take_names(&root, seeded), ["Take 1", "Take 2"]);
		assert_eq!(
			fs::read_dir(takes_on_disk(&root, seeded)).unwrap().count(),
			2
		);

		put_back_everything(root.clone(), first.clone()).unwrap();
		assert!(changes(&root, Some(&first), None).unwrap().nodes.is_empty());
		assert!(!takes_on_disk(&root, seeded).exists());
	}

	#[test]
	fn a_version_opens_as_a_take_and_the_text_it_replaced_is_kept() {
		let (_parent, root, seeded, ..) = novel();
		write_document(root.clone(), seeded, "Old words".to_owned()).unwrap();
		let first = kept(&root);
		write_document(root.clone(), seeded, "New words".to_owned()).unwrap();

		let takes = open_as_take(root.clone(), first, seeded).unwrap();

		assert_eq!(take_names(&root, seeded), ["Take 1", "Take 2"]);
		assert!(takes[1].chosen);
		assert_eq!(takes[1].from, Some(takes[0].id));
		assert_eq!(body_now(&root, seeded), "Old words");
		assert_eq!(
			take_text(root.clone(), seeded, takes[0].id).unwrap(),
			"New words"
		);
	}

	#[test]
	fn a_version_opens_beside_the_takes_a_document_has() {
		let (_parent, root, seeded, ..) = novel();
		write_document(root.clone(), seeded, "Old words".to_owned()).unwrap();
		let first = kept(&root);
		let was = try_something(root.clone(), seeded).unwrap()[1].id;
		write_document(root.clone(), seeded, "Rewritten".to_owned()).unwrap();

		let takes = open_as_take(root.clone(), first, seeded).unwrap();

		assert_eq!(take_names(&root, seeded), ["Take 1", "Take 2", "Take 3"]);
		assert!(takes[2].chosen);
		assert_eq!(takes[2].from, Some(was));
		assert_eq!(body_now(&root, seeded), "Old words");
		assert_eq!(take_text(root.clone(), seeded, was).unwrap(), "Rewritten");
	}

	#[test]
	fn a_renamed_document_opens_from_a_version_by_its_id() {
		let (_parent, root, seeded, ..) = novel();
		write_document(root.clone(), seeded, "Old words".to_owned()).unwrap();
		let first = kept(&root);
		rename_document(root.clone(), seeded, "Renamed".to_owned()).unwrap();

		open_as_take(root.clone(), first, seeded).unwrap();

		assert_eq!(body_now(&root, seeded), "Old words");
	}

	#[test]
	fn opening_a_version_as_a_take_keeps_the_front_matter_of_now() {
		let (_parent, root, seeded, ..) = novel();
		let then = "---\nsynopsis: Then.\n---\nOld words".to_owned();
		write_document(root.clone(), seeded, then).unwrap();
		let first = kept(&root);
		let now = "---\nsynopsis: Now.\n---\nNew words".to_owned();
		write_document(root.clone(), seeded, now).unwrap();

		open_as_take(root.clone(), first, seeded).unwrap();

		assert_eq!(
			read_document(root.clone(), seeded).unwrap(),
			"---\nsynopsis: Now.\n---\nOld words"
		);
	}

	#[test]
	fn a_version_without_the_document_cannot_open_as_a_take() {
		let (_parent, root, seeded, ..) = novel();
		let manifest = read_manifest(&root).unwrap();
		let folder = tree::parent(&manifest.nodes, seeded).unwrap().id();
		let first = kept(&root);
		let newer = create_document(root.clone(), folder, "Newer".to_owned())
			.unwrap()
			.id;

		assert!(matches!(
			open_as_take(root.clone(), first, newer),
			Err(Error::UnknownDocument)
		));
		assert!(list_takes(root.clone(), newer).unwrap().is_empty());
	}

	#[test]
	fn the_book_and_its_cover_come_back() {
		let (_parent, root, ..) = novel();
		let png = root.parent().unwrap().join("cover.png");
		fs::write(&png, b"\x89PNG\r\n\x1a\none").unwrap();
		set_cover(root.clone(), png).unwrap();
		let start = kept(&root);
		let mut book = read_manifest(&root).unwrap().book;
		book.title = "Odyssey".to_owned();
		write_book(root.clone(), book).unwrap();
		let jpeg = root.parent().unwrap().join("cover.jpg");
		fs::write(&jpeg, b"\xff\xd8\xff").unwrap();
		set_cover(root.clone(), jpeg).unwrap();

		stage_all(root.clone()).unwrap();
		let before = checkpoint_and_index(&root);

		put_back_book(root.clone(), start.clone()).unwrap();

		let changes = changes(&root, Some(&start), None).unwrap();
		assert!(!changes.book && !changes.cover);
		assert!(root.join("cover.png").is_file());
		assert!(!root.join("cover.jpg").exists());
		kept_first(&root, before);
	}

	#[test]
	fn everything_comes_back_but_stats_and_the_trash() {
		let (_parent, root, seeded, here, there) = novel();
		let folder = |parent: Uuid, name: &str| {
			create_folder(root.clone(), parent, name.to_owned(), None)
				.unwrap()
				.id()
		};
		let path_of = |id: Uuid| tree::path(&read_manifest(&root).unwrap().nodes, id).unwrap();
		let empty = folder(here, "Empty");
		let empty_path = path_of(empty);
		write_document(root.clone(), seeded, "First".to_owned()).unwrap();
		let start = kept(&root);

		write_document(root.clone(), seeded, "Second".to_owned()).unwrap();
		rename_document(root.clone(), seeded, "Renamed".to_owned()).unwrap();
		delete_folder(root.clone(), empty).unwrap();
		let staged = folder(there, "Staged");
		create_document(root.clone(), staged, "Inside".to_owned()).unwrap();
		let staged_path = path_of(staged);
		stage_all(root.clone()).unwrap();
		// Added after staging, so git has never seen it.
		let later = folder(there, "Later");
		create_document(root.clone(), later, "Inside".to_owned()).unwrap();
		let later_path = path_of(later);
		fs::write(root.join(HISTORY_FILE), "[]").unwrap();
		let before = checkpoint_and_index(&root);

		put_back_everything(root.clone(), start.clone()).unwrap();

		assert!(changes(&root, Some(&start), None).unwrap().nodes.is_empty());
		assert_eq!(body_now(&root, seeded), "First");
		assert!(root.join(&empty_path).is_dir());
		assert!(!root.join(&staged_path).exists());
		assert!(!root.join(&later_path).exists());
		assert_eq!(fs::read_to_string(root.join(HISTORY_FILE)).unwrap(), "[]");
		assert_eq!(list_trash(root.clone()).unwrap().len(), 1);
		kept_first(&root, before);
	}

	/// Keeps the project, does `there` in a new version "Act two" and `here`
	/// on Main, each kept, then merges Act two into Main.
	fn diverged(root: &Path, there: impl FnOnce(), here: impl FnOnce()) -> Vec<merge::Conflict> {
		held(root, "Base");
		new_branch(root, "Act two").unwrap();
		there();
		held(root, "In Act two");
		switch(root, "main").unwrap();
		here();
		held(root, "On Main");
		let conflicts = merge(root, "act-two", "")
			.unwrap()
			.map_or_else(Vec::new, |merging| merging.conflicts);
		assert_eq!(
			merging(root)
				.unwrap()
				.map_or_else(Vec::new, |m| m.conflicts),
			conflicts
		);
		conflicts
	}

	/// The folder and name the project on disk gives a node.
	fn placed(root: &Path, id: Uuid) -> (Option<Uuid>, String) {
		let nodes = read_manifest(root).unwrap().nodes;
		let name = tree::find(&nodes, id).unwrap().name().to_owned();
		(tree::parent(&nodes, id).map(Node::id), name)
	}

	fn children_of(root: &Path, folder: Uuid) -> Vec<Uuid> {
		let nodes = read_manifest(root).unwrap().nodes;
		tree::children(&nodes, folder)
			.unwrap()
			.iter()
			.map(Node::id)
			.collect()
	}

	/// Settling is the merge screen's business, so until then a checkpoint
	/// is refused and the Changes list still reads.
	fn waits(root: &Path) {
		assert_eq!(merge_state_of(root), RepositoryState::Merge);
		assert!(status(root).is_ok());
		let refused = checkpoint(root, "Merged Act two", "");
		assert!(matches!(refused, Err(Error::ConflictsRemain)));
	}

	#[test]
	fn a_scene_one_side_moved_keeps_the_others_edit() {
		let (_parent, root, seeded, _, there) = novel();
		let conflicts = diverged(
			&root,
			|| move_node(root.clone(), seeded, there, 0).unwrap(),
			|| write_document(root.clone(), seeded, "Edited on Main".to_owned()).unwrap(),
		);

		assert_eq!(conflicts, []);
		assert_eq!(placed(&root, seeded).0, Some(there));
		assert_eq!(body_now(&root, seeded), "Edited on Main");
		assert_eq!(parents(&root).len(), 2);
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn empty_scenes_moved_and_renamed_keep_their_text() {
		let (_parent, root, _, here, there) = novel();
		let add = |name: &str| {
			create_document(root.clone(), here, name.to_owned())
				.unwrap()
				.id
		};
		let (a, b) = (add("A"), add("B"));
		let conflicts = diverged(
			&root,
			|| {
				move_node(root.clone(), b, there, 0).unwrap();
				move_node(root.clone(), a, there, 0).unwrap();
				rename_document(root.clone(), a, "Z".to_owned()).unwrap();
			},
			|| write_document(root.clone(), a, "Hello".to_owned()).unwrap(),
		);

		assert_eq!(conflicts, []);
		assert_eq!(placed(&root, a), (Some(there), "Z.md".to_owned()));
		assert_eq!(read_document(root.clone(), a).unwrap(), "Hello");
		assert_eq!(read_document(root.clone(), b).unwrap(), "");
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn two_new_documents_with_one_name_are_both_kept() {
		let (_parent, root, _, here, _) = novel();
		let add = || {
			create_document(root.clone(), here, "New".to_owned()).unwrap();
		};
		assert_eq!(diverged(&root, add, add), []);

		let names: Vec<String> = children_of(&root, here)
			.into_iter()
			.map(|id| placed(&root, id).1)
			.collect();
		assert!(names.contains(&"New.md".to_owned()));
		assert!(names.contains(&"New 2.md".to_owned()));
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn renamed_on_both_keeps_this_versions_name() {
		let (_parent, root, seeded, ..) = novel();
		let conflicts = diverged(
			&root,
			|| {
				rename_document(root.clone(), seeded, "There".to_owned()).unwrap();
			},
			|| {
				rename_document(root.clone(), seeded, "Here".to_owned()).unwrap();
			},
		);

		assert_eq!(
			conflicts,
			[merge::Conflict::Renamed {
				id: seeded,
				ours: "Here.md".to_owned(),
				theirs: "There.md".to_owned(),
			}]
		);
		assert_eq!(placed(&root, seeded).1, "Here.md");
		waits(&root);
	}

	#[test]
	fn moved_on_both_keeps_this_versions_folder() {
		let (_parent, root, seeded, here, there) = novel();
		let conflicts = diverged(
			&root,
			|| move_node(root.clone(), seeded, there, 0).unwrap(),
			|| move_node(root.clone(), seeded, here, 0).unwrap(),
		);

		assert_eq!(
			conflicts,
			[merge::Conflict::Moved {
				id: seeded,
				ours: Some(here),
				theirs: Some(there),
			}]
		);
		assert_eq!(placed(&root, seeded).0, Some(here));
		waits(&root);
	}

	#[test]
	fn deleted_on_one_side_and_edited_on_the_other() {
		let (_parent, root, seeded, here, _) = novel();
		let other = create_document(root.clone(), here, "Other".to_owned())
			.unwrap()
			.id;
		let conflicts = diverged(
			&root,
			|| {
				delete_document(root.clone(), seeded).unwrap();
				write_document(root.clone(), other, "Theirs".to_owned()).unwrap();
			},
			|| {
				write_document(root.clone(), seeded, "Ours".to_owned()).unwrap();
				delete_document(root.clone(), other).unwrap();
			},
		);

		assert_eq!(
			conflicts,
			[
				merge::Conflict::Deleted {
					id: seeded,
					here: false,
					name: placed(&root, seeded).1,
					documents: 1,
				},
				merge::Conflict::Deleted {
					id: other,
					here: true,
					name: "Other.md".to_owned(),
					documents: 1,
				},
			]
		);
		assert_eq!(body_now(&root, seeded), "Ours");
		assert!(tree::find(&read_manifest(&root).unwrap().nodes, other).is_none());
		waits(&root);
	}

	/// A folder holding three new documents, in order.
	fn three_in(root: &Path, under: Uuid) -> (Uuid, [Uuid; 3]) {
		let folder = create_folder(root.to_path_buf(), under, "Scenes".to_owned(), None)
			.unwrap()
			.id();
		let add = |name: &str| {
			create_document(root.to_path_buf(), folder, name.to_owned())
				.unwrap()
				.id
		};
		(folder, [add("A"), add("B"), add("C")])
	}

	#[test]
	fn reordered_on_both_keeps_this_versions_order() {
		let (_parent, root, _, here, _) = novel();
		let (folder, [a, b, c]) = three_in(&root, here);
		let conflicts = diverged(
			&root,
			|| move_node(root.clone(), c, folder, 0).unwrap(),
			|| move_node(root.clone(), a, folder, 2).unwrap(),
		);

		assert_eq!(
			conflicts,
			[merge::Conflict::Reordered {
				id: Some(folder),
				ours: vec![b, c, a],
				theirs: vec![c, a, b],
			}]
		);
		assert_eq!(children_of(&root, folder), [b, c, a]);
		waits(&root);
	}

	#[test]
	fn one_sides_order_takes_the_others_new_document() {
		let (_parent, root, _, here, _) = novel();
		let (folder, [a, b, c]) = three_in(&root, here);
		let added = std::cell::Cell::new(None);
		let conflicts = diverged(
			&root,
			|| move_node(root.clone(), c, folder, 0).unwrap(),
			|| {
				let id = create_document(root.clone(), folder, "D".to_owned())
					.unwrap()
					.id;
				added.set(Some(id));
			},
		);

		// A new document follows the one it was added after.
		assert_eq!(conflicts, []);
		assert_eq!(children_of(&root, folder), [c, added.get().unwrap(), a, b]);
	}

	#[test]
	fn the_book_and_its_cover_changed_on_both() {
		let (_parent, root, ..) = novel();
		let change = |title: &str, image: &[u8]| {
			let png = root.parent().unwrap().join("cover.png");
			fs::write(&png, image).unwrap();
			set_cover(root.clone(), png).unwrap();
			let mut book = read_manifest(&root).unwrap().book;
			book.title = title.to_owned();
			write_book(root.clone(), book).unwrap();
		};
		let conflicts = diverged(
			&root,
			|| change("Theirs", b"\x89PNG\r\n\x1a\ntheirs"),
			|| change("Ours", b"\x89PNG\r\n\x1a\nours"),
		);

		assert_eq!(
			conflicts,
			[
				merge::Conflict::Field {
					id: None,
					field: "title".to_owned(),
					ours: "Ours".into(),
					theirs: "Theirs".into(),
				},
				merge::Conflict::Cover {
					ours: Some("cover.png".to_owned()),
					theirs: Some("cover.png".to_owned()),
				},
			]
		);
		assert_eq!(read_manifest(&root).unwrap().book.title, "Ours");
		assert!(fs::read(root.join("cover.png")).unwrap().ends_with(b"ours"));
		waits(&root);
	}

	/// A merge left waiting on the book's title, changed on both versions.
	fn titles_conflict(root: &Path) -> Vec<merge::Conflict> {
		let retitle = |title: &str| {
			let mut book = read_manifest(root).unwrap().book;
			book.title = title.to_owned();
			write_book(root.to_owned(), book).unwrap();
		};
		let conflicts = diverged(root, || retitle("Theirs"), || retitle("Ours"));
		assert_eq!(conflicts.len(), 1);
		conflicts
	}

	#[test]
	fn settling_the_binder_undoes_book_details_changed_meanwhile() {
		let (_parent, root, ..) = novel();
		titles_conflict(&root);
		let mut book = read_manifest(&root).unwrap().book;
		let merged = book.blurb.clone();
		book.blurb = "Written during the merge".to_owned();
		write_book(root.clone(), book).unwrap();

		settle_rest(&root, &[]).unwrap();

		let book = read_manifest(&root).unwrap().book;
		assert_eq!(book.blurb, merged);
		assert_eq!(book.title, "Ours");
	}

	#[test]
	fn settling_the_binder_keeps_a_document_created_meanwhile() {
		let (_parent, root, _, here, _) = novel();
		titles_conflict(&root);
		assert!(matches!(
			create_document(root.clone(), here, "Meanwhile".to_owned()),
			Err(Error::MergeUnderway)
		));
		// A file added outside Aurora is adopted all the same.
		let folder = tree::path(&read_manifest(&root).unwrap().nodes, here).unwrap();
		fs::write(root.join(&folder).join("Meanwhile.md"), "").unwrap();
		crate::document::refresh_documents(root.clone()).unwrap();
		let manifest = read_manifest(&root).unwrap();
		let path = format!("{folder}/Meanwhile.md");
		let added = manifest
			.documents()
			.iter()
			.find(|document| document.path == path)
			.unwrap()
			.id;
		let file = crate::document::document_path(&manifest, &root, &path).unwrap();
		assert!(file.exists());

		settle_rest(&root, &[]).unwrap();

		let manifest = read_manifest(&root).unwrap();
		assert_eq!(tree::path(&manifest.nodes, added), Some(path));
		assert!(file.exists());
	}

	#[test]
	fn a_documents_target_changed_on_both() {
		let (_parent, root, seeded, ..) = novel();
		let target = |words: u32| {
			let mut manifest = read_manifest(&root).unwrap();
			if let Some(Node::Document { target, .. }) = tree::find_mut(&mut manifest.nodes, seeded)
			{
				*target = Some(words);
			}
			write_manifest(&root, &mut manifest).unwrap();
		};
		let conflicts = diverged(&root, || target(100), || target(200));

		assert_eq!(
			conflicts,
			[merge::Conflict::Field {
				id: Some(seeded),
				field: "target".to_owned(),
				ours: 200.into(),
				theirs: 100.into(),
			}]
		);
		waits(&root);
	}

	#[test]
	fn the_changes_list_reads_while_a_paragraph_conflicts() {
		let (_parent, root, seeded, other, ..) = conflicted();

		let status = status(&root).unwrap();

		assert!(status.unstaged.nodes.is_empty());
		let edited: Vec<Uuid> = status
			.staged
			.nodes
			.iter()
			.filter(|change| change.edited)
			.map(|change| change.id)
			.collect();
		assert_eq!(edited, [other]);
		assert!(!edited.contains(&seeded));
	}

	/// Settles the binder with every conflict the other version's way or
	/// none of them, then checks the merge finishes with the files matching.
	fn settled_as(root: &Path, conflicts: Vec<merge::Conflict>, theirs: bool) {
		let chosen = if theirs { conflicts } else { Vec::new() };
		settle_rest(root, &chosen).unwrap();
		assert_eq!(merging(root).unwrap().unwrap().conflicts, []);
		checkpoint(root, "Merged Act two", "").unwrap();
		assert_eq!(parents(root).len(), 2);
		assert!(!pending(root).unwrap());
	}

	/// Takes `One`, `Two` and `Three`, the last chosen.
	fn written_takes(root: &Path, id: Uuid) -> Vec<TakeView> {
		write_document(root.to_path_buf(), id, "One\n".to_owned()).unwrap();
		try_something(root.to_path_buf(), id).unwrap();
		write_document(root.to_path_buf(), id, "Two\n".to_owned()).unwrap();
		try_something(root.to_path_buf(), id).unwrap();
		write_document(root.to_path_buf(), id, "Three\n".to_owned()).unwrap();
		list_takes(root.to_path_buf(), id).unwrap()
	}

	fn chosen_take(root: &Path, id: Uuid) -> Uuid {
		let takes = list_takes(root.to_path_buf(), id).unwrap();
		takes.iter().find(|take| take.chosen).unwrap().id
	}

	/// Every take has its file, one is chosen, and nothing else is in `.takes`.
	fn takes_whole(root: &Path) {
		let manifest = read_manifest(root).unwrap();
		let mut folders = 0;
		for node in tree::walk(&manifest.nodes) {
			let Node::Document {
				id, takes, chosen, ..
			} = node
			else {
				continue;
			};
			if takes.is_empty() {
				assert!(chosen.is_none());
				assert!(!takes_on_disk(root, *id).exists());
				continue;
			}
			assert!(takes.len() > 1);
			assert!(takes.contains_key(&chosen.unwrap()));
			folders += 1;
			let mut files: Vec<String> = fs::read_dir(takes_on_disk(root, *id))
				.unwrap()
				.map(|file| file.unwrap().file_name().into_string().unwrap())
				.collect();
			let mut named: Vec<String> = takes.keys().map(|take| format!("{take}.md")).collect();
			files.sort();
			named.sort();
			assert_eq!(files, named);
		}
		let on_disk = fs::read_dir(root.join(TAKES_DIR)).map_or(0, |dir| dir.count());
		assert_eq!(on_disk, folders);
	}

	#[test]
	fn takes_one_version_made_come_across() {
		let (_parent, root, seeded, here, _) = novel();
		let conflicts = diverged(
			&root,
			|| {
				try_something(root.clone(), seeded).unwrap();
				write_document(root.clone(), seeded, "Rewritten".to_owned()).unwrap();
			},
			|| {
				create_document(root.clone(), here, "New".to_owned()).unwrap();
			},
		);

		assert_eq!(conflicts, []);
		assert_eq!(take_names(&root, seeded), ["Take 1", "Take 2"]);
		assert_eq!(read_document(root.clone(), seeded).unwrap(), "Rewritten");
		takes_whole(&root);
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn try_something_on_both_sides_keeps_all_of_them() {
		let (_parent, root, seeded, ..) = novel();
		let mut mine = None;
		let conflicts = diverged(
			&root,
			|| {
				try_something(root.clone(), seeded).unwrap();
			},
			|| {
				try_something(root.clone(), seeded).unwrap();
				mine = Some(chosen_take(&root, seeded));
			},
		);

		assert_eq!(conflicts, []);
		assert_eq!(
			take_names(&root, seeded),
			["Take 1", "Take 1", "Take 2", "Take 2"]
		);
		assert_eq!(Some(chosen_take(&root, seeded)), mine);
		takes_whole(&root);
	}

	#[test]
	fn each_side_choosing_a_different_take_finishes_with_every_file() {
		let (_parent, root, seeded, ..) = novel();
		let takes = written_takes(&root, seeded);
		let conflicts = diverged(
			&root,
			|| {
				choose_take(root.clone(), seeded, takes[0].id).unwrap();
			},
			|| {
				choose_take(root.clone(), seeded, takes[1].id).unwrap();
			},
		);

		assert!(matches!(
			conflicts.as_slice(),
			[merge::Conflict::Text { id: Some(id), .. }] if *id == seeded
		));
		assert_eq!(chosen_take(&root, seeded), takes[1].id);
		settle(&root, seeded, "Two\n").unwrap();
		checkpoint(&root, "Merged Act two", "").unwrap();

		assert_eq!(parents(&root).len(), 2);
		assert_eq!(
			take_text(root.clone(), seeded, takes[2].id).unwrap(),
			"Three\n"
		);
		takes_whole(&root);
	}

	#[test]
	fn a_take_deleted_on_one_side_while_chosen_on_the_other_falls_to_the_first() {
		for deleted_here in [false, true] {
			let (_parent, root, seeded, ..) = novel();
			let takes = written_takes(&root, seeded);
			let gone = takes[0].id;
			let delete = || {
				delete_take(root.clone(), seeded, gone).unwrap();
			};
			let choose = || {
				choose_take(root.clone(), seeded, gone).unwrap();
			};
			let conflicts = match deleted_here {
				true => diverged(&root, choose, delete),
				false => diverged(&root, delete, choose),
			};

			assert_eq!(conflicts, []);
			assert_eq!(take_names(&root, seeded), ["Take 2", "Take 3"]);
			assert_eq!(chosen_take(&root, seeded), takes[1].id);
			assert_eq!(read_document(root.clone(), seeded).unwrap(), "One\n");
			takes_whole(&root);
		}
	}

	#[test]
	fn a_clash_over_a_waiting_take_keeps_this_version_and_raises_nothing() {
		let (_parent, root, seeded, ..) = novel();
		write_document(root.clone(), seeded, "One\n".to_owned()).unwrap();
		let takes = try_something(root.clone(), seeded).unwrap();
		write_document(root.clone(), seeded, "Two\n".to_owned()).unwrap();
		let (waiting, chosen) = (takes[0].id, takes[1].id);
		let rewrite = |text: &str| {
			choose_take(root.clone(), seeded, waiting).unwrap();
			write_document(root.clone(), seeded, text.to_owned()).unwrap();
			choose_take(root.clone(), seeded, chosen).unwrap();
		};

		let conflicts = diverged(&root, || rewrite("There\n"), || rewrite("Here\n"));

		assert_eq!(conflicts, []);
		assert_eq!(parents(&root).len(), 2);
		assert_eq!(take_text(root.clone(), seeded, waiting).unwrap(), "Here\n");
		assert!(!pending(&root).unwrap());
		takes_whole(&root);
	}

	#[test]
	fn a_take_renamed_here_and_deleted_there_keeps_its_file() {
		let (_parent, root, seeded, ..) = novel();
		let takes = written_takes(&root, seeded);
		let kept = takes[0].id;
		let conflicts = diverged(
			&root,
			|| {
				delete_take(root.clone(), seeded, kept).unwrap();
			},
			|| {
				rename_take(root.clone(), seeded, kept, "Opening".to_owned()).unwrap();
			},
		);

		assert_eq!(conflicts, []);
		assert_eq!(take_names(&root, seeded), ["Opening", "Take 2", "Take 3"]);
		assert_eq!(take_text(root.clone(), seeded, kept).unwrap(), "One\n");
		assert!(!pending(&root).unwrap());
		takes_whole(&root);
	}

	#[test]
	fn a_document_left_with_takes_and_none_chosen_chooses_the_first() {
		let (_parent, root, seeded, ..) = novel();
		write_document(root.clone(), seeded, "One\n".to_owned()).unwrap();
		let takes = try_something(root.clone(), seeded).unwrap();
		write_document(root.clone(), seeded, "Two\n".to_owned()).unwrap();
		let conflicts = diverged(
			&root,
			|| {
				try_something(root.clone(), seeded).unwrap();
				try_something(root.clone(), seeded).unwrap();
				choose_take(root.clone(), seeded, takes[1].id).unwrap();
			},
			|| {
				delete_take(root.clone(), seeded, takes[1].id).unwrap();
			},
		);

		assert_eq!(conflicts, []);
		assert_eq!(take_names(&root, seeded), ["Take 3", "Take 4"]);
		takes_whole(&root);
		assert_eq!(read_document(root.clone(), seeded).unwrap(), "One\n");

		let left = list_takes(root.clone(), seeded).unwrap();
		choose_take(root.clone(), seeded, left[1].id).unwrap();
		assert_eq!(
			take_text(root.clone(), seeded, left[0].id).unwrap(),
			"One\n",
			"the document's text went into the take it stood for"
		);
	}

	#[test]
	fn a_document_merged_down_to_the_other_versions_new_take_keeps_it_beside_its_text() {
		let (_parent, root, seeded, ..) = novel();
		write_document(root.clone(), seeded, "One\n".to_owned()).unwrap();
		let takes = try_something(root.clone(), seeded).unwrap();
		write_document(root.clone(), seeded, "Two\n".to_owned()).unwrap();
		let conflicts = diverged(
			&root,
			|| {
				try_something(root.clone(), seeded).unwrap();
			},
			|| {
				delete_take(root.clone(), seeded, takes[1].id).unwrap();
			},
		);

		assert_eq!(conflicts, []);
		assert_eq!(take_names(&root, seeded), ["Take 3", "Take 4"]);
		takes_whole(&root);
		assert!(!pending(&root).unwrap());
	}

	#[test]
	fn a_document_deleted_on_one_side_settles_with_its_takes_either_way() {
		for deleted_here in [false, true] {
			for theirs in [false, true] {
				let (_parent, root, seeded, ..) = novel();
				try_something(root.clone(), seeded).unwrap();
				let delete = || delete_document(root.clone(), seeded).unwrap();
				let edit = || write_document(root.clone(), seeded, "Edited".to_owned()).unwrap();
				let conflicts = match deleted_here {
					true => diverged(&root, edit, delete),
					false => diverged(&root, delete, edit),
				};

				settled_as(&root, conflicts, theirs);

				let survives = deleted_here == theirs;
				let manifest = read_manifest(&root).unwrap();
				assert_eq!(tree::find(&manifest.nodes, seeded).is_some(), survives);
				if survives {
					assert_eq!(take_names(&root, seeded), ["Take 1", "Take 2"]);
				}
				takes_whole(&root);
			}
		}
	}

	#[test]
	fn a_rename_on_both_settles_either_way() {
		for theirs in [false, true] {
			let (_parent, root, seeded, ..) = novel();
			write_document(root.clone(), seeded, "Kept".to_owned()).unwrap();
			let conflicts = diverged(
				&root,
				|| {
					rename_document(root.clone(), seeded, "There".to_owned()).unwrap();
				},
				|| {
					rename_document(root.clone(), seeded, "Here".to_owned()).unwrap();
				},
			);

			settled_as(&root, conflicts, theirs);
			let name = if theirs { "There.md" } else { "Here.md" };
			assert_eq!(placed(&root, seeded).1, name);
			assert_eq!(body_now(&root, seeded), "Kept");
		}
	}

	#[test]
	fn a_move_on_both_settles_either_way() {
		for theirs in [false, true] {
			let (_parent, root, seeded, here, there) = novel();
			let conflicts = diverged(
				&root,
				|| move_node(root.clone(), seeded, there, 0).unwrap(),
				|| move_node(root.clone(), seeded, here, 0).unwrap(),
			);

			settled_as(&root, conflicts, theirs);
			let folder = if theirs { there } else { here };
			assert_eq!(placed(&root, seeded).0, Some(folder));
		}
	}

	#[test]
	fn a_move_that_closes_a_cycle_settles_either_way() {
		for theirs in [false, true] {
			let (_parent, root, _, here, _) = novel();
			let folder = |name: &str| {
				create_folder(root.clone(), here, name.to_owned(), None)
					.unwrap()
					.id()
			};
			let (f, g) = (folder("F"), folder("G"));
			let inside = create_document(root.clone(), f, "Inside".to_owned())
				.unwrap()
				.id;
			let conflicts = diverged(
				&root,
				|| move_node(root.clone(), g, f, 0).unwrap(),
				|| move_node(root.clone(), f, g, 0).unwrap(),
			);
			assert_eq!(
				conflicts,
				[merge::Conflict::Moved {
					id: g,
					ours: Some(here),
					theirs: Some(f),
				}]
			);

			settled_as(&root, conflicts, theirs);
			let (outer, inner) = if theirs { (f, g) } else { (g, f) };
			assert_eq!(placed(&root, outer).0, Some(here));
			assert_eq!(placed(&root, inner).0, Some(outer));
			assert_eq!(placed(&root, inside).0, Some(f));
		}
	}

	#[test]
	fn a_deletion_against_an_edit_settles_either_way() {
		for theirs in [false, true] {
			let (_parent, root, seeded, here, _) = novel();
			let other = create_document(root.clone(), here, "Other".to_owned())
				.unwrap()
				.id;
			let conflicts = diverged(
				&root,
				|| {
					delete_document(root.clone(), seeded).unwrap();
					write_document(root.clone(), other, "Theirs".to_owned()).unwrap();
				},
				|| {
					write_document(root.clone(), seeded, "Ours".to_owned()).unwrap();
					delete_document(root.clone(), other).unwrap();
				},
			);

			settled_as(&root, conflicts, theirs);
			let nodes = read_manifest(&root).unwrap().nodes;
			assert_eq!(tree::find(&nodes, seeded).is_some(), !theirs);
			assert_eq!(tree::find(&nodes, other).is_some(), theirs);
			match theirs {
				true => assert_eq!(body_now(&root, other), "Theirs"),
				false => assert_eq!(body_now(&root, seeded), "Ours"),
			}
		}
	}

	#[test]
	fn a_deleted_folder_goes_or_comes_back_whole() {
		for here_deleted in [false, true] {
			for theirs in [false, true] {
				let (_parent, root, _, here, _) = novel();
				let (folder, [a, b, c]) = three_in(&root, here);
				let delete = || delete_folder(root.clone(), folder).unwrap();
				let edit = || write_document(root.clone(), a, "Edited".to_owned()).unwrap();
				let conflicts = match here_deleted {
					true => diverged(&root, edit, delete),
					false => diverged(&root, delete, edit),
				};
				assert_eq!(
					conflicts,
					[merge::Conflict::Deleted {
						id: folder,
						here: here_deleted,
						name: "Scenes".to_owned(),
						documents: 3,
					}]
				);

				settled_as(&root, conflicts, theirs);
				// Taking the other side undoes this version's deletion, or
				// takes the other's.
				let kept = here_deleted == theirs;
				let nodes = read_manifest(&root).unwrap().nodes;
				for id in [folder, a, b, c] {
					assert_eq!(tree::find(&nodes, id).is_some(), kept);
				}
				if kept {
					assert_eq!(body_now(&root, a), "Edited");
				}
			}
		}
	}

	#[test]
	fn a_reorder_on_both_settles_either_way() {
		for theirs in [false, true] {
			let (_parent, root, _, here, _) = novel();
			let (folder, [a, b, c]) = three_in(&root, here);
			let conflicts = diverged(
				&root,
				|| move_node(root.clone(), c, folder, 0).unwrap(),
				|| move_node(root.clone(), a, folder, 2).unwrap(),
			);

			settled_as(&root, conflicts, theirs);
			let order = if theirs { [c, a, b] } else { [b, c, a] };
			assert_eq!(children_of(&root, folder), order);
		}
	}

	#[test]
	fn a_field_on_both_settles_either_way_whatever_the_number_looks_like() {
		for theirs in [false, true] {
			let (_parent, root, seeded, ..) = novel();
			let target = |words: u32| {
				let mut manifest = read_manifest(&root).unwrap();
				if let Some(Node::Document { target, .. }) =
					tree::find_mut(&mut manifest.nodes, seeded)
				{
					*target = Some(words);
				}
				write_manifest(&root, &mut manifest).unwrap();
			};
			let mut conflicts = diverged(&root, || target(100), || target(200));
			// The frontend can send a number back as a float.
			if let [merge::Conflict::Field { theirs, .. }] = conflicts.as_mut_slice() {
				*theirs = serde_json::json!(100.0);
			}

			settled_as(&root, conflicts, theirs);
			let manifest = read_manifest(&root).unwrap();
			let words = if theirs { 100 } else { 200 };
			assert_eq!(manifest.documents()[0].target, Some(words));
		}
	}

	#[test]
	fn the_book_and_a_cover_of_another_kind_settle_either_way() {
		for theirs in [false, true] {
			let (_parent, root, ..) = novel();
			let change = |title: &str, file: &str, image: &[u8]| {
				let source = root.parent().unwrap().join(file);
				fs::write(&source, image).unwrap();
				set_cover(root.clone(), source).unwrap();
				let mut book = read_manifest(&root).unwrap().book;
				book.title = title.to_owned();
				write_book(root.clone(), book).unwrap();
			};
			let conflicts = diverged(
				&root,
				|| change("Theirs", "cover.jpg", b"\xff\xd8\xff\xe0theirs"),
				|| change("Ours", "cover.png", b"\x89PNG\r\n\x1a\nours"),
			);
			assert!(conflicts.contains(&merge::Conflict::Cover {
				ours: Some("cover.png".to_owned()),
				theirs: Some("cover.jpg".to_owned()),
			}));
			assert!(side_cover(&root, false).unwrap().ends_with(b"ours"));
			assert!(side_cover(&root, true).unwrap().ends_with(b"theirs"));

			settled_as(&root, conflicts, theirs);
			let book = read_manifest(&root).unwrap().book;
			let (title, kept, gone) = match theirs {
				true => ("Theirs", "cover.jpg", "cover.png"),
				false => ("Ours", "cover.png", "cover.jpg"),
			};
			assert_eq!((book.title.as_str(), book.cover.as_str()), (title, kept));
			assert!(root.join(kept).is_file());
			assert!(!root.join(gone).exists());
		}
	}

	#[test]
	fn a_checkpoint_finishes_the_merge_once_text_and_binder_are_settled() {
		let (_parent, root, seeded, ..) = novel();
		write_document(root.clone(), seeded, BASE.to_owned()).unwrap();
		let conflicts = diverged(
			&root,
			|| {
				let draft = "One, in Act two.\n\nTwo.\n\nThree.\n";
				write_document(root.clone(), seeded, draft.to_owned()).unwrap();
				rename_document(root.clone(), seeded, "There".to_owned()).unwrap();
			},
			|| {
				write_document(root.clone(), seeded, MAINS.to_owned()).unwrap();
				rename_document(root.clone(), seeded, "Here".to_owned()).unwrap();
			},
		);
		let (text, binder): (Vec<_>, Vec<_>) = conflicts
			.into_iter()
			.partition(|conflict| matches!(conflict, merge::Conflict::Text { .. }));
		assert_eq!(text.len(), 1);

		// The document moves to its new name still conflicted.
		settle_rest(&root, &binder).unwrap();
		assert_eq!(placed(&root, seeded).1, "There.md");
		assert!(spots(&root, seeded).unwrap().is_some());
		waits(&root);

		settle(&root, seeded, "One, settled.\n\nTwo.\n\nThree.\n").unwrap();
		checkpoint(&root, "Merged Act two", "").unwrap();
		assert_eq!(parents(&root).len(), 2);
		assert!(!pending(&root).unwrap());
		assert_eq!(body_now(&root, seeded), "One, settled.\n\nTwo.\n\nThree.\n");
	}

	#[cfg(unix)]
	#[test]
	fn a_merge_that_fails_partway_can_be_aborted() {
		use std::os::unix::fs::PermissionsExt;
		let (_parent, root, _, here, _) = novel();
		held(&root, "Base");
		new_branch(&root, "Act two").unwrap();
		create_folder(root.clone(), here, "Empty".to_owned(), None).unwrap();
		held(&root, "In Act two");
		switch(&root, "main").unwrap();
		let main = repo_head(&root);
		let folder = root.join(tree::path(&read_manifest(&root).unwrap().nodes, here).unwrap());
		fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();

		let failed = merge(&root, "act-two", "");

		fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
		assert!(failed.is_err());
		assert!(merging(&root).unwrap().is_some());
		abort(&root).unwrap();
		assert_eq!(repo_head(&root), main);
		assert!(!pending(&root).unwrap());
		assert_eq!(merge_state_of(&root), RepositoryState::Clean);
	}

	#[test]
	fn a_merge_whose_message_went_missing_is_still_under_way() {
		let (_parent, root, ..) = conflicted();
		let repo = Repository::open(&root).unwrap();
		fs::remove_file(repo.path().join("MERGE_MSG")).unwrap();

		let merging = merging(&root).unwrap().unwrap();

		assert_eq!(merging.message, "");
	}

	#[test]
	fn a_document_moved_into_a_folder_deleted_here_stays_where_this_version_had_it() {
		let (_parent, root, _, here, _) = novel();
		let f = create_folder(root.clone(), here, "F".to_owned(), None)
			.unwrap()
			.id();
		create_document(root.clone(), f, "Inside".to_owned()).unwrap();
		let x = create_document(root.clone(), here, "X".to_owned())
			.unwrap()
			.id;

		diverged(
			&root,
			|| move_node(root.clone(), x, f, 0).unwrap(),
			|| delete_folder(root.clone(), f).unwrap(),
		);

		assert_eq!(placed(&root, x).0, Some(here));
	}

	#[test]
	fn the_one_take_left_from_the_other_version_keeps_its_text() {
		let (_parent, root, seeded, ..) = novel();
		let first = try_something(root.clone(), seeded).unwrap();
		let mut added = None;

		diverged(
			&root,
			|| added = Some(try_something(root.clone(), seeded).unwrap()[2].id),
			|| {
				delete_take(root.clone(), seeded, first[0].id).unwrap();
			},
		);

		let takes = list_takes(root.clone(), seeded).unwrap();
		assert_eq!(takes.len(), 2);
		assert!(takes.iter().any(|take| Some(take.id) == added));
	}

	#[test]
	fn a_file_outside_the_binder_settles_either_way() {
		for (side, kept) in [("mine", "Mine"), ("theirs", "Theirs")] {
			let (_parent, root, ..) = novel();
			let notes = root.join("notes.txt");
			let conflicts = diverged(
				&root,
				|| fs::write(&notes, "Theirs").unwrap(),
				|| fs::write(&notes, "Mine").unwrap(),
			);
			assert_eq!(
				conflicts,
				[merge::Conflict::File {
					path: "notes.txt".to_owned(),
					ours: Some("Mine".to_owned()),
					theirs: Some("Theirs".to_owned()),
				}]
			);

			settle_file(&root, "notes.txt", side).unwrap();

			assert_eq!(fs::read_to_string(&notes).unwrap(), kept);
			let repo = Repository::open(&root).unwrap();
			assert!(!repo.index().unwrap().has_conflicts());
		}
	}

	#[test]
	fn a_commit_with_no_binder_reads_as_holding_nothing() {
		let (_parent, root, ..) = novel();
		let repo = Repository::open(&root).unwrap();
		let mut builder = repo.treebuilder(None).unwrap();
		builder
			.insert("README", repo.blob(b"Read me").unwrap(), 0o100644)
			.unwrap();
		let tree = repo.find_tree(builder.write().unwrap()).unwrap();
		let signature = signature("").unwrap();
		let readme = repo
			.commit(None, &signature, &signature, "README", &tree, &[])
			.unwrap()
			.to_string();

		let changes = changes(&root, Some(&readme), Some("HEAD")).unwrap();

		let mut placed = Vec::new();
		place(&read_manifest(&root).unwrap().nodes, None, "", &mut placed);
		assert_eq!(changes.nodes.len(), placed.len());
		assert!(changes.nodes.iter().all(|change| change.before.is_none()));
	}

	#[test]
	fn staging_half_a_swap_of_names_is_refused() {
		let (_parent, root, _, here, _) = novel();
		let add = |name: &str| {
			create_document(root.clone(), here, name.to_owned())
				.unwrap()
				.id
		};
		let (a, b) = (add("A"), add("B"));
		held(&root, "Two");
		rename_document(root.clone(), a, "Spare".to_owned()).unwrap();
		rename_document(root.clone(), b, "A".to_owned()).unwrap();
		rename_document(root.clone(), a, "B".to_owned()).unwrap();
		let before = checkpoint_and_index(&root);

		let refused = carry(&root, Row::Node { id: a }, true);

		assert!(matches!(refused, Err(Error::NameStaged)));
		assert_eq!(checkpoint_and_index(&root), before);
	}

	fn folder_node(id: Uuid, name: &str, children: Vec<Node>) -> Node {
		Node::Folder {
			id,
			name: name.to_owned(),
			kind: None,
			target: None,
			fields: tree::Fields::new(),
			children,
		}
	}

	#[test]
	fn taking_a_folder_out_of_one_it_now_holds_keeps_both() {
		let (p, f, d) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
		let document = Node::Document {
			id: d,
			name: "d.md".to_owned(),
			target: None,
			takes: BTreeMap::new(),
			chosen: None,
			placed: None,
		};
		let from = vec![folder_node(
			p,
			"P",
			vec![folder_node(f, "F", vec![document.clone()])],
		)];
		let mut into = vec![folder_node(
			f,
			"F",
			vec![document, folder_node(p, "P", vec![])],
		)];

		take(&from, &mut into, f);

		assert_eq!(tree::path(&into, d).as_deref(), Some("P/F/d.md"));
	}

	#[test]
	fn a_document_comes_back_under_its_old_name_another_has_taken_since() {
		let (_parent, root, _, here, there) = novel();
		let a = create_document(root.clone(), here, "A".to_owned())
			.unwrap()
			.id;
		let start = kept(&root);
		move_node(root.clone(), a, there, 0).unwrap();
		rename_document(root.clone(), a, "B".to_owned()).unwrap();
		create_document(root.clone(), there, "A".to_owned()).unwrap();

		put_back_document(root.clone(), start, a).unwrap();

		assert_eq!(placed(&root, a), (Some(here), "A.md".to_owned()));
	}
}
