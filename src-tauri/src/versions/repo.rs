use super::*;

/// Kept out of every version, so going back never rolls back Stats or the
/// Trash. Written to `.git/info/exclude` so the project folder gains no file.
pub(super) fn excluded() -> [String; 5] {
	[
		format!("/{HISTORY_FILE}"),
		format!("/{TRASH_DIR}/"),
		DESKTOP_FILES[0].to_owned(),
		DESKTOP_FILES[1].to_owned(),
		DESKTOP_FILES[2].to_owned(),
	]
}

/// What file managers leave in any folder they show.
pub(super) const DESKTOP_FILES: [&str; 3] = [".DS_Store", "Thumbs.db", "desktop.ini"];

/// Opens the repository at `root`, making one on Main with nothing kept yet
/// when there is none. Returns the folder of a repository the project sits
/// inside, while the writer has not yet been told about it.
pub fn ensure_repository(root: &Path) -> Result<Option<PathBuf>> {
	// `open` looks only at `root`: a repository further up belongs to
	// something else, and the project still gets its own.
	let repo = match Repository::open(root) {
		Ok(repo) => repo,
		Err(e) if e.code() == ErrorCode::NotFound => {
			// Named here so the machine's `init.defaultBranch` has no say.
			let repo =
				Repository::init_opts(root, RepositoryInitOptions::new().initial_head("main"))?;
			// Nor its `core.autocrlf`: Git for Windows usually turns that on, and
			// every checkout would then rewrite the documents with CRLF.
			repo.config()?.set_bool("core.autocrlf", false)?;
			repo
		}
		Err(e) => return Err(e.into()),
	};
	// An adopted repository gets the same unless it says otherwise itself.
	let mut own = repo.config()?.open_level(git2::ConfigLevel::Local)?;
	if own.get_entry("core.autocrlf").is_err() {
		own.set_bool("core.autocrlf", false)?;
	}
	exclude(&repo)?;
	enclosing(root, &repo)
}

/// The commit `HEAD` points at, or none on a branch with nothing kept yet.
pub(super) fn head_commit(repo: &Repository) -> Result<Option<Commit<'_>>> {
	match repo.head() {
		Ok(head) => Ok(Some(head.peel_to_commit()?)),
		Err(e) if e.code() == ErrorCode::UnbornBranch => Ok(None),
		Err(e) => Err(e.into()),
	}
}

pub(super) fn exclude(repo: &Repository) -> Result<()> {
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

pub(super) fn signature(author: &str) -> Result<Signature<'static>> {
	// libgit2 refuses angle brackets and newlines anywhere, and a name that is
	// only punctuation once it trims its own list from the ends.
	let name: String = author
		.chars()
		.filter(|c| !matches!(c, '<' | '>' | '\n' | '\r'))
		.collect();
	let name = name.trim_matches(|c: char| c <= ' ' || ".,:;\"\\'".contains(c));
	let name = if name.is_empty() { "Aurora" } else { name };
	// libgit2 refuses an empty email, and a writer has none to give.
	Ok(Signature::now(name, AUTHOR_EMAIL)?)
}

/// The whole project as it stands on disk, deletions included, in memory
/// only: `.git/index` keeps whatever the writer staged.
pub(super) fn working(repo: &Repository) -> Result<Index> {
	let mut index = repo.index()?;
	index.add_all(["*"], IndexAddOption::DEFAULT, None)?;
	index.update_all(["*"], None)?;
	// Copies kept before they were excluded leave with the next checkpoint.
	let patterns = DESKTOP_FILES
		.iter()
		.flat_map(|name| [name.to_string(), format!("**/{name}")]);
	index.remove_all(patterns, None)?;
	Ok(index)
}

/// Stages the whole project and returns its tree.
pub(super) fn stage(repo: &Repository) -> Result<Oid> {
	let mut index = working(repo)?;
	index.write()?;
	Ok(index.write_tree()?)
}

pub(super) fn enclosing(root: &Path, repo: &Repository) -> Result<Option<PathBuf>> {
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
