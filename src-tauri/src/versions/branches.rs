use super::*;

/// One version of the project, which git calls a branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Branch {
	pub slug: String,
	/// The name as the writer typed it.
	pub name: String,
	pub current: bool,
	/// Checkpoints no other version reaches, which deleting this one loses.
	pub unmerged: u32,
	/// Main, or a checkout no version names: neither can be renamed or
	/// deleted.
	pub fixed: bool,
}

pub(super) const MAIN: &str = "main";
/// What Main is in an adopted repository that has no `main`.
pub(super) const MASTER: &str = "master";

/// Set once the writer has been told about an enclosing repository. It lives
/// in the project's own git config, which is local and never pushed.
pub const NESTED_NOTICE_SHOWN: &str = "aurora.nestedNoticeShown";

/// The branch name for a version: lowercase, each run of anything but letters
/// and digits made one hyphen, and no hyphen at either end.
pub(super) fn slug(name: &str) -> Result<String> {
	let mut slug = String::new();
	for c in name.chars().flat_map(char::to_lowercase) {
		if c.is_alphanumeric() {
			slug.push(c);
		} else if !slug.is_empty() && !slug.ends_with('-') {
			slug.push('-');
		}
	}
	let slug = slug.trim_end_matches('-');
	if slug.is_empty() {
		return Err(Error::VersionName);
	}
	Ok(slug.to_owned())
}

/// Where the typed name lives: the project's local git config, which follows
/// the branch through a rename and goes when it is deleted.
pub(super) fn name_key(slug: &str) -> String {
	format!("branch.{slug}.auroraname")
}

/// The branch `HEAD` names, even one with nothing kept in it yet.
pub(super) fn current_branch(repo: &Repository) -> Result<Option<String>> {
	let head = repo.find_reference("HEAD")?;
	Ok(head
		.symbolic_target()?
		.and_then(|target| target.strip_prefix("refs/heads/"))
		.map(str::to_owned))
}

/// The version shown as Main: `main`, or `master` in an adopted repository
/// that has no `main`.
pub(super) fn main_slug(repo: &Repository) -> Result<&'static str> {
	let current = current_branch(repo)?;
	let has = |slug: &str| {
		repo.find_branch(slug, BranchType::Local).is_ok() || current.as_deref() == Some(slug)
	};
	Ok(if !has(MAIN) && has(MASTER) {
		MASTER
	} else {
		MAIN
	})
}

/// A checkout no version names, as its short id and checkpoint.
pub(super) fn detached(repo: &Repository) -> Result<Option<(String, Oid)>> {
	if !repo.head_detached()? {
		return Ok(None);
	}
	let head = repo.head()?.peel_to_commit()?;
	let short = head.as_object().short_id()?;
	Ok(Some((
		short.as_str().unwrap_or_default().to_owned(),
		head.id(),
	)))
}

pub(super) fn branches(root: &Path) -> Result<Vec<Branch>> {
	let repo = Repository::open(root)?;
	let current = current_branch(&repo)?;
	let main = main_slug(&repo)?;

	let mut tips = Vec::new();
	for branch in repo.branches(Some(BranchType::Local))? {
		let (branch, _) = branch?;
		if let Some(slug) = branch.name()? {
			tips.push((slug.to_owned(), branch.get().peel_to_commit()?.id()));
		}
	}
	// Counted like a version, since leaving it loses what only it reaches.
	let detached = detached(&repo)?;
	if let Some((short, id)) = &detached {
		tips.push((short.clone(), *id));
	}
	let left = |slug: &str| detached.as_ref().is_some_and(|(short, _)| short == slug);

	let mut rows = Vec::new();
	for (slug, tip) in &tips {
		let mut walk = repo.revwalk()?;
		walk.push(*tip)?;
		for (other, tip) in &tips {
			if other != slug {
				walk.hide(*tip)?;
			}
		}
		let mut unmerged = 0;
		for id in walk {
			id?;
			unmerged += 1;
		}
		rows.push(Branch {
			slug: slug.clone(),
			name: String::new(),
			current: current.as_ref() == Some(slug) || left(slug),
			unmerged,
			fixed: slug == main || left(slug),
		});
	}
	// Main before its first checkpoint has no branch yet, but the writer is in it.
	if let Some(current) = current
		&& !tips.iter().any(|(slug, _)| *slug == current)
	{
		rows.push(Branch {
			fixed: current == main,
			slug: current,
			name: String::new(),
			current: true,
			unmerged: 0,
		});
	}

	for row in &mut rows {
		row.name = version_name(&repo, &row.slug)?;
	}
	rows.sort_by_key(|row| (row.slug != main, row.name.to_lowercase()));
	Ok(rows)
}

/// A version's name as the writer typed it.
pub(super) fn version_name(repo: &Repository, slug: &str) -> Result<String> {
	if slug == main_slug(repo)? {
		return Ok("Main".to_owned());
	}
	Ok(repo
		.config()?
		.get_string(&name_key(slug))
		.unwrap_or_else(|_| slug.to_owned()))
}

/// Starts a version at the current checkpoint and moves the writer into it.
/// Only `HEAD` moves, so changes not yet in a checkpoint come along.
pub(super) fn new_branch(root: &Path, name: &str) -> Result<String> {
	let slug = slug(name)?;
	if slug == MAIN || slug == MASTER {
		return Err(Error::VersionExists);
	}
	let repo = Repository::open(root)?;
	refuse_while_merging(&repo)?;
	let Some(head) = head_commit(&repo)? else {
		return Err(Error::NothingKept);
	};
	match repo.branch(&slug, &head, false) {
		Err(e) if e.code() == ErrorCode::Exists => return Err(Error::VersionExists),
		result => result?,
	};
	repo.config()?.set_str(&name_key(&slug), name.trim())?;
	repo.set_head(&format!("refs/heads/{slug}"))?;
	Ok(slug)
}

/// Returns the version's new slug.
pub(super) fn renamed_branch(root: &Path, slug: &str, name: &str) -> Result<String> {
	let repo = Repository::open(root)?;
	refuse_while_merging(&repo)?;
	if slug == main_slug(&repo)? {
		return Err(Error::MainFixed);
	}
	let renamed = self::slug(name)?;
	if renamed == MAIN || renamed == MASTER {
		return Err(Error::VersionExists);
	}
	let mut branch = repo.find_branch(slug, BranchType::Local)?;
	if renamed != slug {
		match branch.rename(&renamed, false) {
			Err(e) if e.code() == ErrorCode::Exists => return Err(Error::VersionExists),
			result => result?,
		};
	}
	repo.config()?.set_str(&name_key(&renamed), name.trim())?;
	Ok(renamed)
}

pub(super) fn dropped_branch(root: &Path, slug: &str) -> Result<()> {
	let repo = Repository::open(root)?;
	refuse_while_merging(&repo)?;
	if slug == main_slug(&repo)? {
		return Err(Error::MainFixed);
	}
	if detached(&repo)?.is_some_and(|(short, _)| short == slug) {
		return Err(Error::CurrentVersion);
	}
	let mut branch = repo.find_branch(slug, BranchType::Local)?;
	if branch.is_head() {
		return Err(Error::CurrentVersion);
	}
	branch.delete()?;
	Ok(())
}

/// Whether the project differs from its last checkpoint, on disk or in what
/// the next checkpoint holds. Stats and the Trash never count.
pub(super) fn pending(root: &Path) -> Result<bool> {
	let repo = Repository::open(root)?;
	let mut options = StatusOptions::new();
	options.include_untracked(true);
	Ok(!repo.statuses(Some(&mut options))?.is_empty())
}

/// Moves the writer into another version, refusing while there are changes
/// a checkout would overwrite.
pub(super) fn switch(root: &Path, slug: &str) -> Result<()> {
	refuse_while_merging(&Repository::open(root)?)?;
	if pending(root)? {
		return Err(Error::ChangesPending);
	}
	let repo = Repository::open(root)?;
	let target = repo
		.find_branch(slug, BranchType::Local)?
		.get()
		.peel_to_commit()?;
	let (_, was) = side(&repo, Some("HEAD"))?;
	let (files, is) = side(&repo, Some(&target.id().to_string()))?;

	// Nothing differs from `HEAD`, so a forced checkout loses no writing. It
	// removes what the target does not hold, updates the index to match, and
	// leaves ignored files alone.
	if let Err(e) = repo.checkout_tree(files.as_object(), Some(CheckoutBuilder::new().force())) {
		// Back to where the writer was, as far as that goes.
		if let Ok(Some(head)) = head_commit(&repo) {
			let _ = repo.checkout_tree(head.as_object(), Some(CheckoutBuilder::new().force()));
		}
		return Err(e.into());
	}
	// The files are the target's now, so `HEAD` says so even if a folder
	// below cannot be made.
	repo.set_head(&format!("refs/heads/{slug}"))?;
	settle_folders(root, &was, &is)
}
