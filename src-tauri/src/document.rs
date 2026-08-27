use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::project::{
	Error, MANIFEST_FILE, Manifest, Result, read_manifest, write_atomic, write_json,
};

/// A document inside a project. The path is relative to the project root and
/// always uses forward slashes, so a manifest written on one platform still
/// resolves on another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
	pub id: Uuid,
	pub path: String,
}

impl Document {
	pub(crate) fn new(section: &str, file_name: &str) -> Self {
		Self {
			id: Uuid::new_v4(),
			path: format!("{section}/{file_name}"),
		}
	}

	/// The file name without its extension. The file name is the title.
	fn title(&self) -> String {
		Path::new(&self.path)
			.file_stem()
			.and_then(|stem| stem.to_str())
			.unwrap_or(&self.path)
			.to_owned()
	}
}

/// A document as the sidebar shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentView {
	pub id: Uuid,
	pub path: String,
	pub folder: String,
	pub title: String,
}

impl From<&Document> for DocumentView {
	fn from(document: &Document) -> Self {
		Self {
			id: document.id,
			path: document.path.clone(),
			folder: section_of(&document.path).unwrap_or_default().to_owned(),
			title: document.title(),
		}
	}
}

/// One of the project's sections and the documents in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SectionDocuments {
	pub folder: String,
	pub documents: Vec<DocumentView>,
}

/// The `.md` files in each of the project's sections, as paths relative to the
/// root. Sections keep the order they are given and files within one are
/// sorted. Anything else is ignored: other extensions, nested folders, symlinks
/// and names that are not valid UTF-8.
pub fn scan(root: &Path, folders: &[String]) -> Result<Vec<String>> {
	let mut found = Vec::new();

	for folder in folders {
		let entries = match fs::read_dir(root.join(folder)) {
			Ok(entries) => entries,
			// A section the writer deleted is not an error; it simply holds
			// nothing.
			Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
			Err(e) => return Err(e.into()),
		};

		let mut files = Vec::new();
		for entry in entries {
			let entry = entry?;
			if !entry.file_type()?.is_file() {
				continue;
			}

			let name = entry.file_name();
			let Some(name) = name.to_str() else {
				continue;
			};
			if !is_markdown(name) {
				continue;
			}

			files.push(format!("{folder}/{name}"));
		}

		files.sort();
		found.append(&mut files);
	}

	Ok(found)
}

fn is_markdown(name: &str) -> bool {
	Path::new(name)
		.extension()
		.is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

/// The section a document belongs to, which is the first component of its path.
fn section_of(path: &str) -> Option<&str> {
	path.split_once('/').map(|(section, _)| section)
}

/// Brings the manifest's document list back in step with what `scan` found.
/// Documents that are still there keep their id and their relative order,
/// vanished ones are dropped, and unrecorded files are adopted at the end of
/// their section. The list is regrouped into section order, since that is the
/// order the sidebar reads it in. Returns whether anything changed.
pub fn reconcile(manifest: &mut Manifest, found: &[String]) -> bool {
	let on_disk: HashSet<&str> = found.iter().map(String::as_str).collect();

	let mut seen: HashSet<&str> = HashSet::new();
	let mut kept: HashMap<&str, Vec<Document>> = HashMap::new();
	for document in &manifest.documents {
		// A path recorded twice — only a hand-edited manifest can manage it —
		// would otherwise open as two documents over one file.
		if !on_disk.contains(document.path.as_str()) || !seen.insert(&document.path) {
			continue;
		}
		if let Some(section) = section_of(&document.path) {
			kept.entry(section).or_default().push(document.clone());
		}
	}

	let mut adopted: HashMap<&str, Vec<Document>> = HashMap::new();
	for path in found {
		if seen.contains(path.as_str()) {
			continue;
		}
		if let Some(section) = section_of(path) {
			adopted.entry(section).or_default().push(Document {
				id: Uuid::new_v4(),
				path: path.clone(),
			});
		}
	}

	let mut documents = Vec::new();
	for folder in &manifest.folders {
		if let Some(existing) = kept.remove(folder.as_str()) {
			documents.extend(existing);
		}
		if let Some(new) = adopted.remove(folder.as_str()) {
			documents.extend(new);
		}
	}

	let changed = documents != manifest.documents;
	manifest.documents = documents;
	changed
}

/// Reads the manifest and brings it back in step with the folder, writing it
/// back only when reconciliation actually changed something.
pub fn refresh(root: &Path) -> Result<Manifest> {
	let mut manifest = read_manifest(root)?;
	let found = scan(root, &manifest.folders)?;
	if reconcile(&mut manifest, &found) {
		write_json(&root.join(MANIFEST_FILE), &manifest)?;
	}
	Ok(manifest)
}

/// The manifest's documents grouped under their section, in the order the
/// project records both. Sections are fixed by the format, so an empty one is
/// still listed.
fn sections(manifest: &Manifest) -> Vec<SectionDocuments> {
	manifest
		.folders
		.iter()
		.map(|folder| SectionDocuments {
			folder: folder.clone(),
			documents: manifest
				.documents
				.iter()
				.filter(|d| section_of(&d.path) == Some(folder.as_str()))
				.map(DocumentView::from)
				.collect(),
		})
		.collect()
}

/// Turns a document id into a path on disk, refusing anything that does not
/// end up inside the project. `aurora.json` is an ordinary file a writer can
/// edit, so the path it records is not to be trusted.
fn resolve(manifest: &Manifest, root: &Path, id: Uuid) -> Result<PathBuf> {
	let document = manifest
		.documents
		.iter()
		.find(|d| d.id == id)
		.ok_or(Error::UnknownDocument)?;

	// Canonicalising both sides resolves `..` and follows symlinks, so the
	// comparison is between two real locations.
	let path = root.join(&document.path);
	let path = path.canonicalize().map_err(|e| match e.kind() {
		io::ErrorKind::NotFound => Error::DocumentMissing,
		_ => Error::Io(e),
	})?;

	if !path.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}

	Ok(path)
}

/// The text of one document.
#[tauri::command]
pub fn read_document(root: PathBuf, id: Uuid) -> Result<String> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	let path = resolve(&manifest, &root, id)?;
	String::from_utf8(fs::read(path)?).map_err(|_| Error::NotText)
}

/// Replaces one document's text. The document has to be one the project knows
/// about and its file has to still be there; writing back a file that has
/// vanished is a different thing to ask for.
#[tauri::command]
pub fn write_document(root: PathBuf, id: Uuid, text: String) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	let path = resolve(&manifest, &root, id)?;
	write_atomic(&path, text.as_bytes())
}

/// The sidebar's view of the project, straight from the manifest.
#[tauri::command]
pub fn list_documents(root: PathBuf) -> Result<Vec<SectionDocuments>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Ok(sections(&read_manifest(&root)?))
}

/// The same, after looking at the folder again for anything added, removed or
/// renamed outside Aurora.
#[tauri::command]
pub fn refresh_documents(root: PathBuf) -> Result<Vec<SectionDocuments>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	Ok(sections(&refresh(&root)?))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::project::{Format, create};
	use time::OffsetDateTime;

	fn fixed_time() -> OffsetDateTime {
		OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
	}

	#[test]
	fn a_document_id_survives_the_round_trip() {
		let document = Document::new("Manuscript", "Chapter 1.md");
		let json = serde_json::to_value(&document).unwrap();
		assert_eq!(json["path"], "Manuscript/Chapter 1.md");
		assert_eq!(json["id"], document.id.to_string());
		assert_eq!(serde_json::from_value::<Document>(json).unwrap(), document);
	}

	fn novel_folders() -> Vec<String> {
		Format::Novel
			.layout()
			.iter()
			.map(|s| s.folder.to_owned())
			.collect()
	}

	#[test]
	fn scan_finds_the_seed_files_in_section_order() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		assert_eq!(
			scan(&root, &novel_folders()).unwrap(),
			[
				"Manuscript/Chapter 1.md",
				"Outline/Outline.md",
				"Characters/Characters.md",
				"Locations/Locations.md",
				"Notes/Notes.md",
			]
		);
	}

	#[test]
	fn files_within_a_section_are_sorted() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		for name in ["Chapter 3.md", "Chapter 2.md"] {
			fs::write(root.join("Manuscript").join(name), "").unwrap();
		}

		let found = scan(&root, &novel_folders()).unwrap();
		assert_eq!(
			&found[..3],
			[
				"Manuscript/Chapter 1.md",
				"Manuscript/Chapter 2.md",
				"Manuscript/Chapter 3.md",
			]
		);
	}

	#[test]
	fn scan_ignores_everything_that_is_not_a_markdown_file() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manuscript = root.join("Manuscript");
		fs::write(manuscript.join("cover.png"), "").unwrap();
		fs::write(manuscript.join("notes.txt"), "").unwrap();
		fs::create_dir(manuscript.join("Part One")).unwrap();
		fs::write(manuscript.join("Part One").join("Chapter 2.md"), "").unwrap();

		let found = scan(&root, &novel_folders()).unwrap();
		assert_eq!(
			found
				.iter()
				.filter(|p| p.starts_with("Manuscript/"))
				.count(),
			1,
			"only the seed chapter belongs to Manuscript"
		);
	}

	#[test]
	fn an_uppercase_extension_is_still_markdown() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Notes").join("Ideas.MD"), "").unwrap();

		assert!(
			scan(&root, &novel_folders())
				.unwrap()
				.contains(&"Notes/Ideas.MD".to_owned())
		);
	}

	#[test]
	fn a_missing_section_is_skipped_rather_than_failing() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::remove_dir_all(root.join("Outline")).unwrap();

		let found = scan(&root, &novel_folders()).unwrap();
		assert!(!found.iter().any(|p| p.starts_with("Outline/")));
		assert_eq!(found.len(), 4);
	}

	#[test]
	fn scan_looks_only_where_it_is_told() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::create_dir(root.join("Scraps")).unwrap();
		fs::write(root.join("Scraps").join("Offcut.md"), "").unwrap();

		let found = scan(&root, &novel_folders()).unwrap();
		assert!(!found.iter().any(|p| p.starts_with("Scraps/")));
	}

	fn manifest_with(paths: &[&str]) -> Manifest {
		let mut manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());
		manifest.documents = paths
			.iter()
			.map(|path| Document {
				id: Uuid::new_v4(),
				path: (*path).to_owned(),
			})
			.collect();
		manifest
	}

	fn paths_of(manifest: &Manifest) -> Vec<&str> {
		manifest.documents.iter().map(|d| d.path.as_str()).collect()
	}

	fn owned(paths: &[&str]) -> Vec<String> {
		paths.iter().map(|p| (*p).to_owned()).collect()
	}

	#[test]
	fn reconcile_leaves_a_manifest_that_already_agrees_alone() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 1.md", "Notes/Notes.md"]);
		let before = manifest.documents.clone();

		assert!(!reconcile(
			&mut manifest,
			&owned(&["Manuscript/Chapter 1.md", "Notes/Notes.md"])
		));
		assert_eq!(manifest.documents, before);
	}

	#[test]
	fn a_new_file_is_adopted_at_the_end_of_its_section() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 1.md", "Notes/Notes.md"]);
		let chapter_one = manifest.documents[0].id;

		assert!(reconcile(
			&mut manifest,
			&owned(&[
				"Manuscript/Chapter 1.md",
				"Manuscript/Chapter 2.md",
				"Notes/Notes.md",
			])
		));
		assert_eq!(
			paths_of(&manifest),
			[
				"Manuscript/Chapter 1.md",
				"Manuscript/Chapter 2.md",
				"Notes/Notes.md",
			]
		);
		assert_eq!(
			manifest.documents[0].id, chapter_one,
			"an existing document keeps its id"
		);
	}

	#[test]
	fn a_vanished_file_is_dropped() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 1.md", "Notes/Notes.md"]);

		assert!(reconcile(&mut manifest, &owned(&["Notes/Notes.md"])));
		assert_eq!(paths_of(&manifest), ["Notes/Notes.md"]);
	}

	#[test]
	fn the_recorded_order_wins_over_the_order_on_disk() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 2.md", "Manuscript/Chapter 1.md"]);

		assert!(!reconcile(
			&mut manifest,
			&owned(&["Manuscript/Chapter 1.md", "Manuscript/Chapter 2.md"])
		));
		assert_eq!(
			paths_of(&manifest),
			["Manuscript/Chapter 2.md", "Manuscript/Chapter 1.md"]
		);
	}

	#[test]
	fn a_manifest_without_documents_adopts_everything() {
		let mut manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());
		let found = owned(&["Manuscript/Chapter 1.md", "Notes/Notes.md"]);

		assert!(reconcile(&mut manifest, &found));
		assert_eq!(paths_of(&manifest), found.as_slice());
		let ids: HashSet<_> = manifest.documents.iter().map(|d| d.id).collect();
		assert_eq!(ids.len(), 2, "each adopted file gets its own id");
	}

	#[test]
	fn documents_are_regrouped_into_section_order() {
		let mut manifest = manifest_with(&[
			"Notes/Notes.md",
			"Manuscript/Chapter 1.md",
			"Notes/Ideas.md",
		]);

		assert!(reconcile(
			&mut manifest,
			&owned(&[
				"Manuscript/Chapter 1.md",
				"Notes/Notes.md",
				"Notes/Ideas.md",
			])
		));
		assert_eq!(
			paths_of(&manifest),
			[
				"Manuscript/Chapter 1.md",
				"Notes/Notes.md",
				"Notes/Ideas.md"
			]
		);
	}

	#[test]
	fn a_path_recorded_twice_is_collapsed() {
		let mut manifest = manifest_with(&["Manuscript/Chapter 1.md", "Manuscript/Chapter 1.md"]);
		let first = manifest.documents[0].id;

		assert!(reconcile(
			&mut manifest,
			&owned(&["Manuscript/Chapter 1.md"])
		));
		assert_eq!(paths_of(&manifest), ["Manuscript/Chapter 1.md"]);
		assert_eq!(manifest.documents[0].id, first, "the first id wins");
	}

	#[test]
	fn a_document_outside_the_projects_sections_is_dropped() {
		let mut manifest = manifest_with(&["Scraps/Offcut.md", "Notes/Notes.md"]);

		assert!(reconcile(
			&mut manifest,
			&owned(&["Scraps/Offcut.md", "Notes/Notes.md"])
		));
		assert_eq!(paths_of(&manifest), ["Notes/Notes.md"]);
	}

	#[test]
	fn listing_groups_documents_under_their_section() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let listed = sections(&read_manifest(&root).unwrap());

		let folders: Vec<_> = listed.iter().map(|s| s.folder.as_str()).collect();
		assert_eq!(
			folders,
			["Manuscript", "Outline", "Characters", "Locations", "Notes"]
		);
		assert_eq!(listed[0].documents.len(), 1);
		assert_eq!(listed[0].documents[0].title, "Chapter 1");
		assert_eq!(listed[0].documents[0].folder, "Manuscript");
		assert_eq!(listed[0].documents[0].path, "Manuscript/Chapter 1.md");
	}

	#[test]
	fn an_empty_section_is_still_listed() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::remove_file(root.join("Notes").join("Notes.md")).unwrap();
		refresh(&root).unwrap();

		let listed = sections(&read_manifest(&root).unwrap());
		let notes = listed.iter().find(|s| s.folder == "Notes").unwrap();
		assert!(notes.documents.is_empty());
	}

	#[test]
	fn listing_does_not_look_at_the_folder_but_refreshing_does() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Notes").join("Ideas.md"), "").unwrap();

		let listed = sections(&read_manifest(&root).unwrap());
		let notes = listed.iter().find(|s| s.folder == "Notes").unwrap();
		assert_eq!(
			notes.documents.len(),
			1,
			"the manifest has not been re-read"
		);

		let refreshed = sections(&refresh(&root).unwrap());
		let notes = refreshed.iter().find(|s| s.folder == "Notes").unwrap();
		assert_eq!(notes.documents.len(), 2);
		assert_eq!(notes.documents[1].title, "Ideas");
	}

	#[test]
	fn the_listing_commands_refuse_a_relative_path() {
		let relative = PathBuf::from("some/where");
		assert!(matches!(
			list_documents(relative.clone()).unwrap_err(),
			Error::RelativePath
		));
		assert!(matches!(
			refresh_documents(relative).unwrap_err(),
			Error::RelativePath
		));
	}

	#[test]
	fn a_title_is_the_file_name_without_its_extension() {
		let document = Document::new("Manuscript", "Chapter 1.md");
		assert_eq!(document.title(), "Chapter 1");
		let view = DocumentView::from(&document);
		assert_eq!(view.title, "Chapter 1");
		assert_eq!(view.folder, "Manuscript");
	}

	/// Rewrites one document's recorded path, the way a hand-edited manifest
	/// could.
	fn set_document_path(root: &Path, index: usize, path: &str) {
		let file = root.join(MANIFEST_FILE);
		let mut value: serde_json::Value =
			serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
		value["documents"][index]["path"] = path.into();
		fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
	}

	fn first_document(root: &Path) -> Document {
		read_manifest(root).unwrap().documents[0].clone()
	}

	#[test]
	fn a_document_reads_back_what_was_written_to_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(
			root.join("Manuscript").join("Chapter 1.md"),
			"Sing to me of the man, Muse.",
		)
		.unwrap();

		let id = first_document(&root).id;
		assert_eq!(
			read_document(root, id).unwrap(),
			"Sing to me of the man, Muse."
		);
	}

	#[test]
	fn an_unknown_id_is_not_a_document() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = read_document(root, Uuid::new_v4()).unwrap_err();
		assert!(matches!(err, Error::UnknownDocument));
	}

	#[test]
	fn a_recorded_document_whose_file_has_gone_is_reported() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;
		fs::remove_file(root.join("Manuscript").join("Chapter 1.md")).unwrap();

		let err = read_document(root, id).unwrap_err();
		assert!(matches!(err, Error::DocumentMissing));
	}

	#[test]
	fn a_path_that_climbs_out_of_the_project_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(parent.path().join("secrets.md"), "not yours").unwrap();
		set_document_path(&root, 0, "../secrets.md");

		let id = first_document(&root).id;
		let err = read_document(root, id).unwrap_err();
		assert!(matches!(err, Error::OutsideProject));
	}

	#[cfg(unix)]
	#[test]
	fn a_symlink_out_of_the_project_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let outside = parent.path().join("secrets.md");
		fs::write(&outside, "not yours").unwrap();
		std::os::unix::fs::symlink(&outside, root.join("Manuscript").join("Link.md")).unwrap();
		set_document_path(&root, 0, "Manuscript/Link.md");

		let id = first_document(&root).id;
		let err = read_document(root, id).unwrap_err();
		assert!(matches!(err, Error::OutsideProject));
	}

	#[test]
	fn a_document_that_is_not_text_is_reported() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(root.join("Manuscript").join("Chapter 1.md"), [0xff, 0xfe]).unwrap();

		let id = first_document(&root).id;
		let err = read_document(root, id).unwrap_err();
		assert!(matches!(err, Error::NotText));
	}

	#[test]
	fn reading_refuses_a_relative_path() {
		let err = read_document(PathBuf::from("some/where"), Uuid::new_v4()).unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn what_is_written_is_what_is_read_back() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		write_document(root.clone(), id, "Sing to me of the man, Muse.".to_owned()).unwrap();
		assert_eq!(
			read_document(root, id).unwrap(),
			"Sing to me of the man, Muse."
		);
	}

	#[test]
	fn a_second_write_replaces_the_first_rather_than_adding_to_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		write_document(root.clone(), id, "a long first draft".to_owned()).unwrap();
		write_document(root.clone(), id, "short".to_owned()).unwrap();
		assert_eq!(read_document(root, id).unwrap(), "short");
	}

	#[test]
	fn writing_leaves_no_temporary_file_behind() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		write_document(root.clone(), id, "Chapter one.".to_owned()).unwrap();

		let left: Vec<_> = fs::read_dir(root.join("Manuscript"))
			.unwrap()
			.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
			.collect();
		assert_eq!(left, ["Chapter 1.md"]);
		assert_eq!(scan(&root, &novel_folders()).unwrap().len(), 5);
	}

	#[test]
	fn writing_to_an_unknown_id_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = write_document(root, Uuid::new_v4(), "lost".to_owned()).unwrap_err();
		assert!(matches!(err, Error::UnknownDocument));
	}

	#[test]
	fn writing_to_a_document_whose_file_has_gone_is_reported() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;
		fs::remove_file(root.join("Manuscript").join("Chapter 1.md")).unwrap();

		let err = write_document(root, id, "back again".to_owned()).unwrap_err();
		assert!(matches!(err, Error::DocumentMissing));
	}

	#[test]
	fn writing_outside_the_project_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let outside = parent.path().join("secrets.md");
		fs::write(&outside, "not yours").unwrap();
		set_document_path(&root, 0, "../secrets.md");

		let id = first_document(&root).id;
		let err = write_document(root, id, "overwritten".to_owned()).unwrap_err();
		assert!(matches!(err, Error::OutsideProject));
		assert_eq!(fs::read_to_string(&outside).unwrap(), "not yours");
	}

	#[test]
	fn writing_refuses_a_relative_path() {
		let err =
			write_document(PathBuf::from("some/where"), Uuid::new_v4(), String::new()).unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}
}
