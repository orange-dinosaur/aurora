use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tauri::{AppHandle, Manager};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::document::{Document, refresh};
use crate::store;
use crate::structure::{self, MAX_CHAPTERS, MAX_PARTS, MAX_SCENES, Start};
use crate::tree::{self, FolderKind, Node, tree_from_flat};
use crate::versions;

mod error;
mod manifest;
mod shelf;

pub use error::*;
pub use manifest::*;
pub use shelf::*;

/// The manifest file at the root of every project.
pub const MANIFEST_FILE: &str = "aurora.json";

/// Creates `parent/<name>/` with the format's section folders, a seed document
/// in the ones that ask for it, and the manifest. Returns the new project's
/// root.
pub fn create(
	parent: &Path,
	name: &str,
	format: Format,
	created_at: OffsetDateTime,
) -> Result<PathBuf> {
	create_from(parent, name, format, None, created_at)
}

/// As `create`, but a start lays out the Manuscript's parts, chapters and
/// scenes in place of its single seed.
pub fn create_from(
	parent: &Path,
	name: &str,
	format: Format,
	start: Option<&Start>,
	created_at: OffsetDateTime,
) -> Result<PathBuf> {
	validate_name(name)?;

	let sections = format.layout();
	if sections.is_empty() {
		return Err(Error::UnsupportedFormat(format));
	}
	if start.is_some_and(|start| !start.in_range()) {
		return Err(Error::StartOutOfRange);
	}

	// The projects folder is only a setting until its first project goes in.
	fs::create_dir_all(parent)?;
	let root = parent.join(name);
	match fs::create_dir(&root) {
		Ok(()) => {}
		Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Err(Error::AlreadyExists),
		Err(e) => return Err(e.into()),
	}

	// The folder did not exist a moment ago, so undoing our own work is safe.
	match fill(&root, name, format, sections, start, created_at) {
		Ok(()) => Ok(root),
		Err(e) => {
			let _ = fs::remove_dir_all(&root);
			Err(e)
		}
	}
}

fn fill(
	root: &Path,
	name: &str,
	format: Format,
	sections: &[Section],
	start: Option<&Start>,
	created_at: OffsetDateTime,
) -> Result<()> {
	let mut nodes = Vec::new();
	for section in sections {
		let children = match start {
			Some(start) if section.folder == MANUSCRIPT => structure::manuscript(start),
			_ => section.seed.map(Node::document).into_iter().collect(),
		};
		let folder = root.join(section.folder);
		fs::create_dir(&folder)?;
		lay_down(&folder, &children)?;
		if let (Some(start), Some(seed @ "Outline.md")) = (start, section.seed) {
			fs::write(folder.join(seed), structure::outline(start))?;
		}
		nodes.push(Node::folder(section.folder, children));
	}

	let mut manifest = Manifest::new(name, format, created_at);
	manifest.nodes = nodes;
	fs::write(root.join(MANIFEST_FILE), to_json(&manifest)?)?;
	Ok(())
}

/// Makes on disk what the nodes describe: a folder for each folder and an
/// empty file for each document.
fn lay_down(dir: &Path, nodes: &[Node]) -> Result<()> {
	for node in nodes {
		let path = dir.join(node.name());
		match node {
			Node::Folder { children, .. } => {
				fs::create_dir(&path)?;
				lay_down(&path, children)?;
			}
			Node::Document { .. } => fs::write(&path, "")?,
		}
	}
	Ok(())
}

/// The store an installed Aurora keeps its recent projects in.
const STORE_FILE: &str = "store.json";

/// A dev build writes beside the installed one instead of into it, so a day of
/// `pnpm tauri dev` cannot disturb the real list of projects.
const DEV_STORE_FILE: &str = "store-dev.json";

fn store_file() -> &'static str {
	if cfg!(debug_assertions) {
		DEV_STORE_FILE
	} else {
		STORE_FILE
	}
}

pub(crate) fn store_path(app: &AppHandle) -> Result<PathBuf> {
	app.path()
		.app_config_dir()
		.map(|dir| dir.join(store_file()))
		.map_err(|_| Error::NoConfigDir)
}

/// Start a dev store off as a copy of the installed build's, so a dev window
/// opens with the same recent projects. Once the dev store exists it goes its
/// own way and is never overwritten.
#[cfg(debug_assertions)]
fn copy_when_missing(from: &Path, to: &Path) -> Result<()> {
	if to.exists() || !from.is_file() {
		return Ok(());
	}
	if let Some(parent) = to.parent() {
		fs::create_dir_all(parent)?;
	}
	fs::copy(from, to)?;
	Ok(())
}

/// Seed this dev build's store from the installed build's, if there is one.
#[cfg(debug_assertions)]
pub fn seed_dev_store(app: &AppHandle) -> Result<()> {
	let dir = app
		.path()
		.app_config_dir()
		.map_err(|_| Error::NoConfigDir)?;
	copy_when_missing(&dir.join(STORE_FILE), &dir.join(DEV_STORE_FILE))
}

fn create_and_remember(
	store_path: &Path,
	parent: &Path,
	name: &str,
	format: Format,
	start: Option<&Start>,
	at: OffsetDateTime,
) -> Result<PathBuf> {
	// The path arrives from the frontend, so it is not trusted to be sensible.
	if !parent.is_absolute() {
		return Err(Error::RelativePath);
	}

	let root = create_from(parent, name, format, start, at)?;
	store::update(store_path, |store| store.remember(name, root.clone(), at))?;
	Ok(root)
}

/// Drops what the store keeps for folders that no longer exist. Saves only
/// when something went.
fn prune_gone(store_path: &Path) -> Result<()> {
	store::update(store_path, |store| {
		store.prune(Path::exists);
	})
}

fn forget(store_path: &Path, root: &Path) -> Result<()> {
	store::update(store_path, |store| store.forget(root))
}

fn close(store_path: &Path) -> Result<()> {
	store::update(store_path, store::Store::close)
}

#[tauri::command]
pub fn create_project(
	app: AppHandle,
	parent: PathBuf,
	name: String,
	format: Format,
	start: Option<Start>,
) -> Result<PathBuf> {
	create_and_remember(
		&store_path(&app)?,
		&parent,
		&name,
		format,
		start.as_ref(),
		OffsetDateTime::now_utc(),
	)
}

/// A project as the frontend refers to it once it is open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OpenedProject {
	pub name: String,
	pub root: PathBuf,
	/// The folder of a git repository the project sits inside, until the
	/// writer has been told once that the project keeps its own versions.
	pub enclosing_repository: Option<PathBuf>,
}

fn open_and_remember(store_path: &Path, root: &Path, at: OffsetDateTime) -> Result<OpenedProject> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	// The manifest holds the name, not the folder, so a renamed folder still
	// opens under the name the writer gave it.
	let manifest = refresh(root)?;
	// Every way into a project comes through here, a new one included.
	let enclosing_repository = versions::ensure_repository(root)?;
	store::update(store_path, |store| {
		store.remember(manifest.name.clone(), root.to_path_buf(), at)
	})?;

	Ok(OpenedProject {
		name: manifest.name,
		root: root.to_path_buf(),
		enclosing_repository,
	})
}

#[tauri::command]
pub fn open_project(app: AppHandle, root: PathBuf) -> Result<OpenedProject> {
	open_and_remember(&store_path(&app)?, &root, OffsetDateTime::now_utc())
}

#[tauri::command]
pub fn forget_project(app: AppHandle, root: PathBuf) -> Result<()> {
	forget(&store_path(&app)?, &root)
}

/// Moves the whole project folder to the desktop's trash and takes it off the
/// recent list. Nothing here unlinks anything: the folder is recoverable from
/// the trash until the writer empties it.
#[tauri::command]
pub fn trash_project(app: AppHandle, root: PathBuf) -> Result<()> {
	let store = store_path(&app)?;
	require_remembered(&store, &root)?;
	move_to_trash(&root)?;
	forget(&store, &root)
}

/// Only a folder on the recent list or a project in the projects folder. A
/// path that arrives from anywhere else must not be able to bin a folder on the
/// strength of being asked to. This asks the list rather than the manifest,
/// because a project that cannot be read still has to be deletable.
fn require_remembered(store_path: &Path, root: &Path) -> Result<()> {
	let store = store::load(store_path)?;
	let in_folder = projects_folder(&store).is_some_and(|folder| {
		folder_books(&folder)
			.iter()
			.any(|book| same_file(book, root))
	});
	if in_folder || store.recent.iter().any(|project| project.root == root) {
		Ok(())
	} else {
		Err(Error::NotAProject)
	}
}

fn move_to_trash(root: &Path) -> Result<()> {
	#[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
	let mut context = trash::TrashContext::new();
	// The crate's default is an AppleScript to Finder, which refuses a folder
	// holding anything it thinks has to be downloaded first, and can stop to
	// ask for permission to control Finder.
	#[cfg(target_os = "macos")]
	{
		use trash::macos::{DeleteMethod, TrashContextExtMacos};
		context.set_delete_method(DeleteMethod::NsFileManager);
	}
	context.delete(root).map_err(Error::Trash)
}

/// The writer is done with the open project for now. It stays in the recent
/// list, but Aurora starts on the welcome screen next time.
#[tauri::command]
pub fn close_project(app: AppHandle) -> Result<()> {
	close(&store_path(&app)?)
}

/// What the project's book says about itself.
///
/// A project written before books existed grows one on the way past, and this
/// saves it. Reading does not usually rewrite the manifest, but the identifier
/// is generated fresh every time it is missing, so leaving it unsaved would
/// give the same project a different identity on every read.
#[tauri::command]
pub fn read_book(root: PathBuf) -> Result<Book> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let mut manifest = read_manifest(&root)?;
	if manifest.version < MANIFEST_VERSION {
		write_manifest(&root, &mut manifest)?;
	}
	Ok(manifest.book)
}

/// Replaces everything the writer has said about the book, which arrives whole
/// because the page that sends it holds the whole of it. The identifier is not
/// theirs to change and is kept from the manifest on disk, as is the cover,
/// which `set_cover` owns and a page holding an older copy would write back.
#[tauri::command]
pub fn write_book(root: PathBuf, book: Book) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let mut manifest = read_manifest(&root)?;
	manifest.book = Book {
		identifier: manifest.book.identifier,
		cover: std::mem::take(&mut manifest.book.cover),
		..book
	};
	write_manifest(&root, &mut manifest)
}

/// What a cover copy can be called. The extension is the one the writer's file
/// had, so the copy is what it claims to be.
pub(crate) const COVER_NAMES: [&str; 2] = ["cover.png", "cover.jpg"];

/// Takes the image the writer chose into the project and records it on the
/// book. The project keeps its own copy beside the manifest, so the cover
/// travels with the folder and nothing breaks when the original is moved,
/// renamed or thrown away.
#[tauri::command]
pub fn set_cover(root: PathBuf, source: PathBuf) -> Result<String> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let name = cover_name(&source)?;
	let mut manifest = read_manifest(&root)?;
	let destination = root.join(name);

	// Choosing the project's own copy again is a no-op, not a file copied over
	// itself, which would leave nothing behind.
	if !same_file(&source, &destination) {
		write_atomic(&destination, &fs::read(&source)?)?;
		// A JPEG replacing a PNG would otherwise leave the old copy in the
		// project folder for good.
		for old in COVER_NAMES {
			if old != name {
				remove_if_there(&root.join(old))?;
			}
		}
	}

	manifest.book.cover = name.to_owned();
	write_manifest(&root, &mut manifest)?;
	Ok(name.to_owned())
}

/// The bytes of the project's cover, for the page to show. They come through a
/// command rather than the asset protocol, which would need a path scope wide
/// enough to reach wherever the writer keeps their projects.
#[tauri::command]
pub fn read_cover(root: PathBuf) -> Result<tauri::ipc::Response> {
	// Raw bytes rather than the default JSON, which would send an image across
	// as a list of several million numbers.
	Ok(tauri::ipc::Response::new(cover_bytes(&root)?))
}

/// The cover image as the project keeps it.
pub fn cover_bytes(root: &Path) -> Result<Vec<u8>> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let name = read_manifest(root)?.book.cover;
	if name.is_empty() {
		return Err(Error::CoverMissing);
	}
	if !cover_stays_inside(&name) {
		return Err(Error::OutsideProject);
	}

	fs::read(root.join(&name)).map_err(|e| match e.kind() {
		io::ErrorKind::NotFound => Error::CoverMissing,
		_ => Error::Io(e),
	})
}

/// Forgets the cover and takes the project's copy of it away with it.
#[tauri::command]
pub fn clear_cover(root: PathBuf) -> Result<()> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}

	let mut manifest = read_manifest(&root)?;
	manifest.book.cover = String::new();
	write_manifest(&root, &mut manifest)?;

	for name in COVER_NAMES {
		remove_if_there(&root.join(name))?;
	}
	Ok(())
}

/// What the copy inside the project is called, from the extension of the file
/// the writer chose. Anything that is not a PNG or a JPEG is refused here,
/// before it has been copied anywhere.
fn cover_name(source: &Path) -> Result<&'static str> {
	let extension = source
		.extension()
		.and_then(|e| e.to_str())
		.unwrap_or_default()
		.to_ascii_lowercase();

	match extension.as_str() {
		"png" => Ok("cover.png"),
		"jpg" | "jpeg" => Ok("cover.jpg"),
		_ => Err(Error::NotAnImage),
	}
}

/// Whether two paths lead to the same file on disk. A path that leads nowhere
/// is not the same as anything, including another path that leads nowhere.
fn same_file(one: &Path, other: &Path) -> bool {
	match (fs::canonicalize(one), fs::canonicalize(other)) {
		(Ok(one), Ok(other)) => one == other,
		_ => false,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::structure::Structure;
	use std::collections::HashSet;
	use uuid::Uuid;

	/// Every test but the structure ones makes today's single-scene project.
	fn create_and_remember(
		store_path: &Path,
		parent: &Path,
		name: &str,
		format: Format,
		at: OffsetDateTime,
	) -> Result<PathBuf> {
		super::create_and_remember(store_path, parent, name, format, None, at)
	}

	fn start(structure: Structure, chapters: u32, scenes: u32) -> Start {
		Start {
			structure,
			parts: 0,
			chapters,
			scenes,
			front_matter: false,
			back_matter: false,
		}
	}

	fn manuscript_paths(root: &Path) -> Vec<String> {
		let json = fs::read_to_string(root.join(MANIFEST_FILE)).unwrap();
		let manifest: Manifest = serde_json::from_str(&json).unwrap();
		paths_of(&manifest)
			.into_iter()
			.filter(|path| path.starts_with("Manuscript/"))
			.collect()
	}

	#[test]
	fn three_acts_lay_out_parts_chapters_and_scenes_in_order() {
		let parent = tempfile::tempdir().unwrap();
		let three_acts = start(Structure::ThreeActs, 25, 2);
		let root = create_from(
			parent.path(),
			"Ithaca",
			Format::Novel,
			Some(&three_acts),
			fixed_time(),
		)
		.unwrap();

		let mut expected = Vec::new();
		let mut chapter = 0;
		for (part, count) in [("Act One", 6), ("Act Two", 13), ("Act Three", 6)] {
			for _ in 0..count {
				chapter += 1;
				for scene in 1..=2 {
					expected.push(format!(
						"Manuscript/{part}/Chapter {chapter}/Scene {scene}.md"
					));
				}
			}
		}
		assert_eq!(manuscript_paths(&root), expected);
		assert!(expected.contains(&"Manuscript/Act Two/Chapter 13/Scene 2.md".to_owned()));
		for path in &expected {
			assert!(root.join(path).is_file(), "{path} is missing on disk");
		}
		assert!(!root.join("Manuscript/Scene 1.md").exists());
	}

	#[test]
	fn no_scenes_make_each_chapter_one_document() {
		let parent = tempfile::tempdir().unwrap();
		let three_acts = start(Structure::ThreeActs, 24, 0);
		let root = create_from(
			parent.path(),
			"Ithaca",
			Format::Novel,
			Some(&three_acts),
			fixed_time(),
		)
		.unwrap();

		let paths = manuscript_paths(&root);
		assert_eq!(paths.len(), 24);
		assert_eq!(paths[0], "Manuscript/Act One/Chapter 1.md");
		assert!(root.join("Manuscript/Act One/Chapter 1.md").is_file());
	}

	/// The Manuscript's direct children as the manifest records them: each
	/// one's name, kind and, for a folder, its children's names.
	fn manuscript_top(root: &Path) -> Vec<(String, Option<FolderKind>, Vec<String>)> {
		let json = fs::read_to_string(root.join(MANIFEST_FILE)).unwrap();
		let manifest: Manifest = serde_json::from_str(&json).unwrap();
		let Some(Node::Folder { children, .. }) =
			manifest.nodes.iter().find(|node| node.name() == MANUSCRIPT)
		else {
			panic!("no Manuscript");
		};
		children
			.iter()
			.map(|node| match node {
				Node::Folder {
					name,
					kind,
					children,
					..
				} => (
					name.clone(),
					*kind,
					children.iter().map(|c| c.name().to_owned()).collect(),
				),
				Node::Document { name, .. } => (name.clone(), None, Vec::new()),
			})
			.collect()
	}

	#[test]
	fn matter_comes_first_and_last_when_asked() {
		let parent = tempfile::tempdir().unwrap();
		let both = Start {
			front_matter: true,
			back_matter: true,
			..start(Structure::FourParts, 8, 1)
		};
		let root = create_from(
			parent.path(),
			"Ithaca",
			Format::Novel,
			Some(&both),
			fixed_time(),
		)
		.unwrap();

		let top = manuscript_top(&root);
		assert_eq!(top.len(), 6);
		assert_eq!(
			top[0],
			(
				FRONT_MATTER.to_owned(),
				Some(FolderKind::FrontMatter),
				vec!["Dedication.md".to_owned(), "Epigraph.md".to_owned()]
			)
		);
		assert_eq!(
			top[5],
			(
				BACK_MATTER.to_owned(),
				Some(FolderKind::BackMatter),
				vec![
					"Acknowledgements.md".to_owned(),
					"About the author.md".to_owned()
				]
			)
		);
		for document in [
			"Front Matter/Dedication.md",
			"Front Matter/Epigraph.md",
			"Back Matter/Acknowledgements.md",
			"Back Matter/About the author.md",
		] {
			assert!(root.join(MANUSCRIPT).join(document).is_file());
			validate_name(document.rsplit('/').next().unwrap()).unwrap();
		}
	}

	#[test]
	fn no_matter_unless_asked() {
		let parent = tempfile::tempdir().unwrap();
		let four_parts = start(Structure::FourParts, 8, 1);
		let root = create_from(
			parent.path(),
			"Ithaca",
			Format::Novel,
			Some(&four_parts),
			fixed_time(),
		)
		.unwrap();

		let top = manuscript_top(&root);
		assert_eq!(top.len(), 4);
		assert!(
			top.iter()
				.all(|(_, kind, _)| *kind == Some(FolderKind::Part))
		);
		assert!(!root.join(MANUSCRIPT).join(FRONT_MATTER).exists());
		assert!(!root.join(MANUSCRIPT).join(BACK_MATTER).exists());
	}

	fn outline_of(structure: Structure, parts: u32) -> String {
		let parent = tempfile::tempdir().unwrap();
		let chosen = Start {
			parts,
			..start(structure, 20, 2)
		};
		let root = create_from(
			parent.path(),
			"Ithaca",
			Format::Novel,
			Some(&chosen),
			fixed_time(),
		)
		.unwrap();
		fs::read_to_string(root.join("Outline").join("Outline.md")).unwrap()
	}

	#[test]
	fn save_the_cat_writes_its_beats_into_the_outline() {
		let outline = outline_of(Structure::SaveTheCat, 0);
		let headings: Vec<&str> = outline.lines().filter(|l| l.starts_with("## ")).collect();
		assert_eq!(headings, ["## Act One", "## Act Two", "## Act Three"]);
		assert_eq!(
			outline.lines().filter(|l| l.starts_with("- **")).count(),
			15
		);
		assert!(outline.starts_with(
			"## Act One\n\n- **Opening image.** A picture of the hero's life before anything \
			 changes.\n"
		));
	}

	#[test]
	fn your_own_leaves_the_outline_empty() {
		assert_eq!(outline_of(Structure::YourOwn, 3), "");
	}

	#[test]
	fn a_start_out_of_range_writes_nothing() {
		let parent = tempfile::tempdir().unwrap();
		let too_many = start(Structure::ThreeActs, 24, 21);
		let err = create_from(
			parent.path(),
			"Ithaca",
			Format::Novel,
			Some(&too_many),
			fixed_time(),
		)
		.unwrap_err();

		assert!(matches!(err, Error::StartOutOfRange));
		assert!(!parent.path().join("Ithaca").exists());
	}

	fn paths_of(manifest: &Manifest) -> Vec<String> {
		manifest
			.documents()
			.into_iter()
			.map(|document| document.path)
			.collect()
	}

	#[test]
	fn format_names_match_the_frontend() {
		assert_eq!(
			serde_json::to_string(&Format::ShortStories).unwrap(),
			"\"short-stories\""
		);
		assert_eq!(
			serde_json::from_str::<Format>("\"stage-play\"").unwrap(),
			Format::StagePlay
		);
	}

	#[test]
	fn unknown_format_is_rejected() {
		assert!(serde_json::from_str::<Format>("\"poetry\"").is_err());
	}

	#[test]
	fn novel_has_the_five_sections() {
		let folders: Vec<_> = Format::Novel.layout().iter().map(|s| s.folder).collect();
		assert_eq!(
			folders,
			["Manuscript", "Outline", "Characters", "Locations", "Notes"]
		);
	}

	#[test]
	fn a_section_that_seeds_seeds_a_markdown_file() {
		for section in Format::Novel.layout() {
			if let Some(seed) = section.seed {
				assert!(seed.ends_with(".md"), "{} seeds {}", section.folder, seed);
			}
		}
	}

	#[test]
	fn the_subject_sections_start_empty() {
		let empty: Vec<_> = Format::Novel
			.layout()
			.iter()
			.filter(|s| s.seed.is_none())
			.map(|s| s.folder)
			.collect();
		assert_eq!(empty, ["Characters", "Locations"]);
	}

	#[test]
	fn the_other_formats_have_no_layout_yet() {
		for format in [Format::Screenplay, Format::ShortStories, Format::StagePlay] {
			assert!(
				format.layout().is_empty(),
				"{format:?} should not be creatable"
			);
		}
	}

	#[test]
	fn every_format_is_offered_in_order() {
		let formats: Vec<_> = format_layouts().into_iter().map(|f| f.format).collect();
		assert_eq!(formats, Format::ALL);
	}

	#[test]
	fn the_layouts_match_the_formats() {
		let layouts = format_layouts();
		let novel = layouts.iter().find(|f| f.format == Format::Novel).unwrap();
		assert!(novel.available);
		assert_eq!(
			novel.folders,
			["Manuscript", "Outline", "Characters", "Locations", "Notes"]
		);

		for layout in layouts.iter().filter(|f| f.format != Format::Novel) {
			assert!(layout.folders.is_empty());
			assert!(!layout.available);
		}
	}

	#[test]
	fn a_layout_crosses_with_the_format_name_the_frontend_uses() {
		let layouts = format_layouts();
		let stage_play = layouts.last().unwrap();
		assert_eq!(
			serde_json::to_value(stage_play).unwrap(),
			serde_json::json!({
				"format": "stage-play",
				"folders": [],
				"available": false,
			})
		);
	}

	fn fixed_time() -> OffsetDateTime {
		OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
	}

	#[test]
	fn manifest_takes_its_folders_from_the_layout() {
		let manifest = Manifest::new("Wuthering Heights", Format::Novel, fixed_time());
		assert_eq!(manifest.version, MANIFEST_VERSION);
		assert_eq!(
			manifest.folders(),
			["Manuscript", "Outline", "Characters", "Locations", "Notes"]
		);
	}

	#[test]
	fn manifest_drops_sub_second_precision() {
		let precise = OffsetDateTime::from_unix_timestamp_nanos(1_700_000_000_297_374_168).unwrap();
		let manifest = Manifest::new("Ithaca", Format::Novel, precise);
		assert_eq!(manifest.created_at, fixed_time());
	}

	#[test]
	fn manifest_serializes_with_camel_case_keys() {
		let manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());
		let json = serde_json::to_value(&manifest).unwrap();
		assert_eq!(json["version"], MANIFEST_VERSION);
		assert_eq!(json["name"], "Ithaca");
		assert_eq!(json["format"], "novel");
		assert_eq!(json["createdAt"], "2023-11-14T22:13:20Z");
		assert!(json.get("created_at").is_none());
	}

	#[test]
	fn manifest_round_trips() {
		let manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());
		let json = serde_json::to_string(&manifest).unwrap();
		assert_eq!(serde_json::from_str::<Manifest>(&json).unwrap(), manifest);
	}

	#[test]
	fn a_new_project_has_a_book_named_after_it() {
		let manifest = Manifest::new("Ithaca", Format::Novel, fixed_time());
		assert_eq!(manifest.book.title, "Ithaca");
		assert_eq!(manifest.book.author, "");
		assert!(manifest.book.contributors.is_empty());
	}

	#[test]
	fn two_projects_are_two_books() {
		let one = Manifest::new("Ithaca", Format::Novel, fixed_time());
		let other = Manifest::new("Ithaca", Format::Novel, fixed_time());
		assert_ne!(one.book.identifier, other.book.identifier);
	}

	/// A project written by the Aurora before books existed: a tree, but nothing
	/// said about what the tree adds up to.
	fn version_three_project(dir: &Path) {
		fs::write(
			dir.join(MANIFEST_FILE),
			format!(
				r#"{{"version":3,"name":"Ithaca","format":"novel",
				   "createdAt":"2023-11-14T22:13:20Z",
				   "nodes":[
				     {{"node":"folder","id":"{}","name":"Manuscript","children":[]}}
				   ]}}"#,
				Uuid::new_v4()
			),
		)
		.unwrap();
	}

	#[test]
	fn a_version_three_manifest_gains_a_book() {
		let dir = tempfile::tempdir().unwrap();
		version_three_project(dir.path());

		let manifest = read_manifest(dir.path()).unwrap();

		assert_eq!(manifest.folders(), ["Manuscript"]);
		assert_eq!(
			manifest.book.title, "Ithaca",
			"a project that never had a book is one about itself"
		);
	}

	#[test]
	fn reading_a_book_settles_its_identifier() {
		let dir = tempfile::tempdir().unwrap();
		version_three_project(dir.path());
		let root = dir.path().to_path_buf();

		let first = read_book(root.clone()).unwrap();

		let json = fs::read_to_string(root.join(MANIFEST_FILE)).unwrap();
		assert!(
			json.contains("\"identifier\""),
			"the book a version three project grew is saved, not grown again"
		);
		assert_eq!(read_book(root).unwrap().identifier, first.identifier);
	}

	#[test]
	fn the_identifier_survives_two_writes() {
		let dir = tempfile::tempdir().unwrap();
		version_three_project(dir.path());
		let root = dir.path().to_path_buf();

		let first = read_book(root.clone()).unwrap();
		write_book(
			root.clone(),
			Book {
				author: "Homer".to_owned(),
				..first
			},
		)
		.unwrap();
		let settled = read_book(root.clone()).unwrap().identifier;

		write_book(
			root.clone(),
			Book {
				title: "Odyssey".to_owned(),
				// The writer's own guess at an identifier is ignored.
				identifier: Uuid::new_v4(),
				..read_book(root.clone()).unwrap()
			},
		)
		.unwrap();

		let book = read_book(root).unwrap();
		assert_eq!(book.identifier, settled);
		assert_eq!(book.title, "Odyssey");
		assert_eq!(book.author, "Homer", "and the rest is left as it was");
	}

	#[test]
	fn the_export_ticks_are_remembered() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		write_book(
			root.clone(),
			Book {
				export_formats: vec![ExportFormat::Markdown],
				..read_book(root.clone()).unwrap()
			},
		)
		.unwrap();

		assert_eq!(
			read_book(root).unwrap().export_formats,
			vec![ExportFormat::Markdown]
		);
	}

	#[test]
	fn writing_a_book_leaves_the_project_alone() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let before = read_manifest(&root).unwrap();

		write_book(
			root.clone(),
			Book {
				keywords: vec!["myth".to_owned(), "sea".to_owned()],
				..read_book(root.clone()).unwrap()
			},
		)
		.unwrap();

		let after = read_manifest(&root).unwrap();
		assert_eq!(after.nodes, before.nodes);
		assert_eq!(after.name, before.name);
		assert_eq!(after.book.keywords, ["myth", "sea"]);
	}

	#[test]
	fn the_book_refuses_a_relative_path() {
		let relative = PathBuf::from("some/where");
		assert!(matches!(
			read_book(relative.clone()).unwrap_err(),
			Error::RelativePath
		));
		assert!(matches!(
			write_book(relative, Book::new("Ithaca")).unwrap_err(),
			Error::RelativePath
		));
	}

	/// A file of the writer's, somewhere that is not the project.
	fn picture(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
		let path = dir.join(name);
		fs::write(&path, bytes).unwrap();
		path
	}

	#[test]
	fn a_cover_is_copied_into_the_project() {
		let elsewhere = tempfile::tempdir().unwrap();
		let dir = tempfile::tempdir().unwrap();
		version_three_project(dir.path());
		let root = dir.path().to_path_buf();
		let chosen = picture(elsewhere.path(), "seascape.PNG", b"one");

		let name = set_cover(root.clone(), chosen.clone()).unwrap();

		assert_eq!(name, "cover.png", "the extension is kept, in lower case");
		assert_eq!(read_book(root.clone()).unwrap().cover, "cover.png");
		assert_eq!(cover_bytes(&root).unwrap(), b"one");

		// The writer's own file is theirs to do what they like with.
		fs::remove_file(&chosen).unwrap();
		assert_eq!(cover_bytes(&root).unwrap(), b"one");
	}

	#[test]
	fn a_second_cover_replaces_the_first() {
		let elsewhere = tempfile::tempdir().unwrap();
		let dir = tempfile::tempdir().unwrap();
		version_three_project(dir.path());
		let root = dir.path().to_path_buf();

		set_cover(root.clone(), picture(elsewhere.path(), "first.png", b"one")).unwrap();
		let name = set_cover(
			root.clone(),
			picture(elsewhere.path(), "second.jpeg", b"two"),
		)
		.unwrap();

		assert_eq!(
			name, "cover.jpg",
			"a JPEG is a JPEG whichever way it is spelt"
		);
		assert_eq!(cover_bytes(&root).unwrap(), b"two");
		assert!(
			!root.join("cover.png").exists(),
			"the copy that is no longer the cover does not stay in the project"
		);
	}

	#[test]
	fn choosing_the_projects_own_copy_keeps_it() {
		let dir = tempfile::tempdir().unwrap();
		version_three_project(dir.path());
		let root = dir.path().to_path_buf();
		set_cover(root.clone(), picture(dir.path(), "art.png", b"one")).unwrap();

		set_cover(root.clone(), root.join("cover.png")).unwrap();

		assert_eq!(cover_bytes(&root).unwrap(), b"one");
	}

	#[test]
	fn only_a_png_or_a_jpeg_is_a_cover() {
		let elsewhere = tempfile::tempdir().unwrap();
		let dir = tempfile::tempdir().unwrap();
		version_three_project(dir.path());
		let root = dir.path().to_path_buf();

		let err =
			set_cover(root.clone(), picture(elsewhere.path(), "notes.txt", b"one")).unwrap_err();

		assert!(matches!(err, Error::NotAnImage));
		assert_eq!(read_book(root).unwrap().cover, "");
	}

	#[test]
	fn a_cover_that_is_gone_says_so() {
		let elsewhere = tempfile::tempdir().unwrap();
		let dir = tempfile::tempdir().unwrap();
		version_three_project(dir.path());
		let root = dir.path().to_path_buf();

		assert!(
			matches!(cover_bytes(&root).unwrap_err(), Error::CoverMissing),
			"a book with nothing chosen has no cover to read"
		);

		set_cover(root.clone(), picture(elsewhere.path(), "art.png", b"one")).unwrap();
		fs::remove_file(root.join("cover.png")).unwrap();

		assert!(matches!(
			cover_bytes(&root).unwrap_err(),
			Error::CoverMissing
		));
		assert_eq!(
			read_book(root).unwrap().cover,
			"cover.png",
			"the book still names the cover it was given, so the page can complain about it"
		);
	}

	#[test]
	fn a_cover_out_of_the_project_is_refused() {
		let dir = tempfile::tempdir().unwrap();
		version_three_project(dir.path());
		let root = dir.path().to_path_buf();
		let mut manifest = read_manifest(&root).unwrap();
		manifest.book.cover = "../cover.png".to_owned();
		write_manifest(&root, &mut manifest).unwrap();

		assert!(matches!(
			cover_bytes(&root).unwrap_err(),
			Error::OutsideProject
		));
	}

	#[test]
	fn a_failed_atomic_write_leaves_no_temporary_file() {
		let dir = tempfile::tempdir().unwrap();
		let target = dir.path().join("missing").join("aurora.json");
		assert!(write_atomic(&target, b"{}").is_err());

		let blocked = dir.path().join("blocked");
		fs::create_dir(&blocked).unwrap();
		assert!(write_atomic(&blocked, b"{}").is_err());
		assert!(!dir.path().join("blocked.tmp").exists());
	}

	#[test]
	fn saving_the_book_keeps_the_cover_on_disk() {
		let elsewhere = tempfile::tempdir().unwrap();
		let dir = tempfile::tempdir().unwrap();
		version_three_project(dir.path());
		let root = dir.path().to_path_buf();
		set_cover(root.clone(), picture(elsewhere.path(), "art.png", b"one")).unwrap();

		write_book(
			root.clone(),
			Book {
				cover: String::new(),
				..read_book(root.clone()).unwrap()
			},
		)
		.unwrap();

		assert_eq!(read_book(root.clone()).unwrap().cover, "cover.png");
		assert_eq!(cover_bytes(&root).unwrap(), b"one");
	}

	#[cfg(unix)]
	#[test]
	fn a_read_only_source_does_not_block_the_next_cover() {
		use std::os::unix::fs::PermissionsExt;

		let elsewhere = tempfile::tempdir().unwrap();
		let dir = tempfile::tempdir().unwrap();
		version_three_project(dir.path());
		let root = dir.path().to_path_buf();
		let first = picture(elsewhere.path(), "first.png", b"one");
		fs::set_permissions(&first, fs::Permissions::from_mode(0o444)).unwrap();

		set_cover(root.clone(), first).unwrap();
		set_cover(
			root.clone(),
			picture(elsewhere.path(), "second.png", b"two"),
		)
		.unwrap();

		assert_eq!(cover_bytes(&root).unwrap(), b"two");
	}

	#[test]
	fn a_cover_that_cannot_be_read_keeps_the_old_one() {
		let elsewhere = tempfile::tempdir().unwrap();
		let dir = tempfile::tempdir().unwrap();
		version_three_project(dir.path());
		let root = dir.path().to_path_buf();
		set_cover(root.clone(), picture(elsewhere.path(), "art.png", b"one")).unwrap();

		assert!(set_cover(root.clone(), elsewhere.path().join("gone.jpg")).is_err());

		assert_eq!(read_book(root.clone()).unwrap().cover, "cover.png");
		assert_eq!(cover_bytes(&root).unwrap(), b"one");
	}

	#[test]
	fn clearing_a_cover_takes_the_copy_with_it() {
		let elsewhere = tempfile::tempdir().unwrap();
		let dir = tempfile::tempdir().unwrap();
		version_three_project(dir.path());
		let root = dir.path().to_path_buf();
		set_cover(root.clone(), picture(elsewhere.path(), "art.png", b"one")).unwrap();

		clear_cover(root.clone()).unwrap();

		assert_eq!(read_book(root.clone()).unwrap().cover, "");
		assert!(!root.join("cover.png").exists());
		clear_cover(root).unwrap();
	}

	#[test]
	fn the_cover_refuses_a_relative_path() {
		let relative = PathBuf::from("some/where");
		assert!(matches!(
			set_cover(relative.clone(), PathBuf::from("/tmp/art.png")).unwrap_err(),
			Error::RelativePath
		));
		assert!(matches!(
			cover_bytes(&relative).unwrap_err(),
			Error::RelativePath
		));
		assert!(matches!(
			clear_cover(relative).unwrap_err(),
			Error::RelativePath
		));
	}

	#[test]
	fn a_manifest_missing_a_field_is_rejected() {
		let json = r#"{"version":1,"name":"Ithaca","format":"novel","folders":[]}"#;
		assert!(serde_json::from_str::<Manifest>(json).is_err());
	}

	#[test]
	fn ordinary_names_are_accepted() {
		for name in [
			"Ithaca",
			"Book One — Draft 2",
			"Les Misérables",
			"夜明け前",
			"Chapter 1 (rewrite)",
			"a",
		] {
			assert_eq!(validate_name(name), Ok(()), "{name} should be accepted");
		}
	}

	#[test]
	fn blank_names_are_rejected() {
		assert_eq!(validate_name(""), Err(NameError::Empty));
		assert_eq!(validate_name("   "), Err(NameError::Empty));
		assert_eq!(validate_name("\t\n"), Err(NameError::Empty));
	}

	#[test]
	fn names_that_break_windows_are_rejected() {
		assert_eq!(
			validate_name("Ithaca: Book One"),
			Err(NameError::IllegalCharacter(':'))
		);
		assert_eq!(
			validate_name("What Now?"),
			Err(NameError::IllegalCharacter('?'))
		);
		assert_eq!(
			validate_name("\"Quoted\""),
			Err(NameError::IllegalCharacter('"'))
		);
	}

	#[test]
	fn path_separators_and_traversal_are_rejected() {
		assert_eq!(
			validate_name("drafts/old"),
			Err(NameError::IllegalCharacter('/'))
		);
		assert_eq!(
			validate_name("drafts\\old"),
			Err(NameError::IllegalCharacter('\\'))
		);
		assert_eq!(validate_name("."), Err(NameError::EdgeWhitespaceOrDot));
		assert_eq!(validate_name(".."), Err(NameError::EdgeWhitespaceOrDot));
		assert_eq!(
			validate_name(".hidden"),
			Err(NameError::EdgeWhitespaceOrDot)
		);
	}

	#[test]
	fn edge_whitespace_and_dots_are_rejected() {
		assert_eq!(
			validate_name("Ithaca "),
			Err(NameError::EdgeWhitespaceOrDot)
		);
		assert_eq!(
			validate_name(" Ithaca"),
			Err(NameError::EdgeWhitespaceOrDot)
		);
		assert_eq!(
			validate_name("Ithaca."),
			Err(NameError::EdgeWhitespaceOrDot)
		);
	}

	#[test]
	fn control_characters_are_rejected() {
		assert_eq!(
			validate_name("Itha\u{0}ca"),
			Err(NameError::IllegalCharacter('\u{0}'))
		);
		assert_eq!(
			validate_name("Itha\nca"),
			Err(NameError::IllegalCharacter('\n'))
		);
	}

	#[test]
	fn reserved_device_names_are_rejected() {
		for name in ["CON", "con", "NUL", "Com1", "LPT9", "CON.md"] {
			assert_eq!(
				validate_name(name),
				Err(NameError::Reserved),
				"{name} should be reserved"
			);
		}
		assert_eq!(validate_name("Console"), Ok(()));
	}

	#[test]
	fn over_long_names_are_rejected() {
		assert_eq!(validate_name(&"a".repeat(MAX_NAME_BYTES)), Ok(()));
		assert_eq!(
			validate_name(&"a".repeat(MAX_NAME_BYTES + 1)),
			Err(NameError::TooLong)
		);
	}

	#[test]
	fn errors_carry_their_cause() {
		let io: Error = std::io::Error::from(std::io::ErrorKind::PermissionDenied).into();
		assert!(std::error::Error::source(&io).is_some());

		let name: Error = NameError::Reserved.into();
		assert!(std::error::Error::source(&name).is_none());
		assert_eq!(name.to_string(), NameError::Reserved.to_string());
	}

	#[test]
	fn create_lays_out_a_novel() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		assert_eq!(root, parent.path().join("Ithaca"));
		for section in Format::Novel.layout() {
			let folder = root.join(section.folder);
			assert!(folder.is_dir(), "{} is missing", section.folder);
			match section.seed {
				Some(name) => {
					let seed = folder.join(name);
					assert!(seed.is_file(), "{name} is missing");
					assert_eq!(fs::read_to_string(&seed).unwrap(), "");
				}
				None => assert_eq!(
					fs::read_dir(&folder).unwrap().count(),
					0,
					"{} should start empty",
					section.folder
				),
			}
		}

		let json = fs::read_to_string(root.join(MANIFEST_FILE)).unwrap();
		let manifest: Manifest = serde_json::from_str(&json).unwrap();
		// Every node carries a random id, and so does the book, so the tree is
		// checked on its own and the book is checked by its title.
		let fresh = Manifest::new("Ithaca", Format::Novel, fixed_time());
		assert_eq!(manifest.book.title, fresh.book.title);
		assert_eq!(
			Manifest {
				nodes: Vec::new(),
				book: fresh.book.clone(),
				..manifest
			},
			Manifest {
				nodes: Vec::new(),
				..fresh
			}
		);
	}

	#[test]
	fn the_manifest_is_written_for_a_human_to_read() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let json = fs::read_to_string(root.join(MANIFEST_FILE)).unwrap();
		assert!(
			json.contains(&format!("\n\t\"version\": {MANIFEST_VERSION}")),
			"expected tab indentation"
		);
		assert!(json.contains("\"createdAt\": \"2023-11-14T22:13:20Z\""));
		assert!(json.ends_with("\n"), "expected a trailing newline");
	}

	#[test]
	fn a_new_project_records_its_seed_documents() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		let manifest = read_manifest(&root).unwrap();

		let documents = manifest.documents();
		let paths: Vec<_> = documents.iter().map(|d| d.path.as_str()).collect();
		assert_eq!(
			paths,
			[
				"Manuscript/Scene 1.md",
				"Outline/Outline.md",
				"Notes/Notes.md"
			]
		);

		let ids: HashSet<_> = documents.iter().map(|d| d.id).collect();
		assert_eq!(ids.len(), documents.len(), "ids must be unique");
	}

	#[test]
	fn every_recorded_document_is_on_disk() {
		let parent = tempfile::tempdir().unwrap();
		let root = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		for document in read_manifest(&root).unwrap().documents() {
			assert!(
				root.join(&document.path).is_file(),
				"{} is recorded but missing",
				document.path
			);
		}
	}

	#[test]
	fn a_manifest_from_before_documents_existed_still_loads() {
		let dir = tempfile::tempdir().unwrap();
		fs::write(
			dir.path().join(MANIFEST_FILE),
			r#"{"version":1,"name":"Ithaca","format":"novel",
			   "createdAt":"2023-11-14T22:13:20Z","folders":["Manuscript"]}"#,
		)
		.unwrap();

		let manifest = read_manifest(dir.path()).unwrap();
		assert_eq!(manifest.version, 1);
		assert!(manifest.documents().is_empty());
	}

	/// A project written by the Aurora before this one: section names and a flat
	/// list of documents, with no tree in sight.
	fn version_two_project(dir: &Path, id: Uuid) {
		fs::write(
			dir.join(MANIFEST_FILE),
			format!(
				r#"{{"version":2,"name":"Ithaca","format":"novel",
				   "createdAt":"2023-11-14T22:13:20Z",
				   "folders":["Manuscript","Notes"],
				   "documents":[
				     {{"id":"{id}","path":"Manuscript/Scene 1.md","target":1200}},
				     {{"id":"{}","path":"Notes/Notes.md"}}
				   ]}}"#,
				Uuid::new_v4()
			),
		)
		.unwrap();
	}

	#[test]
	fn a_version_two_manifest_opens_as_a_tree() {
		let dir = tempfile::tempdir().unwrap();
		let chapter_one = Uuid::new_v4();
		version_two_project(dir.path(), chapter_one);

		let manifest = read_manifest(dir.path()).unwrap();

		assert_eq!(manifest.folders(), ["Manuscript", "Notes"]);
		let documents = manifest.documents();
		assert_eq!(
			paths_of(&manifest),
			["Manuscript/Scene 1.md", "Notes/Notes.md"]
		);
		assert_eq!(
			documents[0].id, chapter_one,
			"a document keeps the id the project already gave it"
		);
		assert_eq!(documents[0].target, Some(1200), "and what it is aiming at");
	}

	#[test]
	fn reading_an_old_manifest_does_not_rewrite_it() {
		let dir = tempfile::tempdir().unwrap();
		version_two_project(dir.path(), Uuid::new_v4());

		read_manifest(dir.path()).unwrap();

		let json = fs::read_to_string(dir.path().join(MANIFEST_FILE)).unwrap();
		assert!(
			json.contains("\"documents\"") && !json.contains("\"nodes\""),
			"the file is left as it was until something changes it"
		);
	}

	#[test]
	fn writing_an_old_manifest_back_makes_it_a_tree() {
		let dir = tempfile::tempdir().unwrap();
		version_two_project(dir.path(), Uuid::new_v4());
		let mut manifest = read_manifest(dir.path()).unwrap();

		write_manifest(dir.path(), &mut manifest).unwrap();

		let json = fs::read_to_string(dir.path().join(MANIFEST_FILE)).unwrap();
		assert!(json.contains("\"nodes\""));
		assert!(
			!json.contains("\"documents\"") && !json.contains("\"folders\""),
			"what version 2 recorded is gone: {json}"
		);
		assert_eq!(
			read_manifest(dir.path()).unwrap().version,
			MANIFEST_VERSION,
			"and it says which version it now is"
		);
	}

	#[test]
	fn create_refuses_a_bad_name_without_touching_the_disk() {
		let parent = tempfile::tempdir().unwrap();
		let err = create(
			parent.path(),
			"Ithaca: Book One",
			Format::Novel,
			fixed_time(),
		)
		.unwrap_err();
		assert!(matches!(
			err,
			Error::InvalidName(NameError::IllegalCharacter(':'))
		));
		assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
	}

	#[test]
	fn create_refuses_a_format_without_a_layout() {
		let parent = tempfile::tempdir().unwrap();
		let err = create(parent.path(), "Ithaca", Format::Screenplay, fixed_time()).unwrap_err();
		assert!(matches!(err, Error::UnsupportedFormat(Format::Screenplay)));
		assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
	}

	#[test]
	fn create_will_not_overwrite_an_existing_folder() {
		let parent = tempfile::tempdir().unwrap();
		create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let err = create(parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap_err();
		assert!(matches!(err, Error::AlreadyExists));
		assert!(parent.path().join("Ithaca").join(MANIFEST_FILE).is_file());
	}

	#[test]
	fn create_reports_a_parent_that_cannot_be_made() {
		let parent = tempfile::tempdir().unwrap();
		let blocked = parent.path().join("nowhere");
		fs::write(&blocked, "").unwrap();
		let err = create(&blocked, "Ithaca", Format::Novel, fixed_time()).unwrap_err();
		assert!(matches!(err, Error::Io(_)));
	}

	#[test]
	fn a_blocked_section_folder_is_an_error() {
		let parent = tempfile::tempdir().unwrap();
		let root = parent.path().join("Ithaca");
		fs::create_dir(&root).unwrap();
		// A file where the first section folder needs to go.
		fs::write(root.join("Manuscript"), "").unwrap();

		assert!(
			fill(
				&root,
				"Ithaca",
				Format::Novel,
				Format::Novel.layout(),
				None,
				fixed_time()
			)
			.is_err()
		);
	}

	#[test]
	fn a_dev_store_starts_as_a_copy_of_the_installed_one() {
		let dir = tempfile::tempdir().unwrap();
		let installed = dir.path().join(STORE_FILE);
		let dev = dir.path().join(DEV_STORE_FILE);
		fs::write(&installed, "installed").unwrap();

		copy_when_missing(&installed, &dev).unwrap();

		assert_eq!(fs::read_to_string(&dev).unwrap(), "installed");
	}

	#[test]
	fn an_existing_dev_store_is_left_alone() {
		let dir = tempfile::tempdir().unwrap();
		let installed = dir.path().join(STORE_FILE);
		let dev = dir.path().join(DEV_STORE_FILE);
		fs::write(&installed, "installed").unwrap();
		fs::write(&dev, "my own").unwrap();

		copy_when_missing(&installed, &dev).unwrap();

		assert_eq!(fs::read_to_string(&dev).unwrap(), "my own");
	}

	#[test]
	fn creating_records_the_project_in_the_store() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("config").join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		assert!(root.join(MANIFEST_FILE).is_file());
		assert!(root.join("Manuscript").join("Scene 1.md").is_file());

		let remembered = store::load(&store).unwrap();
		assert_eq!(remembered.reopen().unwrap().name, "Ithaca");
		assert_eq!(remembered.reopen().unwrap().root, root);
	}

	#[test]
	fn a_failed_creation_is_not_remembered() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		assert!(
			create_and_remember(
				&store,
				parent.path(),
				"Ithaca: Book One",
				Format::Novel,
				fixed_time()
			)
			.is_err()
		);
		assert!(store::load(&store).unwrap().reopen().is_none());
	}

	#[test]
	fn opening_adopts_a_file_added_outside_aurora() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		fs::write(root.join("Manuscript").join("Chapter 2.md"), "").unwrap();

		open_and_remember(&store, &root, fixed_time()).unwrap();

		let manifest = read_manifest(&root).unwrap();
		assert!(
			paths_of(&manifest)
				.iter()
				.any(|p| p == "Manuscript/Chapter 2.md")
		);
	}

	#[test]
	fn opening_drops_a_file_deleted_outside_aurora() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		fs::remove_file(root.join("Notes").join("Notes.md")).unwrap();

		open_and_remember(&store, &root, fixed_time()).unwrap();

		let manifest = read_manifest(&root).unwrap();
		assert!(!paths_of(&manifest).iter().any(|p| p == "Notes/Notes.md"));
	}

	#[test]
	fn opening_an_unchanged_project_leaves_the_manifest_alone() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		// Rewritten compactly, so any rewrite would restore the pretty form.
		let manifest = read_manifest(&root).unwrap();
		let compact = serde_json::to_vec(&manifest).unwrap();
		fs::write(root.join(MANIFEST_FILE), &compact).unwrap();

		open_and_remember(&store, &root, fixed_time()).unwrap();

		assert_eq!(fs::read(root.join(MANIFEST_FILE)).unwrap(), compact);
	}

	#[test]
	fn the_command_refuses_a_relative_path() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let err = create_and_remember(
			&store,
			Path::new("some/where"),
			"Ithaca",
			Format::Novel,
			fixed_time(),
		)
		.unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	#[test]
	fn a_closed_project_does_not_reopen_but_is_still_offered() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		close(&store).unwrap();

		assert_eq!(resolve_last(&store).unwrap(), LastProject::None);
		assert_eq!(available_recents(&store::load(&store).unwrap()).len(), 1);
	}

	#[test]
	fn nothing_remembered_means_nothing_to_reopen() {
		let dir = tempfile::tempdir().unwrap();
		let store = dir.path().join("store.json");
		assert_eq!(resolve_last(&store).unwrap(), LastProject::None);
	}

	#[test]
	fn the_last_project_is_reopened() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		assert_eq!(
			resolve_last(&store).unwrap(),
			LastProject::Open {
				name: "Ithaca".to_owned(),
				root,
			}
		);
	}

	#[test]
	fn asking_twice_gives_the_same_answer() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		fs::remove_dir_all(&root).unwrap();

		let missing = LastProject::Missing {
			name: "Ithaca".to_owned(),
			root: root.clone(),
		};
		assert_eq!(resolve_last(&store).unwrap(), missing);
		assert_eq!(resolve_last(&store).unwrap(), missing);

		forget(&store, &root).unwrap();
		assert_eq!(resolve_last(&store).unwrap(), LastProject::None);
	}

	#[test]
	fn forgetting_twice_is_harmless() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		forget(&store, &root).unwrap();
		forget(&store, &root).unwrap();
		assert_eq!(resolve_last(&store).unwrap(), LastProject::None);
	}

	#[test]
	fn a_folder_without_a_manifest_counts_as_missing() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		fs::remove_file(root.join(MANIFEST_FILE)).unwrap();

		assert!(matches!(
			resolve_last(&store).unwrap(),
			LastProject::Missing { .. }
		));
	}

	#[test]
	fn the_last_project_serializes_as_a_tagged_object() {
		assert_eq!(
			serde_json::to_value(LastProject::None).unwrap(),
			serde_json::json!({ "kind": "none" })
		);
		assert_eq!(
			serde_json::to_value(LastProject::Missing {
				name: "Ithaca".to_owned(),
				root: PathBuf::from("/writing/Ithaca"),
			})
			.unwrap(),
			serde_json::json!({
				"kind": "missing",
				"name": "Ithaca",
				"root": "/writing/Ithaca",
			})
		);
	}

	#[test]
	fn an_existing_project_can_be_reopened() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		assert_eq!(
			open_and_remember(&store, &root, fixed_time()).unwrap(),
			OpenedProject {
				name: "Ithaca".to_owned(),
				root: root.clone(),
				enclosing_repository: None,
			}
		);
	}

	#[test]
	fn opening_uses_the_name_in_the_manifest() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		let renamed = parent.path().join("moved-elsewhere");
		fs::rename(&root, &renamed).unwrap();

		let opened = open_and_remember(&store, &renamed, fixed_time()).unwrap();
		assert_eq!(opened.name, "Ithaca");
		assert_eq!(opened.root, renamed);
	}

	#[test]
	fn opening_moves_the_project_to_the_front() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let first =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		create_and_remember(
			&store,
			parent.path(),
			"Penelope",
			Format::Novel,
			fixed_time(),
		)
		.unwrap();

		open_and_remember(&store, &first, fixed_time()).unwrap();
		assert_eq!(
			store::load(&store).unwrap().reopen().unwrap().name,
			"Ithaca"
		);
	}

	#[test]
	fn an_ordinary_folder_is_not_a_project() {
		let dir = tempfile::tempdir().unwrap();
		let store = dir.path().join("store.json");
		let err = open_and_remember(&store, dir.path(), fixed_time()).unwrap_err();
		assert!(matches!(err, Error::NotAProject));
	}

	#[test]
	fn opening_refuses_a_relative_path() {
		let dir = tempfile::tempdir().unwrap();
		let store = dir.path().join("store.json");
		let err = open_and_remember(&store, Path::new("some/where"), fixed_time()).unwrap_err();
		assert!(matches!(err, Error::RelativePath));
	}

	/// Rewrites the manifest's version field, leaving the rest of the file alone.
	fn set_manifest_version(root: &Path, version: u32) {
		let path = root.join(MANIFEST_FILE);
		let mut value: serde_json::Value =
			serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
		value["version"] = version.into();
		fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
	}

	#[test]
	fn a_manifest_from_a_newer_aurora_is_refused() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		set_manifest_version(&root, MANIFEST_VERSION + 1);

		let err = read_manifest(&root).unwrap_err();
		assert!(matches!(
			err,
			Error::UnsupportedVersion { found, supported }
				if found == MANIFEST_VERSION + 1 && supported == MANIFEST_VERSION
		));
	}

	#[test]
	fn an_error_crosses_as_a_kind_and_a_message() {
		let json = serde_json::to_value(Error::DocumentMissing).unwrap();
		assert_eq!(json["kind"], "documentMissing");
		assert_eq!(json["message"], "that document's file is no longer there");

		let json = serde_json::to_value(Error::InvalidName(NameError::Empty)).unwrap();
		assert_eq!(json["kind"], "invalidName");
		assert_eq!(json["message"], NameError::Empty.to_string());
	}

	/// The frontend switches on these, so no two variants may answer to the
	/// same name and none may be left out.
	#[test]
	fn every_error_has_its_own_kind() {
		let kinds = [
			Error::InvalidName(NameError::Empty),
			Error::RelativePath,
			Error::NoConfigDir,
			Error::NotAProject,
			Error::UnsupportedVersion {
				found: 3,
				supported: 2,
			},
			Error::UnknownDocument,
			Error::UnknownSection,
			Error::UnknownFolder,
			Error::FolderNotAllowed,
			Error::SectionFixed,
			Error::MoveInsideItself,
			Error::DocumentMissing,
			Error::ChangedOnDisk,
			Error::DocumentExists,
			Error::BadDocumentPath,
			Error::OutsideProject,
			Error::NotText,
			Error::AlreadyExists,
			Error::EmptyMessage,
			Error::UnsupportedFormat(Format::Screenplay),
			Error::Io(io::Error::from(io::ErrorKind::PermissionDenied)),
			Error::Json(serde_json::from_str::<Manifest>("{").unwrap_err()),
		]
		.map(|error| error.kind());

		let unique: std::collections::HashSet<_> = kinds.iter().collect();
		assert_eq!(unique.len(), kinds.len());
	}

	/// The message reaches the writer verbatim, so it has to read as a sentence.
	#[test]
	fn error_messages_are_single_spaced() {
		for error in [
			Error::UnsupportedVersion {
				found: 3,
				supported: 2,
			},
			Error::InvalidName(NameError::Empty),
			Error::RelativePath,
			Error::NoConfigDir,
			Error::NotAProject,
			Error::UnknownDocument,
			Error::UnknownSection,
			Error::UnknownFolder,
			Error::FolderNotAllowed,
			Error::SectionFixed,
			Error::MoveInsideItself,
			Error::DocumentMissing,
			Error::ChangedOnDisk,
			Error::DocumentExists,
			Error::BadDocumentPath,
			Error::OutsideProject,
			Error::NotText,
			Error::AlreadyExists,
			Error::EmptyMessage,
			Error::UnsupportedFormat(Format::Screenplay),
		] {
			let message = error.to_string();
			assert!(
				!message.contains('\t') && !message.contains('\n') && !message.contains("  "),
				"{message:?}"
			);
		}
	}

	#[test]
	fn a_project_we_cannot_read_is_not_remembered() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		set_manifest_version(&root, MANIFEST_VERSION + 1);
		forget(&store, &root).unwrap();

		let err = open_and_remember(&store, &root, fixed_time()).unwrap_err();
		assert!(matches!(err, Error::UnsupportedVersion { .. }));
		assert!(available_recents(&store::load(&store).unwrap()).is_empty());
	}

	#[test]
	fn an_older_manifest_still_opens() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		set_manifest_version(&root, MANIFEST_VERSION - 1);

		assert_eq!(read_manifest(&root).unwrap().version, MANIFEST_VERSION - 1);
	}

	#[test]
	fn the_recent_list_hides_projects_that_have_gone() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let gone =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		create_and_remember(
			&store,
			parent.path(),
			"Penelope",
			Format::Novel,
			fixed_time(),
		)
		.unwrap();
		fs::remove_dir_all(&gone).unwrap();

		let recents = available_recents(&store::load(&store).unwrap());
		assert_eq!(recents.len(), 1);
		assert_eq!(recents[0].name, "Penelope");
	}

	#[test]
	fn the_recent_list_counts_every_document_in_a_project() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		let manuscript = root.join("Manuscript");
		fs::write(manuscript.join("Scene 1.md"), "one two three").unwrap();
		fs::write(manuscript.join("Chapter 2.md"), "four five").unwrap();
		refresh(&root).unwrap();

		let recents = summarise_recents(&store).unwrap();
		assert_eq!(recents.len(), 1);
		assert_eq!(recents[0].words, Some(5));
	}

	#[test]
	fn a_document_pointing_out_of_the_project_is_not_counted() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		fs::write(parent.path().join("elsewhere.md"), "one two three").unwrap();
		// A path that climbs out of the project can only be written by hand,
		// and only at the top of the tree: a name under a folder is one part.
		let mut manifest = read_manifest(&root).unwrap();
		manifest.nodes.insert(0, Node::document("../elsewhere.md"));
		write_manifest(&root, &mut manifest).unwrap();

		assert_eq!(summarise_recents(&store).unwrap()[0].words, Some(0));
	}

	/// Where the shelf says a book was left, with `front` the only tab open.
	fn left_on(front: store::SavedTab, nodes: &[Node]) -> Option<String> {
		let workspace = store::Workspace {
			tabs: vec![store::SavedTab::Trash, front],
			active: Some(1),
			beside: None,
		};
		left_off(&workspace, nodes)
	}

	#[test]
	fn the_shelf_names_the_tab_a_book_was_left_on() {
		let scene = Node::document("Harbour.md");
		let loose = Node::document("Notes.md");
		let chapter = Node::folder("Chapter 1", vec![scene.clone()]);
		let nodes = [chapter.clone(), loose.clone()];
		let document = |id| store::SavedTab::Document {
			id,
			caret: None,
			scroll: 0,
		};

		assert_eq!(
			left_on(document(scene.id()), &nodes).as_deref(),
			Some("Harbour · Chapter 1"),
		);
		assert_eq!(
			left_on(document(loose.id()), &nodes).as_deref(),
			Some("Notes")
		);
		assert_eq!(
			left_on(store::SavedTab::Folder { id: chapter.id() }, &nodes).as_deref(),
			Some("Chapter 1"),
		);
		assert_eq!(
			left_on(store::SavedTab::Subject { id: scene.id() }, &nodes).as_deref(),
			Some("Harbour"),
		);
		assert_eq!(
			left_on(
				store::SavedTab::Tag {
					tag: "storm".to_owned()
				},
				&nodes
			)
			.as_deref(),
			Some("storm"),
		);
		assert_eq!(
			left_on(store::SavedTab::Book, &nodes).as_deref(),
			Some("Book details"),
		);
		assert_eq!(left_on(store::SavedTab::Trash, &nodes), None);
		assert_eq!(left_on(store::SavedTab::Search, &nodes), None);
	}

	#[test]
	fn the_shelf_says_nothing_when_the_front_tab_is_gone() {
		let nodes = [Node::folder("Chapter 1", Vec::new())];

		assert_eq!(
			left_on(
				store::SavedTab::Document {
					id: Uuid::new_v4(),
					caret: None,
					scroll: 0,
				},
				&nodes
			),
			None,
		);
		assert_eq!(
			left_on(store::SavedTab::Folder { id: Uuid::new_v4() }, &nodes),
			None,
		);
		// A folder's id where a document was expected names nothing.
		assert_eq!(
			left_on(store::SavedTab::Subject { id: nodes[0].id() }, &nodes),
			None,
		);
		let nothing_open = store::Workspace {
			tabs: vec![store::SavedTab::Book],
			active: None,
			beside: None,
		};
		assert_eq!(left_off(&nothing_open, &nodes), None);
	}

	#[test]
	fn the_store_forgets_tabs_for_a_folder_that_is_gone() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let there = parent.path().join("there");
		let gone = parent.path().join("gone");
		fs::create_dir(&there).unwrap();
		store::update(&store, |loaded| {
			loaded.expanded.insert(there.clone(), Vec::new());
			loaded.expanded.insert(gone, Vec::new());
		})
		.unwrap();

		prune_gone(&store).unwrap();

		let kept = store::load(&store).unwrap();
		assert_eq!(kept.expanded.keys().collect::<Vec<_>>(), vec![&there]);
	}

	/// A projects folder inside `parent`, set in the store.
	fn books_folder(store: &Path, parent: &Path) -> PathBuf {
		let folder = parent.join("Books");
		fs::create_dir(&folder).unwrap();
		store::update(store, |loaded| {
			loaded.preferences.projects_folder = Some(folder.clone());
		})
		.unwrap();
		folder
	}

	/// A project made in the folder without the shelf's store hearing of it,
	/// as one copied in by hand would be.
	fn unopened(folder: &Path, name: &str) -> PathBuf {
		let elsewhere = folder.join(format!(".{name}-store.json"));
		let root =
			create_and_remember(&elsewhere, folder, name, Format::Novel, fixed_time()).unwrap();
		fs::remove_file(elsewhere).unwrap();
		root
	}

	fn names(shelf: &[RecentSummary]) -> Vec<&str> {
		shelf.iter().map(|book| book.name.as_str()).collect()
	}

	#[test]
	fn a_book_in_the_projects_folder_is_shelved_before_it_is_opened() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let folder = books_folder(&store, parent.path());
		unopened(&folder, "Penelope");
		fs::create_dir(folder.join("Not a book")).unwrap();

		let shelf = summarise_recents(&store).unwrap();
		assert_eq!(names(&shelf), ["Penelope"]);
		assert_eq!(shelf[0].last_opened, None);
		assert!(!shelf[0].elsewhere);
		assert_eq!(shelf[0].words, Some(0));
	}

	#[test]
	fn the_shelf_lists_opened_books_newest_first_then_the_rest_by_name() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let folder = books_folder(&store, parent.path());
		unopened(&folder, "zeta");
		unopened(&folder, "Alpha");
		let at = |seconds| OffsetDateTime::from_unix_timestamp(seconds).unwrap();
		create_and_remember(&store, &folder, "Ithaca", Format::Novel, at(1)).unwrap();
		create_and_remember(&store, parent.path(), "Troy", Format::Novel, at(2)).unwrap();

		let shelf = summarise_recents(&store).unwrap();
		// Ithaca is both opened and in the folder, and shows once.
		assert_eq!(names(&shelf), ["Troy", "Ithaca", "Alpha", "zeta"]);
		assert!(shelf[0].elsewhere);
		assert!(!shelf[1].elsewhere);
	}

	#[test]
	fn a_forgotten_folder_book_stays_off_the_shelf_until_it_is_opened() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let folder = books_folder(&store, parent.path());
		let root = unopened(&folder, "Penelope");

		forget(&store, &root).unwrap();
		assert!(summarise_recents(&store).unwrap().is_empty());

		open_and_remember(&store, &root, fixed_time()).unwrap();
		assert_eq!(names(&summarise_recents(&store).unwrap()), ["Penelope"]);
	}

	#[test]
	fn every_book_is_from_elsewhere_while_no_projects_folder_is_set() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		let shelf = summarise_recents(&store).unwrap();
		assert_eq!(names(&shelf), ["Ithaca"]);
		assert!(shelf[0].elsewhere);
	}

	#[test]
	fn a_missing_projects_folder_leaves_the_opened_books() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let folder = books_folder(&store, parent.path());
		create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();
		fs::remove_dir(folder).unwrap();

		assert_eq!(names(&summarise_recents(&store).unwrap()), ["Ithaca"]);
	}

	#[test]
	fn the_shelf_has_no_cap() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		for n in 0..12 {
			create_and_remember(
				&store,
				parent.path(),
				&format!("Book {n}"),
				Format::Novel,
				fixed_time(),
			)
			.unwrap();
		}

		assert_eq!(summarise_recents(&store).unwrap().len(), 12);
	}

	#[test]
	fn a_missing_projects_folder_is_made_by_its_first_project() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let folder = parent.path().join("Documents").join("Aurora");

		let root =
			create_and_remember(&store, &folder, "Ithaca", Format::Novel, fixed_time()).unwrap();
		assert_eq!(root, folder.join("Ithaca"));
		assert!(root.join(MANIFEST_FILE).is_file());
	}

	#[test]
	fn a_never_opened_folder_book_can_go_to_the_trash() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let folder = books_folder(&store, parent.path());
		let root = unopened(&folder, "Penelope");

		assert!(require_remembered(&store, &root).is_ok());
	}

	#[test]
	fn a_project_on_the_list_can_go_to_the_trash_even_if_it_cannot_be_read() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		fs::write(root.join(MANIFEST_FILE), "not a manifest").unwrap();

		assert!(require_remembered(&store, &root).is_ok());
	}

	#[test]
	fn a_project_off_the_list_cannot_go_to_the_trash() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		forget(&store, &root).unwrap();

		assert!(matches!(
			require_remembered(&store, &root),
			Err(Error::NotAProject)
		));
	}

	#[test]
	fn the_recent_list_shows_the_book_a_project_holds() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		let mut manifest = read_manifest(&root).unwrap();
		manifest.book.title = "The Salt Almanac".to_owned();
		manifest.book.author = "Ines Marlowe".to_owned();
		manifest.book.cover = "cover.png".to_owned();
		write_manifest(&root, &mut manifest).unwrap();

		let recents = summarise_recents(&store).unwrap();
		assert_eq!(recents[0].title.as_deref(), Some("The Salt Almanac"));
		assert_eq!(recents[0].author.as_deref(), Some("Ines Marlowe"));
		assert_eq!(recents[0].cover.as_deref(), Some("cover.png"));
		assert_eq!(recents[0].format, Some(Format::Novel));
	}

	#[test]
	fn a_project_that_cannot_be_read_shows_no_book() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();
		fs::write(root.join(MANIFEST_FILE), "not a manifest").unwrap();

		let recents = summarise_recents(&store).unwrap();
		assert_eq!(recents[0].title, None);
		assert_eq!(recents[0].author, None);
		assert_eq!(recents[0].cover, None);
		assert_eq!(recents[0].format, None);
	}

	#[test]
	fn a_book_with_no_image_has_no_cover_to_fetch() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time()).unwrap();

		assert_eq!(summarise_recents(&store).unwrap()[0].cover, None);
	}

	#[test]
	fn a_cover_pointing_out_of_the_project_is_passed_over() {
		let parent = tempfile::tempdir().unwrap();
		let store = parent.path().join("store.json");
		let root =
			create_and_remember(&store, parent.path(), "Ithaca", Format::Novel, fixed_time())
				.unwrap();

		let mut manifest = read_manifest(&root).unwrap();
		manifest.book.cover = "../elsewhere.png".to_owned();
		write_manifest(&root, &mut manifest).unwrap();

		assert_eq!(summarise_recents(&store).unwrap()[0].cover, None);
	}
}
