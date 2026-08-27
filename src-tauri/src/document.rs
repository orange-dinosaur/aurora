use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::project::{
	Error, MANIFEST_FILE, Manifest, Result, read_manifest, validate_name, write_atomic, write_json,
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

/// Where a deleted document goes. It sits inside the project so that a move
/// into it is a rename rather than a copy, and `scan` never looks at it: it
/// only ever reads the section folders the manifest names.
pub const TRASH_DIR: &str = ".trash";

/// How much of a document's opening the overview carries. Long enough to
/// recognise a chapter by, short enough that a whole section stays small.
const EXCERPT_CHARS: usize = 240;

/// A document as the section overview shows it: the sidebar's view plus enough
/// of the file to tell one chapter from another without opening it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentSummary {
	#[serde(flatten)]
	pub document: DocumentView,
	pub words: usize,
	pub excerpt: String,
	#[serde(with = "time::serde::rfc3339::option")]
	pub modified: Option<OffsetDateTime>,
}

impl DocumentSummary {
	/// A card for a document whose file cannot be read.
	fn blank(document: &Document) -> Self {
		Self {
			document: document.into(),
			words: 0,
			excerpt: String::new(),
			modified: None,
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

/// The opening of a document collapsed onto one line and cut to something a
/// card can hold.
fn excerpt(text: &str) -> String {
	let mut excerpt = String::new();
	let mut length = 0;

	for word in text.split_whitespace() {
		let separator = usize::from(!excerpt.is_empty());
		let width = word.chars().count();

		if length + separator + width > EXCERPT_CHARS {
			// A first word longer than the whole excerpt still has to show
			// something.
			if excerpt.is_empty() {
				excerpt.extend(word.chars().take(EXCERPT_CHARS));
			}
			excerpt.push('\u{2026}');
			break;
		}

		if separator == 1 {
			excerpt.push(' ');
		}
		excerpt.push_str(word);
		length += separator + width;
	}

	excerpt
}

/// Everything the overview shows about one document, from a single open of its
/// file. A file that cannot be read gives a blank card rather than failing the
/// whole section.
fn summarise(document: &Document, path: &Path) -> DocumentSummary {
	let Ok(mut file) = fs::File::open(path) else {
		return DocumentSummary::blank(document);
	};

	let modified = file
		.metadata()
		.and_then(|metadata| metadata.modified())
		.ok()
		.map(OffsetDateTime::from);

	let mut bytes = Vec::new();
	let text = match file.read_to_end(&mut bytes) {
		Ok(_) => String::from_utf8(bytes).unwrap_or_default(),
		Err(_) => String::new(),
	};

	DocumentSummary {
		document: document.into(),
		words: text.split_whitespace().count(),
		excerpt: excerpt(&text),
		modified,
	}
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

/// Turns a path offered by the frontend into somewhere this project is willing
/// to put a document: one of the format's sections, and a Markdown file
/// directly inside it.
fn restore_path(manifest: &Manifest, root: &Path, path: &str) -> Result<PathBuf> {
	let Some((section, name)) = path.split_once('/') else {
		return Err(Error::BadDocumentPath);
	};

	let ordinary = |part: &str| {
		!part.is_empty()
			&& part != "."
			&& part != ".."
			&& !part.contains('/')
			&& !part.contains('\\')
	};

	if !ordinary(section) || !ordinary(name) || !is_markdown(name) {
		return Err(Error::BadDocumentPath);
	}
	if !manifest.folders.iter().any(|folder| folder == section) {
		return Err(Error::BadDocumentPath);
	}

	Ok(root.join(section).join(name))
}

/// Puts a document that has gone missing back on disk and brings the manifest
/// with it. The path comes from the tab that still holds the text rather than
/// from the manifest, since a refresh may already have dropped the document.
/// The document it returns is the one to carry on editing: the same id if the
/// manifest still knew the path, a fresh one if it had to be adopted again.
#[tauri::command]
pub fn restore_document(root: PathBuf, path: String, text: String) -> Result<DocumentView> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	add_document(&root, &manifest, &path, &text)
}

/// Puts a file that is not there yet at a relative path this project is willing
/// to accept, and brings the manifest with it. Shared by restoring a vanished
/// document and creating a new one: on disk the two are the same act, and the
/// document that comes back is the one to start editing.
fn add_document(root: &Path, manifest: &Manifest, path: &str, text: &str) -> Result<DocumentView> {
	let file = restore_path(manifest, root, path)?;
	if file.exists() {
		return Err(Error::DocumentExists);
	}

	let folder = file.parent().ok_or(Error::BadDocumentPath)?;
	fs::create_dir_all(folder)?;
	// A section folder can be a symlink the writer made, so where it actually
	// leads is what decides whether this write stays inside the project.
	if !folder.canonicalize()?.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}

	write_atomic(&file, text.as_bytes())?;

	refresh(root)?
		.documents
		.iter()
		.find(|d| d.path == path)
		.map(DocumentView::from)
		.ok_or(Error::UnknownDocument)
}

/// `.md` is Aurora's business rather than the writer's, so a name that already
/// carries it means the same thing as one that does not, and gets it taken off
/// rather than doubled.
fn without_extension(name: &str) -> &str {
	match name.rfind('.') {
		Some(dot) if name[dot..].eq_ignore_ascii_case(".md") => &name[..dot],
		_ => name,
	}
}

/// Starts a new, empty document in one of the project's sections. The name is
/// the title.
#[tauri::command]
pub fn create_document(root: PathBuf, section: String, name: String) -> Result<DocumentView> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let name = without_extension(&name);
	validate_name(name)?;

	let manifest = read_manifest(&root)?;
	if !manifest.folders.contains(&section) {
		return Err(Error::UnknownSection);
	}

	add_document(&root, &manifest, &format!("{section}/{name}.md"), "")
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

/// Gives a document a new title, which is to say a new file name. It stays in
/// its section, and keeps its id and its place in the order.
#[tauri::command]
pub fn rename_document(root: PathBuf, id: Uuid, name: String) -> Result<DocumentView> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let name = without_extension(&name);
	validate_name(name)?;

	let mut manifest = read_manifest(&root)?;
	let from = resolve(&manifest, &root, id)?;

	let index = manifest
		.documents
		.iter()
		.position(|d| d.id == id)
		.ok_or(Error::UnknownDocument)?;
	let section = section_of(&manifest.documents[index].path).ok_or(Error::BadDocumentPath)?;
	let path = format!("{section}/{name}.md");

	// Being renamed to what it is already called is not a collision with
	// itself.
	if path == manifest.documents[index].path {
		return Ok(DocumentView::from(&manifest.documents[index]));
	}

	let to = restore_path(&manifest, &root, &path)?;
	if to.exists() {
		return Err(Error::DocumentExists);
	}

	// The file moves first. If writing the manifest then fails, the next
	// refresh adopts the renamed file under a new id rather than losing it.
	fs::rename(&from, &to)?;

	manifest.documents[index].path = path;
	write_json(&root.join(MANIFEST_FILE), &manifest)?;
	Ok(DocumentView::from(&manifest.documents[index]))
}

/// The moment of a deletion, as the prefix on its file in the trash: sortable,
/// and made of nothing a filesystem objects to.
fn stamp(at: OffsetDateTime) -> String {
	format!(
		"{:04}{:02}{:02}-{:02}{:02}{:02}",
		at.year(),
		at.month() as u8,
		at.day(),
		at.hour(),
		at.minute(),
		at.second()
	)
}

/// Moves a document into the project's trash and drops it from the manifest.
/// The file keeps its name behind the moment it was deleted, so deleting two
/// documents called the same thing does not lose the first.
fn trash(root: &Path, id: Uuid, at: OffsetDateTime) -> Result<()> {
	let mut manifest = read_manifest(root)?;
	let from = resolve(&manifest, root, id)?;

	let index = manifest
		.documents
		.iter()
		.position(|d| d.id == id)
		.ok_or(Error::UnknownDocument)?;
	let (section, file_name) = manifest.documents[index]
		.path
		.split_once('/')
		.ok_or(Error::BadDocumentPath)?;

	let folder = root.join(TRASH_DIR).join(section);
	fs::create_dir_all(&folder)?;
	// The trash is an ordinary folder a writer can replace with a symlink, so
	// where it actually leads is what decides whether this move stays inside
	// the project.
	if !folder.canonicalize()?.starts_with(root.canonicalize()?) {
		return Err(Error::OutsideProject);
	}

	let at = stamp(at);
	let mut to = folder.join(format!("{at} {file_name}"));
	// Two deletions within the same second would otherwise write over each
	// other.
	let mut again = 1;
	while to.exists() {
		to = folder.join(format!("{at}-{again} {file_name}"));
		again += 1;
	}

	// The file moves first. If writing the manifest then fails, the next
	// refresh drops the document anyway, since its file is no longer in the
	// section.
	fs::rename(&from, &to)?;

	manifest.documents.remove(index);
	write_json(&root.join(MANIFEST_FILE), &manifest)
}

/// Deletes a document, which is to say puts it in the project's trash.
#[tauri::command]
pub fn delete_document(root: PathBuf, id: Uuid) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	trash(&root, id, OffsetDateTime::now_utc())
}

/// Every document in one section, with enough of each to recognise it. Reads
/// the manifest rather than the folder, the way `list_documents` does.
#[tauri::command]
pub fn section_overview(root: PathBuf, section: String) -> Result<Vec<DocumentSummary>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let manifest = read_manifest(&root)?;
	if !manifest.folders.contains(&section) {
		return Err(Error::UnknownSection);
	}

	Ok(manifest
		.documents
		.iter()
		.filter(|d| section_of(&d.path) == Some(section.as_str()))
		.map(|document| match resolve(&manifest, &root, document.id) {
			Ok(path) => summarise(document, &path),
			// A vanished file, or one the manifest points outside the project.
			Err(_) => DocumentSummary::blank(document),
		})
		.collect())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::project::{Format, NameError, create};
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

	/// A project whose first document has been deleted from disk.
	fn with_chapter_one_gone(parent: &tempfile::TempDir) -> (PathBuf, Uuid) {
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;
		fs::remove_file(root.join("Manuscript").join("Chapter 1.md")).unwrap();
		(root, id)
	}

	#[test]
	fn a_vanished_document_can_be_put_back() {
		let parent = tempfile::tempdir().unwrap();
		let (root, id) = with_chapter_one_gone(&parent);

		let restored = restore_document(
			root.clone(),
			"Manuscript/Chapter 1.md".to_owned(),
			"Sing to me of the man, Muse.".to_owned(),
		)
		.unwrap();

		assert_eq!(restored.id, id, "the manifest still knew the path");
		assert_eq!(restored.title, "Chapter 1");
		assert_eq!(
			read_document(root, restored.id).unwrap(),
			"Sing to me of the man, Muse."
		);
	}

	#[test]
	fn a_document_already_forgotten_comes_back_under_a_new_id() {
		let parent = tempfile::tempdir().unwrap();
		let (root, id) = with_chapter_one_gone(&parent);
		// A refresh while the file was away drops it from the manifest, which
		// is why the path has to come from the caller.
		refresh(&root).unwrap();

		let restored = restore_document(
			root.clone(),
			"Manuscript/Chapter 1.md".to_owned(),
			"Back again.".to_owned(),
		)
		.unwrap();

		assert_ne!(restored.id, id);
		assert_eq!(read_document(root, restored.id).unwrap(), "Back again.");
	}

	#[test]
	fn restoring_recreates_a_section_folder_that_went_with_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::remove_dir_all(root.join("Manuscript")).unwrap();

		let restored = restore_document(
			root.clone(),
			"Manuscript/Chapter 1.md".to_owned(),
			"Back again.".to_owned(),
		)
		.unwrap();

		assert_eq!(restored.folder, "Manuscript");
		assert_eq!(read_document(root, restored.id).unwrap(), "Back again.");
	}

	#[test]
	fn a_document_that_came_back_on_its_own_is_not_written_over() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(
			root.join("Manuscript").join("Chapter 1.md"),
			"what the sync client brought back",
		)
		.unwrap();

		let err = restore_document(
			root.clone(),
			"Manuscript/Chapter 1.md".to_owned(),
			"my copy".to_owned(),
		)
		.unwrap_err();

		assert!(matches!(err, Error::DocumentExists));
		assert_eq!(
			fs::read_to_string(root.join("Manuscript").join("Chapter 1.md")).unwrap(),
			"what the sync client brought back"
		);
	}

	#[test]
	fn restoring_refuses_a_path_that_is_not_a_document_in_a_section() {
		let parent = tempfile::tempdir().unwrap();
		let (root, _) = with_chapter_one_gone(&parent);

		for path in [
			"secrets.md",
			"../secrets.md",
			"Manuscript/../../secrets.md",
			"Scraps/Offcut.md",
			"Manuscript/notes.txt",
			"Manuscript/",
			"Manuscript/..",
		] {
			let err =
				restore_document(root.clone(), path.to_owned(), "text".to_owned()).unwrap_err();
			assert!(
				matches!(err, Error::BadDocumentPath),
				"{path} should not be a document path"
			);
		}

		assert!(!parent.path().join("secrets.md").exists());
	}

	#[cfg(unix)]
	#[test]
	fn restoring_through_a_symlinked_section_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let elsewhere = parent.path().join("elsewhere");
		fs::create_dir(&elsewhere).unwrap();
		fs::remove_dir_all(root.join("Notes")).unwrap();
		std::os::unix::fs::symlink(&elsewhere, root.join("Notes")).unwrap();

		let err =
			restore_document(root, "Notes/Notes.md".to_owned(), "text".to_owned()).unwrap_err();

		assert!(matches!(err, Error::OutsideProject));
		assert!(!elsewhere.join("Notes.md").exists());
	}

	#[test]
	fn restoring_refuses_a_relative_path() {
		let err = restore_document(
			PathBuf::from("some/where"),
			"Notes/Notes.md".to_owned(),
			String::new(),
		)
		.unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn an_overview_carries_the_opening_the_count_and_the_time() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(
			root.join("Manuscript").join("Chapter 1.md"),
			"Sing to me of the man, Muse.",
		)
		.unwrap();

		let overview = section_overview(root, "Manuscript".to_owned()).unwrap();

		assert_eq!(overview.len(), 1);
		assert_eq!(overview[0].document.title, "Chapter 1");
		assert_eq!(overview[0].excerpt, "Sing to me of the man, Muse.");
		assert_eq!(overview[0].words, 7);
		assert!(overview[0].modified.is_some());
	}

	#[test]
	fn an_overview_lists_a_section_in_the_manifests_order() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		for name in ["Chapter 3.md", "Chapter 2.md"] {
			fs::write(root.join("Manuscript").join(name), "").unwrap();
		}
		refresh(&root).unwrap();

		let overview = section_overview(root, "Manuscript".to_owned()).unwrap();

		let titles: Vec<_> = overview.iter().map(|d| d.document.title.as_str()).collect();
		assert_eq!(titles, ["Chapter 1", "Chapter 2", "Chapter 3"]);
	}

	#[test]
	fn an_overview_holds_only_its_own_section() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let overview = section_overview(root, "Notes".to_owned()).unwrap();

		assert_eq!(overview.len(), 1);
		assert_eq!(overview[0].document.folder, "Notes");
	}

	#[test]
	fn a_document_whose_file_has_gone_still_gets_a_card() {
		let parent = tempfile::tempdir().unwrap();
		let (root, id) = with_chapter_one_gone(&parent);

		let overview = section_overview(root, "Manuscript".to_owned()).unwrap();

		assert_eq!(overview.len(), 1);
		assert_eq!(overview[0].document.id, id);
		assert_eq!(overview[0].words, 0);
		assert_eq!(overview[0].excerpt, "");
		assert!(overview[0].modified.is_none());
	}

	#[test]
	fn an_overview_does_not_read_a_document_outside_the_project() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(parent.path().join("secrets.md"), "not yours").unwrap();
		set_document_path(&root, 0, "../secrets.md");

		let overview = section_overview(root, "Manuscript".to_owned()).unwrap();

		assert!(overview.is_empty(), "it is no longer in Manuscript");
	}

	#[test]
	fn an_overview_of_a_section_the_project_does_not_have_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = section_overview(root, "Scraps".to_owned()).unwrap_err();
		assert!(matches!(err, Error::UnknownSection));
	}

	#[test]
	fn an_overview_refuses_a_relative_path() {
		let err = section_overview(PathBuf::from("some/where"), "Notes".to_owned()).unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn a_summary_serializes_flat_alongside_the_document() {
		let document = Document::new("Manuscript", "Chapter 1.md");
		let summary = DocumentSummary {
			document: (&document).into(),
			words: 3,
			excerpt: "Sing to me".to_owned(),
			modified: Some(fixed_time()),
		};

		let json = serde_json::to_value(&summary).unwrap();
		assert_eq!(json["title"], "Chapter 1");
		assert_eq!(json["path"], "Manuscript/Chapter 1.md");
		assert_eq!(json["words"], 3);
		assert_eq!(json["modified"], "2023-11-14T22:13:20Z");
		assert!(json.get("document").is_none());
	}

	#[test]
	fn an_unreadable_summary_reports_no_time_at_all() {
		let document = Document::new("Manuscript", "Chapter 1.md");
		let json = serde_json::to_value(DocumentSummary::blank(&document)).unwrap();
		assert!(json["modified"].is_null());
	}

	#[test]
	fn an_excerpt_is_one_line_of_the_opening() {
		assert_eq!(
			excerpt("  Sing to me\n\nof the man,\tMuse.  "),
			"Sing to me of the man, Muse."
		);
		assert_eq!(excerpt(""), "");
	}

	#[test]
	fn a_long_excerpt_is_cut_on_a_word_boundary() {
		let text = "word ".repeat(200);
		let cut = excerpt(&text);

		assert!(cut.ends_with('\u{2026}'));
		assert!(cut.chars().count() <= EXCERPT_CHARS + 1);
		assert!(cut.trim_end_matches('\u{2026}').ends_with("word"));
	}

	#[test]
	fn a_single_endless_word_is_still_shown() {
		let cut = excerpt(&"a".repeat(EXCERPT_CHARS * 2));

		assert_eq!(cut.chars().count(), EXCERPT_CHARS + 1);
		assert!(cut.ends_with('\u{2026}'));
	}

	#[test]
	fn words_are_counted_across_lines() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(
			root.join("Notes").join("Notes.md"),
			"one two\nthree\n\n  four  ",
		)
		.unwrap();

		let overview = section_overview(root, "Notes".to_owned()).unwrap();
		assert_eq!(overview[0].words, 4);
	}

	#[test]
	fn a_new_document_starts_empty_at_the_end_of_its_section() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let created = create_document(
			root.clone(),
			"Manuscript".to_owned(),
			"Chapter 2".to_owned(),
		)
		.unwrap();

		assert_eq!(created.title, "Chapter 2");
		assert_eq!(created.folder, "Manuscript");
		assert_eq!(created.path, "Manuscript/Chapter 2.md");
		assert_eq!(read_document(root.clone(), created.id).unwrap(), "");

		let manuscript = sections(&read_manifest(&root).unwrap()).remove(0);
		let titles: Vec<_> = manuscript.documents.iter().map(|d| &d.title).collect();
		assert_eq!(titles, ["Chapter 1", "Chapter 2"]);
	}

	#[test]
	fn a_new_document_can_be_written_to_straight_away() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let created =
			create_document(root.clone(), "Notes".to_owned(), "Ideas".to_owned()).unwrap();
		write_document(root.clone(), created.id, "Begin in the middle.".to_owned()).unwrap();

		assert_eq!(
			read_document(root, created.id).unwrap(),
			"Begin in the middle."
		);
	}

	#[test]
	fn a_name_already_taken_in_that_section_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::write(
			root.join("Manuscript").join("Chapter 1.md"),
			"Sing to me of the man, Muse.",
		)
		.unwrap();

		let err = create_document(
			root.clone(),
			"Manuscript".to_owned(),
			"Chapter 1".to_owned(),
		)
		.unwrap_err();

		assert!(matches!(err, Error::DocumentExists));
		assert_eq!(
			fs::read_to_string(root.join("Manuscript").join("Chapter 1.md")).unwrap(),
			"Sing to me of the man, Muse.",
			"the document that was already there is untouched"
		);
	}

	#[test]
	fn the_same_name_in_another_section_is_fine() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		create_document(root.clone(), "Notes".to_owned(), "Chapter 1".to_owned()).unwrap();

		assert!(root.join("Notes").join("Chapter 1.md").exists());
	}

	#[test]
	fn a_name_the_filesystem_would_not_take_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		for name in [
			"",
			"  ",
			"Chapter/Two",
			"Chapter?",
			".hidden",
			"NUL",
			"Chapter ",
		] {
			let err = create_document(root.clone(), "Manuscript".to_owned(), name.to_owned())
				.unwrap_err();
			assert!(
				matches!(err, Error::InvalidName(_)),
				"{name:?} should not be a document name"
			);
		}

		assert_eq!(scan(&root, &novel_folders()).unwrap().len(), 5);
	}

	#[test]
	fn creating_in_a_section_the_project_does_not_have_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err =
			create_document(root.clone(), "Scraps".to_owned(), "Offcut".to_owned()).unwrap_err();

		assert!(matches!(err, Error::UnknownSection));
		assert!(!root.join("Scraps").exists());
	}

	#[test]
	fn creating_recreates_a_section_folder_that_has_gone() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::remove_dir_all(root.join("Manuscript")).unwrap();

		let created = create_document(
			root.clone(),
			"Manuscript".to_owned(),
			"Chapter 2".to_owned(),
		)
		.unwrap();

		assert_eq!(created.path, "Manuscript/Chapter 2.md");
		assert_eq!(read_document(root, created.id).unwrap(), "");
	}

	#[cfg(unix)]
	#[test]
	fn creating_through_a_symlinked_section_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let elsewhere = parent.path().join("elsewhere");
		fs::create_dir(&elsewhere).unwrap();
		fs::remove_dir_all(root.join("Notes")).unwrap();
		std::os::unix::fs::symlink(&elsewhere, root.join("Notes")).unwrap();

		let err = create_document(root, "Notes".to_owned(), "Ideas".to_owned()).unwrap_err();

		assert!(matches!(err, Error::OutsideProject));
		assert!(!elsewhere.join("Ideas.md").exists());
	}

	#[test]
	fn creating_refuses_a_relative_path() {
		let err = create_document(
			PathBuf::from("some/where"),
			"Notes".to_owned(),
			"Ideas".to_owned(),
		)
		.unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn a_name_that_already_carries_the_extension_does_not_double_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		for (typed, title) in [
			("Chapter 2.md", "Chapter 2"),
			("Chapter 3.MD", "Chapter 3"),
			("Chapter 4.markdown", "Chapter 4.markdown"),
			("Chapter 5", "Chapter 5"),
		] {
			let created =
				create_document(root.clone(), "Manuscript".to_owned(), typed.to_owned()).unwrap();
			assert_eq!(created.title, title, "typed {typed:?}");
			assert_eq!(created.path, format!("Manuscript/{title}.md"));
		}
	}

	#[test]
	fn a_name_that_is_nothing_but_the_extension_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = create_document(root.clone(), "Notes".to_owned(), ".md".to_owned()).unwrap_err();

		assert!(matches!(err, Error::InvalidName(NameError::Empty)));
		assert_eq!(scan(&root, &novel_folders()).unwrap().len(), 5);
	}

	#[test]
	fn a_renamed_document_keeps_its_id_its_text_and_its_place() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		create_document(
			root.clone(),
			"Manuscript".to_owned(),
			"Chapter 2".to_owned(),
		)
		.unwrap();
		let id = first_document(&root).id;
		write_document(root.clone(), id, "Sing to me of the man, Muse.".to_owned()).unwrap();

		let renamed = rename_document(root.clone(), id, "Ithaca Falls".to_owned()).unwrap();

		assert_eq!(renamed.id, id);
		assert_eq!(renamed.title, "Ithaca Falls");
		assert_eq!(renamed.folder, "Manuscript");
		assert_eq!(renamed.path, "Manuscript/Ithaca Falls.md");
		assert_eq!(
			read_document(root.clone(), id).unwrap(),
			"Sing to me of the man, Muse."
		);

		let manuscript = sections(&read_manifest(&root).unwrap()).remove(0);
		let titles: Vec<_> = manuscript.documents.iter().map(|d| &d.title).collect();
		assert_eq!(titles, ["Ithaca Falls", "Chapter 2"], "it did not move");
	}

	#[test]
	fn renaming_takes_the_old_file_with_it() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		rename_document(root.clone(), id, "Ithaca Falls".to_owned()).unwrap();

		assert!(!root.join("Manuscript").join("Chapter 1.md").exists());
		assert_eq!(
			scan(&root, &novel_folders()).unwrap()[0],
			"Manuscript/Ithaca Falls.md"
		);
	}

	#[test]
	fn renaming_to_the_name_it_already_has_changes_nothing() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		let renamed = rename_document(root.clone(), id, "Chapter 1".to_owned()).unwrap();

		assert_eq!(renamed.id, id);
		assert_eq!(renamed.path, "Manuscript/Chapter 1.md");
		assert!(root.join("Manuscript").join("Chapter 1.md").exists());
	}

	#[test]
	fn renaming_onto_another_document_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let other = create_document(
			root.clone(),
			"Manuscript".to_owned(),
			"Chapter 2".to_owned(),
		)
		.unwrap();
		write_document(root.clone(), other.id, "the second chapter".to_owned()).unwrap();
		let id = first_document(&root).id;

		let err = rename_document(root.clone(), id, "Chapter 2".to_owned()).unwrap_err();

		assert!(matches!(err, Error::DocumentExists));
		assert_eq!(
			read_document(root.clone(), other.id).unwrap(),
			"the second chapter",
			"the document that was already there is untouched"
		);
		assert_eq!(first_document(&root).path, "Manuscript/Chapter 1.md");
	}

	#[test]
	fn a_name_in_another_section_is_not_a_collision() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		let renamed = rename_document(root.clone(), id, "Notes".to_owned()).unwrap();

		assert_eq!(renamed.path, "Manuscript/Notes.md");
		assert!(root.join("Notes").join("Notes.md").exists());
	}

	#[test]
	fn renaming_drops_an_extension_the_writer_typed() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		let renamed = rename_document(root, id, "Ithaca Falls.md".to_owned()).unwrap();

		assert_eq!(renamed.title, "Ithaca Falls");
		assert_eq!(renamed.path, "Manuscript/Ithaca Falls.md");
	}

	#[test]
	fn renaming_to_a_name_the_filesystem_would_not_take_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		for name in ["", "  ", "Chapter/Two", "Chapter?", ".hidden", "NUL"] {
			let err = rename_document(root.clone(), id, name.to_owned()).unwrap_err();
			assert!(
				matches!(err, Error::InvalidName(_)),
				"{name:?} should not be a document name"
			);
		}

		assert_eq!(first_document(&root).path, "Manuscript/Chapter 1.md");
	}

	#[test]
	fn renaming_an_unknown_document_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = rename_document(root, Uuid::new_v4(), "Ithaca Falls".to_owned()).unwrap_err();
		assert!(matches!(err, Error::UnknownDocument));
	}

	#[test]
	fn renaming_a_document_whose_file_has_gone_is_reported() {
		let parent = tempfile::tempdir().unwrap();
		let (root, id) = with_chapter_one_gone(&parent);

		let err = rename_document(root, id, "Ithaca Falls".to_owned()).unwrap_err();
		assert!(matches!(err, Error::DocumentMissing));
	}

	#[test]
	fn renaming_refuses_a_relative_path() {
		let err = rename_document(
			PathBuf::from("some/where"),
			Uuid::new_v4(),
			"Ithaca Falls".to_owned(),
		)
		.unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	/// What is in the trash, as file names relative to `.trash/<section>`.
	fn trashed(root: &Path, section: &str) -> Vec<String> {
		let mut names: Vec<String> = fs::read_dir(root.join(TRASH_DIR).join(section))
			.unwrap()
			.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
			.collect();
		names.sort();
		names
	}

	#[test]
	fn a_deleted_document_keeps_its_text_in_the_trash() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;
		write_document(root.clone(), id, "Sing to me of the man, Muse.".to_owned()).unwrap();

		trash(&root, id, fixed_time()).unwrap();

		assert_eq!(
			trashed(&root, "Manuscript"),
			["20231114-221320 Chapter 1.md"]
		);
		assert_eq!(
			fs::read_to_string(
				root.join(TRASH_DIR)
					.join("Manuscript")
					.join("20231114-221320 Chapter 1.md")
			)
			.unwrap(),
			"Sing to me of the man, Muse."
		);
	}

	#[test]
	fn a_deleted_document_leaves_its_section_and_the_manifest() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		trash(&root, id, fixed_time()).unwrap();

		assert!(!root.join("Manuscript").join("Chapter 1.md").exists());
		assert!(
			!read_manifest(&root)
				.unwrap()
				.documents
				.iter()
				.any(|d| d.id == id)
		);
		let manuscript = sections(&read_manifest(&root).unwrap()).remove(0);
		assert!(manuscript.documents.is_empty());
	}

	#[test]
	fn the_trash_is_invisible_to_a_refresh() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let id = first_document(&root).id;

		trash(&root, id, fixed_time()).unwrap();
		refresh(&root).unwrap();

		assert_eq!(scan(&root, &novel_folders()).unwrap().len(), 4);
		assert_eq!(read_manifest(&root).unwrap().documents.len(), 4);
		assert!(trashed(&root, "Manuscript").len() == 1, "it is still there");
	}

	#[test]
	fn deleting_two_documents_of_the_same_name_keeps_both() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		for text in ["the first one", "the second one"] {
			let made =
				create_document(root.clone(), "Notes".to_owned(), "Ideas".to_owned()).unwrap();
			write_document(root.clone(), made.id, text.to_owned()).unwrap();
			// The same instant both times, which is what forces the collision.
			trash(&root, made.id, fixed_time()).unwrap();
		}

		assert_eq!(
			trashed(&root, "Notes"),
			["20231114-221320 Ideas.md", "20231114-221320-1 Ideas.md"]
		);
	}

	#[test]
	fn documents_deleted_from_different_sections_do_not_meet() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let notes =
			create_document(root.clone(), "Notes".to_owned(), "Chapter 1".to_owned()).unwrap();
		let manuscript = first_document(&root).id;

		trash(&root, manuscript, fixed_time()).unwrap();
		trash(&root, notes.id, fixed_time()).unwrap();

		assert_eq!(
			trashed(&root, "Manuscript"),
			["20231114-221320 Chapter 1.md"]
		);
		assert_eq!(trashed(&root, "Notes"), ["20231114-221320 Chapter 1.md"]);
	}

	#[test]
	fn deleting_an_unknown_document_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = delete_document(root.clone(), Uuid::new_v4()).unwrap_err();

		assert!(matches!(err, Error::UnknownDocument));
		assert!(!root.join(TRASH_DIR).exists());
	}

	#[test]
	fn deleting_a_document_whose_file_has_gone_is_reported() {
		let parent = tempfile::tempdir().unwrap();
		let (root, id) = with_chapter_one_gone(&parent);

		let err = delete_document(root, id).unwrap_err();
		assert!(matches!(err, Error::DocumentMissing));
	}

	#[cfg(unix)]
	#[test]
	fn a_trash_that_leads_out_of_the_project_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let elsewhere = parent.path().join("elsewhere");
		fs::create_dir(&elsewhere).unwrap();
		std::os::unix::fs::symlink(&elsewhere, root.join(TRASH_DIR)).unwrap();
		let id = first_document(&root).id;

		let err = trash(&root, id, fixed_time()).unwrap_err();

		assert!(matches!(err, Error::OutsideProject));
		assert!(root.join("Manuscript").join("Chapter 1.md").exists());
	}

	#[test]
	fn deleting_refuses_a_relative_path() {
		let err = delete_document(PathBuf::from("some/where"), Uuid::new_v4()).unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn a_stamp_is_sortable_and_holds_no_separators() {
		assert_eq!(stamp(fixed_time()), "20231114-221320");
		assert!(!stamp(fixed_time()).contains(' '));
	}

	#[test]
	fn writing_refuses_a_relative_path() {
		let err =
			write_document(PathBuf::from("some/where"), Uuid::new_v4(), String::new()).unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}
}
