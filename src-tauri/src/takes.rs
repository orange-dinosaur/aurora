//! Other ways of writing a document.
//!
//! The document's own file always holds the chosen take, so nothing that reads
//! a document has to know takes exist. Every take also has a file of its own,
//! prose only, and the chosen take's is a stale copy that nothing reads:
//! keeping it means a merge can never leave a take without a file.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use uuid::Uuid;

use crate::document::{body, resolve};
use crate::project::{
	Error, MANUSCRIPT, Manifest, Result, read_manifest, write_atomic, write_manifest,
};
use crate::tree::{self, Node, Take};
use crate::versions::merge_underway;

pub const TAKES_DIR: &str = ".takes";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TakeView {
	pub id: Uuid,
	pub name: String,
	pub from: Option<Uuid>,
	pub chosen: bool,
	pub words: usize,
}

/// Where one take's file goes. The folder is made if it is missing, and one
/// linked out of the project is refused.
fn file(root: &Path, id: Uuid, take: Uuid) -> Result<PathBuf> {
	let folder = root.join(TAKES_DIR).join(id.to_string());
	fs::create_dir_all(&folder)?;
	if !folder.canonicalize()?.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}
	Ok(folder.join(format!("{take}.md")))
}

fn read(path: &Path) -> Result<String> {
	String::from_utf8(fs::read(path)?).map_err(|_| Error::NotText)
}

fn takes_of(manifest: &Manifest, id: Uuid) -> Result<(&BTreeMap<Uuid, Take>, Option<Uuid>)> {
	match tree::find(&manifest.nodes, id) {
		Some(Node::Document { takes, chosen, .. }) => Ok((takes, *chosen)),
		_ => Err(Error::UnknownDocument),
	}
}

/// One take's text. The chosen take's is the document's, never its own file.
fn prose(root: &Path, manifest: &Manifest, id: Uuid, take: Uuid) -> Result<String> {
	let (takes, chosen) = takes_of(manifest, id)?;
	if !takes.contains_key(&take) {
		return Err(Error::UnknownDocument);
	}
	if chosen == Some(take) {
		let text = read(&resolve(manifest, root, id)?)?;
		Ok(body(&text).to_owned())
	} else {
		read(&file(root, id, take)?)
	}
}

/// A document's takes in order. One whose file cannot be read counts as
/// nothing, so it can still be chosen around or deleted.
fn views(root: &Path, manifest: &Manifest, id: Uuid) -> Result<Vec<TakeView>> {
	let (takes, chosen) = takes_of(manifest, id)?;
	let mut ordered: Vec<(&Uuid, &Take)> = takes.iter().collect();
	ordered.sort_by_key(|(take, details)| (details.number, **take));
	Ok(ordered
		.into_iter()
		.map(|(&take, details)| TakeView {
			id: take,
			name: details.name.clone(),
			from: details.from,
			chosen: chosen == Some(take),
			words: prose(root, manifest, id, take)
				.map_or(0, |text| text.split_whitespace().count()),
		})
		.collect())
}

/// Adds a take with the next number, named after it.
fn add(takes: &mut BTreeMap<Uuid, Take>, from: Option<Uuid>) -> Uuid {
	let number = takes.values().map(|take| take.number).max().unwrap_or(0) + 1;
	let id = Uuid::new_v4();
	let name = format!("Take {number}");
	takes.insert(id, Take { name, number, from });
	id
}

#[tauri::command]
pub fn list_takes(root: PathBuf, id: Uuid) -> Result<Vec<TakeView>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	views(&root, &read_manifest(&root)?, id)
}

#[tauri::command]
pub fn take_text(root: PathBuf, id: Uuid, take: Uuid) -> Result<String> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	prose(&root, &read_manifest(&root)?, id, take)
}

/// Starts a new take as a copy of the document's text and chooses it. The first
/// time, the text as it stands becomes a take too, so it can be gone back to.
#[tauri::command]
pub fn try_something(root: PathBuf, id: Uuid) -> Result<Vec<TakeView>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	if merge_underway(&root)? {
		return Err(Error::MergeUnderway);
	}

	let mut manifest = read_manifest(&root)?;
	let text = read(&resolve(&manifest, &root, id)?)?;
	let in_manuscript =
		tree::trail(&manifest.nodes, id).is_some_and(|chain| chain[0].name() == MANUSCRIPT);
	if !in_manuscript {
		return Err(Error::NotInManuscript);
	}

	let Some(Node::Document { takes, chosen, .. }) = tree::find_mut(&mut manifest.nodes, id) else {
		return Err(Error::UnknownDocument);
	};
	let from = match *chosen {
		Some(from) => from,
		None => add(takes, None),
	};
	let new = add(takes, Some(from));
	*chosen = Some(new);

	// The take being left has moved on from its file, which holds what it was
	// when it was last left.
	for take in [from, new] {
		write_atomic(&file(&root, id, take)?, body(&text).as_bytes())?;
	}
	write_manifest(&root, &mut manifest)?;
	views(&root, &manifest, id)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::document::{read_document, refresh, write_document};
	use crate::project::{Format, create};
	use crate::versions::ensure_repository;
	use git2::{Repository, Signature};
	use time::OffsetDateTime;

	const SCENE: &str = "---\nsynopsis: The crossing.\n---\nThe ice held.\n";

	fn project() -> (tempfile::TempDir, PathBuf) {
		let dir = tempfile::tempdir().unwrap();
		let root = create(
			dir.path(),
			"Wintering",
			Format::Novel,
			OffsetDateTime::UNIX_EPOCH,
		)
		.unwrap();
		ensure_repository(&root).unwrap();
		(dir, root)
	}

	fn document(root: &Path, section: &str) -> Uuid {
		read_manifest(root)
			.unwrap()
			.documents()
			.into_iter()
			.find(|document| document.path.starts_with(section))
			.unwrap()
			.id
	}

	fn scene(root: &Path) -> Uuid {
		let id = document(root, MANUSCRIPT);
		write_document(root.to_owned(), id, SCENE.to_owned()).unwrap();
		id
	}

	fn names(takes: &[TakeView]) -> Vec<&str> {
		takes.iter().map(|take| take.name.as_str()).collect()
	}

	#[test]
	fn a_document_with_no_takes_writes_no_keys() {
		let (_dir, root) = project();
		let id = scene(&root);

		assert!(list_takes(root.clone(), id).unwrap().is_empty());
		let json = fs::read_to_string(root.join("aurora.json")).unwrap();
		assert!(!json.contains("takes"));
		assert!(!json.contains("chosen"));
	}

	#[test]
	fn the_first_try_makes_the_text_as_it_stands_a_take() {
		let (_dir, root) = project();
		let id = scene(&root);

		let takes = try_something(root.clone(), id).unwrap();

		assert_eq!(names(&takes), ["Take 1", "Take 2"]);
		assert!(!takes[0].chosen);
		assert!(takes[1].chosen);
		assert_eq!(takes[0].from, None);
		assert_eq!(takes[1].from, Some(takes[0].id));
		assert_eq!(takes[0].words, 3);
		assert_eq!(takes[1].words, 3);
		assert_eq!(read_document(root.clone(), id).unwrap(), SCENE);
		for take in &takes {
			let on_disk = fs::read_to_string(file(&root, id, take.id).unwrap()).unwrap();
			assert_eq!(on_disk, "The ice held.\n");
		}
	}

	#[test]
	fn the_next_try_starts_from_the_chosen_take() {
		let (_dir, root) = project();
		let id = scene(&root);
		let first = try_something(root.clone(), id).unwrap();

		let rewritten = "---\nsynopsis: The crossing.\n---\nThe ice broke.\n";
		write_document(root.clone(), id, rewritten.to_owned()).unwrap();
		let takes = try_something(root.clone(), id).unwrap();

		assert_eq!(names(&takes), ["Take 1", "Take 2", "Take 3"]);
		assert!(takes[2].chosen);
		assert_eq!(takes[2].from, Some(first[1].id));
		assert_eq!(read_document(root.clone(), id).unwrap(), rewritten);
		let text = |take: Uuid| take_text(root.clone(), id, take).unwrap();
		assert_eq!(text(takes[0].id), "The ice held.\n");
		assert_eq!(text(takes[1].id), "The ice broke.\n");
		assert_eq!(text(takes[2].id), "The ice broke.\n");
	}

	#[test]
	fn the_chosen_take_is_read_from_the_document_not_its_file() {
		let (_dir, root) = project();
		let id = scene(&root);
		let takes = try_something(root.clone(), id).unwrap();

		fs::write(file(&root, id, takes[1].id).unwrap(), "A stale copy.").unwrap();

		assert_eq!(
			take_text(root.clone(), id, takes[1].id).unwrap(),
			"The ice held.\n"
		);
		assert_eq!(list_takes(root.clone(), id).unwrap()[1].words, 3);
	}

	#[test]
	fn a_take_another_document_has_is_unknown() {
		let (_dir, root) = project();
		let id = scene(&root);
		try_something(root.clone(), id).unwrap();

		let other = take_text(root.clone(), id, Uuid::new_v4());
		assert!(matches!(other, Err(Error::UnknownDocument)));
	}

	#[test]
	fn takes_survive_a_refresh_and_the_manifest_round_trip() {
		let (_dir, root) = project();
		let id = scene(&root);
		let takes = try_something(root.clone(), id).unwrap();
		let documents = read_manifest(&root).unwrap().documents().len();

		refresh(&root).unwrap();

		let manifest = read_manifest(&root).unwrap();
		assert_eq!(manifest.documents().len(), documents);
		assert_eq!(views(&root, &manifest, id).unwrap(), takes);
	}

	#[test]
	fn only_a_document_in_the_manuscript_can_start_a_take() {
		let (_dir, root) = project();
		let outline = document(&root, "Outline");

		let refused = try_something(root.clone(), outline);

		assert!(matches!(refused, Err(Error::NotInManuscript)));
		assert!(!root.join(TAKES_DIR).exists());
	}

	#[test]
	fn nothing_starts_during_a_merge() {
		let (_dir, root) = project();
		let id = scene(&root);
		let repo = Repository::open(&root).unwrap();
		let tree = repo.index().unwrap().write_tree().unwrap();
		let tree = repo.find_tree(tree).unwrap();
		let writer = Signature::now("Writer", "writer@localhost").unwrap();
		let commit = repo
			.commit(Some("HEAD"), &writer, &writer, "Other", &tree, &[])
			.unwrap();
		repo.reference("MERGE_HEAD", commit, true, "").unwrap();

		let refused = try_something(root.clone(), id);

		assert!(matches!(refused, Err(Error::MergeUnderway)));
	}
}
