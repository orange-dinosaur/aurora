//! Other ways of writing a document.
//!
//! The document's own file always holds the chosen take, so nothing that reads
//! a document has to know takes exist. Every take also has a file of its own,
//! prose only, and the chosen take's is a stale copy that nothing reads:
//! keeping it means a merge can never leave a take without a file.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::document::{TRASH_DIR, body, resolve, with_body};
use crate::project::{
	Error, MANUSCRIPT, Manifest, NameError, Result, read_manifest, write_atomic, write_manifest,
};
use crate::tree::{self, Node, Take};
use crate::versions::{self, merge_underway};

pub const TAKES_DIR: &str = ".takes";

/// The file beside a trashed document's takes. The manifest has let go of them
/// by then, so this is what still knows their names and which was chosen.
const DETAILS: &str = "takes.json";

#[derive(Serialize, Deserialize)]
struct Details {
	takes: BTreeMap<Uuid, Take>,
	chosen: Option<Uuid>,
}

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

/// Where a document's takes are kept. The folder is made if it is missing, and
/// one linked out of the project is refused.
fn folder(root: &Path, id: Uuid) -> Result<PathBuf> {
	let folder = root.join(TAKES_DIR).join(id.to_string());
	fs::create_dir_all(&folder)?;
	within(root, &folder)?;
	Ok(folder)
}

fn within(root: &Path, path: &Path) -> Result<()> {
	if path.canonicalize()?.starts_with(root.canonicalize()?) {
		Ok(())
	} else {
		Err(Error::OutsideProject)
	}
}

fn file(root: &Path, id: Uuid, take: Uuid) -> Result<PathBuf> {
	Ok(folder(root, id)?.join(format!("{take}.md")))
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

fn takes_mut(
	manifest: &mut Manifest,
	id: Uuid,
) -> Result<(&mut BTreeMap<Uuid, Take>, &mut Option<Uuid>)> {
	match tree::find_mut(&mut manifest.nodes, id) {
		Some(Node::Document { takes, chosen, .. }) => Ok((takes, chosen)),
		_ => Err(Error::UnknownDocument),
	}
}

/// The manifest, for a command that changes takes. None may during a merge.
fn editable(root: &Path) -> Result<Manifest> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	if merge_underway(root)? {
		return Err(Error::MergeUnderway);
	}
	read_manifest(root)
}

/// Takes in order, by number and then id, since a merge can bring two with one
/// number.
pub(crate) fn in_order(takes: &BTreeMap<Uuid, Take>) -> Vec<(Uuid, &Take)> {
	let mut ordered: Vec<(Uuid, &Take)> = takes
		.iter()
		.map(|(&take, details)| (take, details))
		.collect();
	ordered.sort_by_key(|(take, details)| (details.number, *take));
	ordered
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
	Ok(in_order(takes)
		.into_iter()
		.map(|(take, details)| TakeView {
			id: take,
			name: details.name.clone(),
			from: details.from,
			chosen: chosen == Some(take),
			words: prose(root, manifest, id, take)
				.map_or(0, |text| text.split_whitespace().count()),
		})
		.collect())
}

/// Makes a document's takes these, each with the text in `files`, and drops any
/// other. The new files land before the manifest names them and the old ones
/// go after, so a take is never listed without its file.
pub(crate) fn restore(
	root: &Path,
	id: Uuid,
	takes: BTreeMap<Uuid, Take>,
	chosen: Option<Uuid>,
	files: &BTreeMap<Uuid, Vec<u8>>,
) -> Result<()> {
	let mut manifest = read_manifest(root)?;
	let (now, was) = takes_mut(&mut manifest, id)?;
	*now = takes;
	*was = chosen;
	if files.is_empty() && !root.join(TAKES_DIR).join(id.to_string()).exists() {
		return write_manifest(root, &mut manifest);
	}

	let mut kept = Vec::new();
	for (take, text) in files {
		let path = file(root, id, *take)?;
		write_atomic(&path, text)?;
		kept.push(path);
	}
	write_manifest(root, &mut manifest)?;

	let folder = folder(root, id)?;
	for entry in fs::read_dir(&folder)? {
		let path = entry?.path();
		if !kept.contains(&path) {
			fs::remove_file(path)?;
		}
	}
	if files.is_empty() {
		fs::remove_dir(folder)?;
	}
	Ok(())
}

/// Where the takes of something in the trash wait: at the path it has there, so
/// a folder's take folders sit as its documents do inside it.
fn waiting(root: &Path, at: &str) -> PathBuf {
	root.join(TRASH_DIR).join(TAKES_DIR).join(at)
}

/// What follows `top` in a path at or below it, which is nothing for `top`.
fn below<'a>(path: &'a str, top: &str) -> Option<&'a str> {
	let rest = path.strip_prefix(top)?;
	(rest.is_empty() || rest.starts_with('/')).then_some(rest)
}

/// Moves the takes of everything at or below `was` to wait beside the trash
/// entry at `at`, with their details, while the manifest still has them.
pub(crate) fn into_trash(root: &Path, manifest: &Manifest, was: &str, at: &str) -> Result<()> {
	for document in manifest.documents() {
		let Some(rest) = below(&document.path, was) else {
			continue;
		};
		let (takes, chosen) = takes_of(manifest, document.id)?;
		if takes.is_empty() {
			continue;
		}

		let takes = takes.clone();
		let details = serde_json::to_vec(&Details { takes, chosen })?;
		let from = folder(root, document.id)?;
		write_atomic(&from.join(DETAILS), &details)?;

		let to = waiting(root, &format!("{at}{rest}"));
		let above = to.parent().ok_or(Error::BadDocumentPath)?;
		fs::create_dir_all(above)?;
		within(root, above)?;
		fs::rename(from, to)?;
	}
	Ok(())
}

/// Gives what came back from `at` to `back` its takes again. The documents have
/// new ids by now, so the takes are found by path. The manifest only learns of
/// them once every file has landed, and the details stay until it has.
pub(crate) fn out_of_trash(root: &Path, at: &str, back: &str) -> Result<()> {
	let entry = waiting(root, at);
	if !entry.exists() {
		return Ok(());
	}
	within(root, &entry)?;
	let takes_dir = root.join(TAKES_DIR);
	fs::create_dir_all(&takes_dir)?;
	within(root, &takes_dir)?;

	let mut manifest = read_manifest(root)?;
	let mut landed = Vec::new();
	for document in manifest.documents() {
		let Some(rest) = below(&document.path, back) else {
			continue;
		};
		let from = waiting(root, &format!("{at}{rest}"));
		if !from.exists() {
			continue;
		}

		let details: Details = serde_json::from_slice(&fs::read(from.join(DETAILS))?)?;
		let to = takes_dir.join(document.id.to_string());
		fs::rename(from, &to)?;
		landed.push(to.join(DETAILS));

		let (takes, chosen) = takes_mut(&mut manifest, document.id)?;
		*takes = details.takes;
		*chosen = details.chosen;
	}
	write_manifest(root, &mut manifest)?;

	for details in landed {
		fs::remove_file(details)?;
	}
	Ok(())
}

/// Throws away the takes waiting beside a trash entry.
pub(crate) fn purge(root: &Path, at: &str) -> Result<()> {
	let entry = waiting(root, at);
	if entry.exists() {
		within(root, &entry)?;
		fs::remove_dir_all(entry)?;
	}
	Ok(())
}

/// Adds a take with the next number, named after it.
fn add(takes: &mut BTreeMap<Uuid, Take>, from: Option<Uuid>) -> Uuid {
	let number = takes.values().map(|take| take.number).max().unwrap_or(0) + 1;
	let id = Uuid::new_v4();
	let name = format!("Take {number}");
	takes.insert(id, Take { name, number, from });
	id
}

/// Puts a waiting take's text in the document, under its front matter. The text
/// leaving goes to its take's file first.
fn bring_in(root: &Path, manifest: &Manifest, id: Uuid, take: Uuid) -> Result<()> {
	let (_, chosen) = takes_of(manifest, id)?;
	let path = resolve(manifest, root, id)?;
	let text = read(&path)?;
	let arriving = read(&file(root, id, take)?)?;
	if let Some(leaving) = chosen {
		write_atomic(&file(root, id, leaving)?, body(&text).as_bytes())?;
	}
	write_atomic(&path, with_body(&text, &arriving).as_bytes())
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

/// Starts a new take and chooses it, holding `prose` or else a copy of the
/// document's text. The first time, the text as it stands becomes a take too,
/// so it can be gone back to.
fn start(
	root: &Path,
	mut manifest: Manifest,
	id: Uuid,
	prose: Option<&str>,
) -> Result<Vec<TakeView>> {
	let path = resolve(&manifest, root, id)?;
	let text = read(&path)?;
	let in_manuscript =
		tree::trail(&manifest.nodes, id).is_some_and(|chain| chain[0].name() == MANUSCRIPT);
	if !in_manuscript {
		return Err(Error::NotInManuscript);
	}

	let (takes, chosen) = takes_mut(&mut manifest, id)?;
	let from = match *chosen {
		Some(from) => from,
		None => add(takes, None),
	};
	let new = add(takes, Some(from));
	*chosen = Some(new);

	// The take being left has moved on from its file, which holds what it was
	// when it was last left.
	write_atomic(&file(root, id, from)?, body(&text).as_bytes())?;
	let arriving = prose.unwrap_or(body(&text));
	write_atomic(&file(root, id, new)?, arriving.as_bytes())?;
	if prose.is_some() {
		write_atomic(&path, with_body(&text, arriving).as_bytes())?;
	}
	write_manifest(root, &mut manifest)?;
	views(root, &manifest, id)
}

#[tauri::command]
pub fn try_something(root: PathBuf, id: Uuid) -> Result<Vec<TakeView>> {
	start(&root, editable(&root)?, id, None)
}

/// Starts a new take from the document's prose at a version, found by id, under
/// the front matter it has now.
#[tauri::command]
pub fn open_as_take(root: PathBuf, version: String, id: Uuid) -> Result<Vec<TakeView>> {
	let manifest = editable(&root)?;
	let then = versions::text(&root, &version, id)?.ok_or(Error::UnknownDocument)?;
	start(&root, manifest, id, Some(body(&then)))
}

/// Makes a waiting take the document's text. Choosing the chosen take does
/// nothing.
#[tauri::command]
pub fn choose_take(root: PathBuf, id: Uuid, take: Uuid) -> Result<Vec<TakeView>> {
	let mut manifest = editable(&root)?;
	let (takes, chosen) = takes_of(&manifest, id)?;
	if !takes.contains_key(&take) {
		return Err(Error::UnknownDocument);
	}

	if chosen != Some(take) {
		bring_in(&root, &manifest, id, take)?;
		*takes_mut(&mut manifest, id)?.1 = Some(take);
		write_manifest(&root, &mut manifest)?;
	}
	views(&root, &manifest, id)
}

#[tauri::command]
pub fn rename_take(root: PathBuf, id: Uuid, take: Uuid, name: String) -> Result<Vec<TakeView>> {
	let mut manifest = editable(&root)?;
	let name = name.trim();
	if name.is_empty() {
		return Err(NameError::Empty.into());
	}

	let (takes, _) = takes_mut(&mut manifest, id)?;
	if takes
		.iter()
		.any(|(other, details)| *other != take && details.name == name)
	{
		return Err(Error::TakeExists);
	}
	let details = takes.get_mut(&take).ok_or(Error::UnknownDocument)?;
	details.name = name.to_owned();

	write_manifest(&root, &mut manifest)?;
	views(&root, &manifest, id)
}

/// Removes a take for good. If it was chosen, the take to its left takes over,
/// or the next one if it was first. When one take is left the document is
/// ordinary again, holding that take's text.
#[tauri::command]
pub fn delete_take(root: PathBuf, id: Uuid, take: Uuid) -> Result<Vec<TakeView>> {
	let mut manifest = editable(&root)?;
	let (takes, chosen) = takes_of(&manifest, id)?;
	let order = in_order(takes);
	let at = order
		.iter()
		.position(|(one, _)| *one == take)
		.ok_or(Error::UnknownDocument)?;
	let neighbour = order
		.get(if at == 0 { 1 } else { at - 1 })
		.map(|(one, _)| *one);

	let next = if chosen == Some(take) {
		if let Some(neighbour) = neighbour {
			bring_in(&root, &manifest, id, neighbour)?;
		}
		neighbour
	} else {
		chosen
	};

	let (takes, chosen) = takes_mut(&mut manifest, id)?;
	takes.remove(&take);
	*chosen = next;
	let ordinary = takes.len() < 2;
	if ordinary {
		takes.clear();
		*chosen = None;
	}
	write_manifest(&root, &mut manifest)?;

	if ordinary {
		fs::remove_dir_all(folder(&root, id)?)?;
	} else {
		let path = file(&root, id, take)?;
		if path.exists() {
			fs::remove_file(path)?;
		}
	}
	views(&root, &manifest, id)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::document::{
		TrashEntry, create_document, create_folder, delete_document, delete_folder, list_trash,
		move_node, purge_trash_entry, read_document, refresh, rename_document, restore_from_trash,
		write_document,
	};
	use crate::project::{Format, create};
	use crate::tree::FolderKind;
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

	fn start_merge(root: &Path) {
		let repo = Repository::open(root).unwrap();
		let tree = repo.index().unwrap().write_tree().unwrap();
		let tree = repo.find_tree(tree).unwrap();
		let writer = Signature::now("Writer", "writer@localhost").unwrap();
		let commit = repo
			.commit(Some("HEAD"), &writer, &writer, "Other", &tree, &[])
			.unwrap();
		repo.reference("MERGE_HEAD", commit, true, "").unwrap();
	}

	#[test]
	fn nothing_starts_during_a_merge() {
		let (_dir, root) = project();
		let id = scene(&root);
		start_merge(&root);

		let refused = try_something(root.clone(), id);

		assert!(matches!(refused, Err(Error::MergeUnderway)));
	}

	#[test]
	fn nothing_changes_during_a_merge() {
		let (_dir, root) = project();
		let id = scene(&root);
		let takes = try_something(root.clone(), id).unwrap();
		start_merge(&root);
		let first = takes[0].id;

		let chose = choose_take(root.clone(), id, first);
		let renamed = rename_take(root.clone(), id, first, "Colder".to_owned());
		let deleted = delete_take(root.clone(), id, first);

		assert!(matches!(chose, Err(Error::MergeUnderway)));
		assert!(matches!(renamed, Err(Error::MergeUnderway)));
		assert!(matches!(deleted, Err(Error::MergeUnderway)));
		assert_eq!(list_takes(root.clone(), id).unwrap(), takes);
	}

	const FRONT: &str = "---\nsynopsis: The crossing.\n---\n";

	/// Three takes, each with its own text under the front matter, `Three`
	/// chosen.
	fn three(root: &Path) -> (Uuid, Vec<TakeView>) {
		let id = scene(root);
		let write = |prose: &str| {
			write_document(root.to_owned(), id, format!("{FRONT}{prose}")).unwrap();
		};
		write("One\n");
		try_something(root.to_owned(), id).unwrap();
		write("Two\n");
		try_something(root.to_owned(), id).unwrap();
		write("Three\n");
		(id, list_takes(root.to_owned(), id).unwrap())
	}

	#[test]
	fn swapping_there_and_back_changes_nothing() {
		let cases = [
			(SCENE, "---\nsynopsis: The crossing.\n---\nThe ice broke.\n"),
			("The ice held.\n", "The ice broke.\n"),
			(
				"---\nsynopsis: The crossing.\n---\n\nThe ice held.\n",
				"---\nsynopsis: The crossing.\n---\n\nThe ice broke.\n",
			),
			(
				"---\nsynopsis: The crossing.\n---  \nThe ice held.\n",
				"---\nsynopsis: The crossing.\n---  \nThe ice broke.\n",
			),
		];
		for (original, rewritten) in cases {
			let (_dir, root) = project();
			let id = document(&root, MANUSCRIPT);
			write_document(root.clone(), id, original.to_owned()).unwrap();
			let takes = try_something(root.clone(), id).unwrap();
			write_document(root.clone(), id, rewritten.to_owned()).unwrap();

			let chosen = choose_take(root.clone(), id, takes[0].id).unwrap();

			assert!(chosen[0].chosen);
			assert!(!chosen[1].chosen);
			assert_eq!(read_document(root.clone(), id).unwrap(), original);
			assert_eq!(
				take_text(root.clone(), id, takes[1].id).unwrap(),
				body(rewritten)
			);

			choose_take(root.clone(), id, takes[1].id).unwrap();

			assert_eq!(read_document(root.clone(), id).unwrap(), rewritten);
			assert_eq!(
				take_text(root.clone(), id, takes[0].id).unwrap(),
				body(original)
			);
		}
	}

	#[test]
	fn a_block_closing_at_the_end_of_the_file_stays_a_block() {
		let text = "---\nsynopsis: The crossing.\n---";

		let swapped = with_body(text, "The ice held.\n");

		assert_eq!(
			swapped,
			"---\nsynopsis: The crossing.\n---\nThe ice held.\n"
		);
		assert_eq!(body(&swapped), "The ice held.\n");
	}

	#[test]
	fn choosing_the_chosen_take_does_nothing() {
		let (_dir, root) = project();
		let (id, takes) = three(&root);
		let stale = file(&root, id, takes[2].id).unwrap();
		fs::write(&stale, "A stale copy.").unwrap();

		let chosen = choose_take(root.clone(), id, takes[2].id).unwrap();

		assert_eq!(chosen, takes);
		assert_eq!(fs::read_to_string(stale).unwrap(), "A stale copy.");
		assert_eq!(
			read_document(root.clone(), id).unwrap(),
			format!("{FRONT}Three\n")
		);
	}

	#[test]
	fn a_stale_chosen_file_is_never_read() {
		let (_dir, root) = project();
		let (id, takes) = three(&root);
		fs::write(file(&root, id, takes[2].id).unwrap(), "A stale copy.").unwrap();

		choose_take(root.clone(), id, takes[1].id).unwrap();
		assert_eq!(take_text(root.clone(), id, takes[2].id).unwrap(), "Three\n");

		choose_take(root.clone(), id, takes[2].id).unwrap();
		fs::write(file(&root, id, takes[2].id).unwrap(), "A stale copy.").unwrap();
		delete_take(root.clone(), id, takes[1].id).unwrap();
		delete_take(root.clone(), id, takes[0].id).unwrap();

		assert_eq!(
			read_document(root.clone(), id).unwrap(),
			format!("{FRONT}Three\n")
		);
	}

	#[test]
	fn choosing_an_unknown_take_is_refused() {
		let (_dir, root) = project();
		let id = scene(&root);
		try_something(root.clone(), id).unwrap();

		let refused = choose_take(root.clone(), id, Uuid::new_v4());

		assert!(matches!(refused, Err(Error::UnknownDocument)));
		assert_eq!(read_document(root.clone(), id).unwrap(), SCENE);
	}

	#[test]
	fn a_rename_trims_the_name_and_keeps_the_order() {
		let (_dir, root) = project();
		let (id, takes) = three(&root);

		let renamed = rename_take(root.clone(), id, takes[1].id, "  Colder \n".to_owned()).unwrap();

		assert_eq!(names(&renamed), ["Take 1", "Colder", "Take 3"]);
		assert_eq!(
			names(&list_takes(root.clone(), id).unwrap()),
			names(&renamed)
		);
	}

	#[test]
	fn a_take_can_keep_its_own_name() {
		let (_dir, root) = project();
		let (id, takes) = three(&root);

		let renamed = rename_take(root.clone(), id, takes[1].id, "Take 2".to_owned()).unwrap();

		assert_eq!(renamed, takes);
	}

	#[test]
	fn a_rename_refuses_an_empty_name_or_one_in_use() {
		let (_dir, root) = project();
		let (id, takes) = three(&root);
		let first = takes[0].id;

		let empty = rename_take(root.clone(), id, first, "   ".to_owned());
		let duplicate = rename_take(root.clone(), id, first, " Take 2 ".to_owned());
		let unknown = rename_take(root.clone(), id, Uuid::new_v4(), "Colder".to_owned());

		assert!(matches!(empty, Err(Error::InvalidName(NameError::Empty))));
		assert!(matches!(duplicate, Err(Error::TakeExists)));
		assert!(matches!(unknown, Err(Error::UnknownDocument)));
		assert_eq!(list_takes(root.clone(), id).unwrap(), takes);
	}

	#[test]
	fn deleting_the_chosen_take_chooses_the_one_to_its_left() {
		let (_dir, root) = project();
		let (id, takes) = three(&root);

		let left = delete_take(root.clone(), id, takes[2].id).unwrap();

		assert_eq!(names(&left), ["Take 1", "Take 2"]);
		assert!(left[1].chosen);
		assert_eq!(
			read_document(root.clone(), id).unwrap(),
			format!("{FRONT}Two\n")
		);
		assert!(!file(&root, id, takes[2].id).unwrap().exists());
	}

	#[test]
	fn deleting_the_first_take_when_chosen_chooses_the_next() {
		let (_dir, root) = project();
		let (id, takes) = three(&root);
		choose_take(root.clone(), id, takes[0].id).unwrap();

		let left = delete_take(root.clone(), id, takes[0].id).unwrap();

		assert_eq!(names(&left), ["Take 2", "Take 3"]);
		assert!(left[0].chosen);
		assert_eq!(
			read_document(root.clone(), id).unwrap(),
			format!("{FRONT}Two\n")
		);
	}

	#[test]
	fn deleting_a_waiting_take_leaves_the_document_alone() {
		let (_dir, root) = project();
		let (id, takes) = three(&root);

		let left = delete_take(root.clone(), id, takes[0].id).unwrap();

		assert_eq!(names(&left), ["Take 2", "Take 3"]);
		assert!(left[1].chosen);
		assert_eq!(left[0].from, Some(takes[0].id));
		assert_eq!(
			read_document(root.clone(), id).unwrap(),
			format!("{FRONT}Three\n")
		);
	}

	#[test]
	fn a_take_whose_file_is_gone_can_still_be_deleted() {
		let (_dir, root) = project();
		let (id, takes) = three(&root);
		fs::remove_file(file(&root, id, takes[0].id).unwrap()).unwrap();

		let left = delete_take(root.clone(), id, takes[0].id).unwrap();

		assert_eq!(names(&left), ["Take 2", "Take 3"]);
	}

	#[test]
	fn the_last_take_left_makes_the_document_ordinary_again() {
		for delete_chosen in [true, false] {
			let (_dir, root) = project();
			let id = scene(&root);
			let takes = try_something(root.clone(), id).unwrap();
			let rewritten = format!("{FRONT}The ice broke.\n");
			write_document(root.clone(), id, rewritten.clone()).unwrap();

			let (gone, kept) = if delete_chosen {
				(takes[1].id, SCENE.to_owned())
			} else {
				(takes[0].id, rewritten)
			};
			let left = delete_take(root.clone(), id, gone).unwrap();

			assert!(left.is_empty());
			assert_eq!(read_document(root.clone(), id).unwrap(), kept);
			assert!(!root.join(TAKES_DIR).join(id.to_string()).exists());
			let json = fs::read_to_string(root.join("aurora.json")).unwrap();
			assert!(!json.contains("takes"));
			assert!(!json.contains("chosen"));
		}
	}

	fn section(root: &Path, name: &str) -> Uuid {
		let manifest = read_manifest(root).unwrap();
		manifest
			.nodes
			.iter()
			.find(|node| node.name() == name)
			.unwrap()
			.id()
	}

	fn at(root: &Path, path: &str) -> Uuid {
		read_manifest(root)
			.unwrap()
			.documents()
			.into_iter()
			.find(|document| document.path == path)
			.unwrap()
			.id
	}

	fn path_of(root: &Path, id: Uuid) -> String {
		tree::path(&read_manifest(root).unwrap().nodes, id).unwrap()
	}

	fn only_entry(root: &Path) -> TrashEntry {
		let mut listed = list_trash(root.to_owned()).unwrap();
		assert_eq!(listed.len(), 1);
		listed.remove(0)
	}

	/// A part holding a scene, and a chapter with one scene that has takes and
	/// one that has none.
	fn part(root: &Path) -> (Uuid, Uuid, Uuid) {
		let make = |parent, name: &str, kind| {
			create_folder(root.to_owned(), parent, name.to_owned(), Some(kind))
				.unwrap()
				.id()
		};
		let scene = |parent, name: &str| {
			let id = create_document(root.to_owned(), parent, name.to_owned())
				.unwrap()
				.id;
			write_document(root.to_owned(), id, format!("{FRONT}{name}\n")).unwrap();
			id
		};
		let part = make(section(root, MANUSCRIPT), "Part", FolderKind::Part);
		let chapter = make(part, "Chapter", FolderKind::Chapter);
		let loose = scene(part, "Loose");
		let deep = scene(chapter, "Deep");
		scene(chapter, "Plain");
		try_something(root.to_owned(), loose).unwrap();
		try_something(root.to_owned(), deep).unwrap();
		(part, loose, deep)
	}

	#[test]
	fn a_deleted_document_gets_its_takes_back_under_its_new_id() {
		let (_dir, root) = project();
		let (id, takes) = three(&root);
		let path = path_of(&root, id);

		delete_document(root.clone(), id).unwrap();

		assert!(!root.join(TAKES_DIR).join(id.to_string()).exists());
		let entry = only_entry(&root);
		assert!(waiting(&root, &entry.path).join(DETAILS).exists());

		restore_from_trash(root.clone(), entry.path.clone()).unwrap();

		let back = at(&root, &path);
		assert_ne!(back, id);
		assert_eq!(list_takes(root.clone(), back).unwrap(), takes);
		assert_eq!(
			read_document(root.clone(), back).unwrap(),
			format!("{FRONT}Three\n")
		);
		let folder = root.join(TAKES_DIR).join(back.to_string());
		assert!(!folder.join(DETAILS).exists());
		assert!(!waiting(&root, &entry.path).exists());
		assert!(list_trash(root.clone()).unwrap().is_empty());

		choose_take(root.clone(), back, takes[0].id).unwrap();
		assert_eq!(
			read_document(root, back).unwrap(),
			format!("{FRONT}One\n"),
			"a waiting take's file came back too"
		);
	}

	#[test]
	fn a_deleted_folder_gets_the_takes_of_everything_inside_back() {
		let (_dir, root) = project();
		let (part, loose, deep) = part(&root);
		let loose_takes = list_takes(root.clone(), loose).unwrap();
		let deep_takes = list_takes(root.clone(), deep).unwrap();

		delete_folder(root.clone(), part).unwrap();

		let entry = only_entry(&root);
		assert_eq!(entry.inside, Some(3));
		let chapter = format!("{}/Chapter", entry.path);
		assert!(waiting(&root, &format!("{}/Loose.md", entry.path)).exists());
		assert!(waiting(&root, &format!("{chapter}/Deep.md")).exists());
		assert!(!waiting(&root, &format!("{chapter}/Plain.md")).exists());

		restore_from_trash(root.clone(), entry.path).unwrap();

		let back = |path: &str| list_takes(root.clone(), at(&root, path)).unwrap();
		assert_eq!(back("Manuscript/Part/Loose.md"), loose_takes);
		assert_eq!(back("Manuscript/Part/Chapter/Deep.md"), deep_takes);
		assert!(back("Manuscript/Part/Chapter/Plain.md").is_empty());
	}

	#[test]
	fn purging_a_document_or_a_folder_removes_its_takes() {
		let (_dir, root) = project();
		let (id, _) = three(&root);
		let (part, ..) = part(&root);

		delete_document(root.clone(), id).unwrap();
		delete_folder(root.clone(), part).unwrap();
		for entry in list_trash(root.clone()).unwrap() {
			purge_trash_entry(root.clone(), entry.path.clone()).unwrap();
			assert!(!waiting(&root, &entry.path).exists());
		}

		assert!(list_trash(root).unwrap().is_empty());
	}

	#[test]
	fn two_documents_of_one_name_keep_their_own_takes() {
		let (_dir, root) = project();
		let manuscript = section(&root, MANUSCRIPT);
		for name in ["North", "South"] {
			let id = create_document(root.clone(), manuscript, "Twin".to_owned())
				.unwrap()
				.id;
			let takes = try_something(root.clone(), id).unwrap();
			rename_take(root.clone(), id, takes[1].id, name.to_owned()).unwrap();
			delete_document(root.clone(), id).unwrap();
		}

		let mut found = Vec::new();
		for (number, entry) in list_trash(root.clone()).unwrap().into_iter().enumerate() {
			restore_from_trash(root.clone(), entry.path).unwrap();
			let back = at(&root, "Manuscript/Twin.md");
			found.push(list_takes(root.clone(), back).unwrap()[1].name.clone());
			rename_document(root.clone(), back, format!("Twin {number}")).unwrap();
		}

		found.sort();
		assert_eq!(found, ["North", "South"]);
	}

	#[test]
	fn a_document_with_no_takes_leaves_nothing_beside_it() {
		let (_dir, root) = project();
		let id = scene(&root);
		let path = path_of(&root, id);

		delete_document(root.clone(), id).unwrap();
		restore_from_trash(root.clone(), only_entry(&root).path).unwrap();

		let back = at(&root, &path);
		assert_eq!(read_document(root.clone(), back).unwrap(), SCENE);
		assert!(list_takes(root.clone(), back).unwrap().is_empty());

		delete_document(root.clone(), back).unwrap();
		purge_trash_entry(root.clone(), only_entry(&root).path).unwrap();

		assert!(!root.join(TRASH_DIR).join(TAKES_DIR).exists());
		assert!(!root.join(TAKES_DIR).exists());
		let json = fs::read_to_string(root.join("aurora.json")).unwrap();
		assert!(!json.contains("takes"));
	}

	#[test]
	fn a_document_outside_the_manuscript_still_gets_its_takes_back() {
		let (_dir, root) = project();
		let (id, takes) = three(&root);
		move_node(root.clone(), id, section(&root, "Notes"), 0).unwrap();
		let path = path_of(&root, id);

		delete_document(root.clone(), id).unwrap();
		restore_from_trash(root.clone(), only_entry(&root).path).unwrap();

		let back = at(&root, &path);
		assert_eq!(list_takes(root.clone(), back).unwrap(), takes);
		assert!(try_something(root, back).is_err());
	}

	#[test]
	fn takes_whose_files_are_gone_do_not_stop_a_deletion() {
		let (_dir, root) = project();
		let (id, takes) = three(&root);
		let path = path_of(&root, id);
		fs::remove_dir_all(root.join(TAKES_DIR).join(id.to_string())).unwrap();

		delete_document(root.clone(), id).unwrap();
		restore_from_trash(root.clone(), only_entry(&root).path).unwrap();

		let back = list_takes(root.clone(), at(&root, &path)).unwrap();
		assert_eq!(names(&back), names(&takes));
	}
}
