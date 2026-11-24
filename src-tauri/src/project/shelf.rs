use super::*;

/// What the frontend should show on launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum LastProject {
	None,
	Open { name: String, root: PathBuf },
	Missing { name: String, root: PathBuf },
}

/// Reports what to reopen without changing anything, so asking twice gives the
/// same answer. Dropping a missing project is `forget` below.
pub(super) fn resolve_last(store_path: &Path) -> Result<LastProject> {
	let store = store::load(store_path)?;
	let Some(last) = store.reopen() else {
		return Ok(LastProject::None);
	};

	let name = last.name.clone();
	let root = last.root.clone();
	if root.join(MANIFEST_FILE).is_file() {
		Ok(LastProject::Open { name, root })
	} else {
		Ok(LastProject::Missing { name, root })
	}
}

/// Only projects that are still on disk, so the list never offers a button that
/// cannot work.
pub(super) fn available_recents(store: &store::Store) -> Vec<store::RecentProject> {
	store
		.recent
		.iter()
		.filter(|project| project.root.join(MANIFEST_FILE).is_file())
		.cloned()
		.collect()
}

/// A book as the welcome screen shows it: where it is and when it was last
/// opened, with the size of the writing counted from the files themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecentSummary {
	pub name: String,
	pub root: PathBuf,
	/// None for a book in the projects folder that has never been opened.
	#[serde(with = "time::serde::rfc3339::option")]
	#[ts(type = "string | null")]
	pub last_opened: Option<OffsetDateTime>,
	/// Whether the book lives outside the projects folder, which is every book
	/// while no projects folder is set.
	pub elsewhere: bool,
	/// None when the manifest could not be read, so the screen can leave the
	/// count off rather than report a project as empty.
	pub words: Option<usize>,
	/// The script's page count for a screenplay, None for any other book.
	pub pages: Option<usize>,
	/// None for the same reason. The question asked before a project is
	/// trashed counts what is about to go, so it needs this to be honest.
	pub documents: Option<usize>,
	/// None for the same reason, and likewise the three after it.
	pub title: Option<String>,
	pub author: Option<String>,
	/// Relative to the project, for `read_cover` to fetch. None as well when
	/// the book has no image, or names one `read_cover` would refuse.
	pub cover: Option<String>,
	pub format: Option<Format>,
	/// The tab that was in front when the book was last left, in words. None
	/// for a book never left on this machine, for Trash and Search, and for a
	/// tab whose document or folder has gone since.
	pub left_off: Option<String>,
}

/// What the welcome screen says about a project. All of it comes out of one
/// read of the manifest, since nothing ever wants only part of it.
pub(super) struct Measure {
	name: String,
	words: usize,
	pages: Option<usize>,
	documents: usize,
	title: String,
	author: String,
	cover: Option<String>,
	format: Format,
	left_off: Option<String>,
}

/// Whether a cover's name in the manifest is one file in the project. The
/// manifest is a file the writer can edit, so a path out of it has to be caught.
pub(super) fn cover_stays_inside(name: &str) -> bool {
	Path::new(name).file_name() == Some(std::ffi::OsStr::new(name))
}

/// What the shelf says a book was left on: the front tab, named the way the
/// writer would name it.
pub(super) fn left_off(workspace: &store::Workspace, nodes: &[Node]) -> Option<String> {
	let front = workspace
		.tabs
		.get(usize::try_from(workspace.active?).ok()?)?;
	let title = |id| match tree::find(nodes, id)? {
		Node::Document { name, .. } => Path::new(name)
			.file_stem()
			.map(|stem| stem.to_string_lossy().into_owned()),
		Node::Folder { .. } => None,
	};
	match front {
		store::SavedTab::Document { id, .. } => {
			let title = title(*id)?;
			Some(match tree::parent(nodes, *id) {
				Some(folder) => format!("{title} · {}", folder.name()),
				None => title,
			})
		}
		store::SavedTab::Subject { id } => title(*id),
		store::SavedTab::Folder { id } => match tree::find(nodes, *id)? {
			Node::Folder { name, .. } => Some(name.clone()),
			Node::Document { .. } => None,
		},
		store::SavedTab::Tag { tag } => Some(tag.clone()),
		store::SavedTab::Book => Some("Book details".to_owned()),
		store::SavedTab::Trash | store::SavedTab::Search => None,
	}
}

pub(super) fn measure(root: &Path, workspace: Option<&store::Workspace>) -> Option<Measure> {
	let manifest = read_manifest(root).ok()?;
	// Counted the way the Manuscript heading counts, which also passes over a
	// path in the manifest that climbs out of the project. The documents stay
	// every one there is: the shelf says how many a delete will take.
	let words = crate::document::manuscript_words(root, &manifest.nodes);
	let pages = crate::document::Pages::of(root, &manifest).map(|pages| pages.total());
	let documents = manifest.documents().len();
	Some(Measure {
		pages,
		left_off: workspace.and_then(|workspace| left_off(workspace, &manifest.nodes)),
		name: manifest.name,
		title: manifest.book.title,
		author: manifest.book.author,
		cover: Some(manifest.book.cover).filter(|name| cover_stays_inside(name)),
		format: manifest.format,
		words,
		documents,
	})
}

/// A book that has never been opened has no name in the store, so it takes the
/// one in its manifest, or its folder's when the manifest cannot be read.
pub(super) fn summarise(
	root: PathBuf,
	name: Option<String>,
	last_opened: Option<OffsetDateTime>,
	elsewhere: bool,
	workspace: Option<&store::Workspace>,
) -> RecentSummary {
	let measured = measure(&root, workspace);
	let name = name
		.or_else(|| measured.as_ref().map(|m| m.name.clone()))
		.unwrap_or_else(|| {
			root.file_name()
				.map(|name| name.to_string_lossy().into_owned())
				.unwrap_or_default()
		});
	RecentSummary {
		name,
		last_opened,
		elsewhere,
		words: measured.as_ref().map(|m| m.words),
		pages: measured.as_ref().and_then(|m| m.pages),
		documents: measured.as_ref().map(|m| m.documents),
		title: measured.as_ref().map(|m| m.title.clone()),
		author: measured.as_ref().map(|m| m.author.clone()),
		cover: measured.as_ref().and_then(|m| m.cover.clone()),
		format: measured.as_ref().map(|m| m.format),
		left_off: measured.and_then(|m| m.left_off),
		root,
	}
}

/// Where the path leads on disk, so a book reached two ways is one book.
pub(super) fn on_disk(path: &Path) -> PathBuf {
	fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The projects folder as it is on disk, or None while it is unset or gone.
pub(super) fn projects_folder(store: &store::Store) -> Option<PathBuf> {
	fs::canonicalize(store.preferences.projects_folder.as_deref()?).ok()
}

/// Every folder directly inside the projects folder that holds a project.
pub(super) fn folder_books(folder: &Path) -> Vec<PathBuf> {
	let Ok(entries) = fs::read_dir(folder) else {
		return Vec::new();
	};
	entries
		.filter_map(|entry| Some(entry.ok()?.path()))
		.filter(|path| path.join(MANIFEST_FILE).is_file())
		.collect()
}

/// The shelf: books opened before, newest first, then the projects folder's
/// books that have never been opened, by name. A forgotten book stays off it
/// until it is opened again.
pub(super) fn summarise_recents(store_path: &Path) -> Result<Vec<RecentSummary>> {
	let store = store::load(store_path)?;
	let folder = projects_folder(&store);
	let forgotten: HashSet<PathBuf> = store.forgotten.iter().map(|root| on_disk(root)).collect();
	let mut seen = HashSet::new();
	let mut shelf = Vec::new();

	for project in available_recents(&store) {
		let real = on_disk(&project.root);
		if !seen.insert(real.clone()) {
			continue;
		}
		let elsewhere = folder
			.as_ref()
			.is_none_or(|folder| !real.starts_with(folder));
		let workspace = store.workspace.get(&project.root);
		shelf.push(summarise(
			project.root,
			Some(project.name),
			Some(project.last_opened),
			elsewhere,
			workspace,
		));
	}

	let mut unopened: Vec<RecentSummary> = folder
		.as_deref()
		.map(folder_books)
		.unwrap_or_default()
		.into_iter()
		.filter(|root| {
			let real = on_disk(root);
			!forgotten.contains(&real) && seen.insert(real)
		})
		.map(|root| summarise(root, None, None, false, None))
		.collect();
	unopened.sort_by_key(|book| book.name.to_lowercase());
	shelf.extend(unopened);

	Ok(shelf)
}

#[tauri::command(async)]
pub fn recent_projects(app: AppHandle) -> Result<Vec<RecentSummary>> {
	summarise_recents(&store_path(&app)?)
}

#[tauri::command]
pub fn last_project(app: AppHandle) -> Result<LastProject> {
	let store = store_path(&app)?;
	// Tidying is not worth stopping a launch for.
	let _ = prune_gone(&store);
	resolve_last(&store)
}
