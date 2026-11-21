use super::*;

/// One row of a history of checkpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Checkpoint {
	pub id: String,
	#[serde(with = "time::serde::rfc3339")]
	#[ts(type = "string")]
	pub at: OffsetDateTime,
	pub message: String,
	pub author: String,
	/// The document's words at this checkpoint, when the list is of one document.
	pub words: Option<u32>,
}

/// The blob holding a document's text in one version, found by its id in
/// that version's manifest so a rename or a move does not lose it. None when
/// the document is not there, or the manifest cannot be read.
///
/// Most versions share their manifest with the one before, so `paths`
/// remembers the document's path by manifest and each is parsed once.
pub(super) fn text_in(
	repo: &Repository,
	commit: &Commit,
	document: Uuid,
	paths: &mut HashMap<Oid, Option<String>>,
) -> Result<Option<Oid>> {
	let tree = commit.tree()?;
	let Ok(entry) = tree.get_path(Path::new(MANIFEST_FILE)) else {
		return Ok(None);
	};
	if let Entry::Vacant(slot) = paths.entry(entry.id()) {
		let blob = repo.find_blob(entry.id())?;
		let path = serde_json::from_slice::<Manifest>(blob.content())
			.ok()
			.and_then(|manifest| {
				manifest
					.documents()
					.into_iter()
					.find(|d| d.id == document)
					.map(|d| d.path)
			});
		slot.insert(path);
	}
	let Some(path) = &paths[&entry.id()] else {
		return Ok(None);
	};
	Ok(tree.get_path(Path::new(path)).ok().map(|e| e.id()))
}

/// `start` and the commits before it along first parents, newest first.
pub(super) fn first_parents(repo: &Repository, start: Oid) -> Result<git2::Revwalk<'_>> {
	let mut walk = repo.revwalk()?;
	walk.set_sorting(Sort::TOPOLOGICAL)?;
	walk.simplify_first_parent()?;
	walk.push(start)?;
	Ok(walk)
}

/// The document's words in `commit` when its text differs from the first
/// parent's, 0 when it was removed, and none when its text is unchanged.
pub(super) fn changed_words(
	repo: &Repository,
	commit: &Commit,
	document: Uuid,
	paths: &mut HashMap<Oid, Option<String>>,
) -> Result<Option<u32>> {
	let before = match commit.parent(0) {
		Ok(parent) => text_in(repo, &parent, document, paths)?,
		Err(_) => None,
	};
	let now = text_in(repo, commit, document, paths)?;
	if now == before {
		return Ok(None);
	}
	Ok(Some(match now {
		Some(blob) => words_in(repo.find_blob(blob)?.content()),
		None => 0,
	}))
}

/// Commits what the next checkpoint holds with the writer's message, or the
/// whole project as it is when nothing is staged, and returns the new
/// checkpoint.
pub(super) fn checkpoint(root: &Path, message: &str, author: &str) -> Result<Oid> {
	if message.trim().is_empty() {
		return Err(Error::EmptyMessage);
	}
	let repo = Repository::open(root)?;
	if repo.index()?.has_conflicts() {
		return Err(Error::ConflictsRemain);
	}
	let staged = compare(&side(&repo, Some("HEAD"))?, &staged_side(&repo)?);
	let tree = match staged.nodes.is_empty() && !staged.book && !staged.cover {
		true => stage(&repo)?,
		false => repo.index()?.write_tree()?,
	};
	let tree = repo.find_tree(tree)?;
	let signature = signature(author)?;
	let head = head_commit(&repo)?;
	let merged = merge_head(&repo)?;
	if merged.is_none()
		&& head
			.as_ref()
			.is_some_and(|head| head.tree_id() == tree.id())
	{
		return Err(Error::NothingToKeep);
	}
	let parents: Vec<&Commit> = head.iter().chain(merged.iter()).collect();
	let id = repo.commit(
		Some("HEAD"),
		&signature,
		&signature,
		message,
		&tree,
		&parents,
	)?;
	// Ends the merge under way, if there is one.
	repo.cleanup_state()?;
	Ok(id)
}

/// The checkpoints of a version, newest first along first parents, or of the
/// one the writer is in when `version` is none. A version with nothing in it
/// yet has no rows. With a document, only the checkpoints that changed its
/// text.
pub(super) fn checkpoints(
	root: &Path,
	version: Option<&str>,
	document: Option<Uuid>,
) -> Result<Vec<Checkpoint>> {
	let repo = Repository::open(root)?;
	let start = match version {
		None => head_commit(&repo)?,
		Some(name) => match repo.find_branch(name, BranchType::Local) {
			Ok(branch) => Some(branch.get().peel_to_commit()?),
			Err(e) if e.code() == ErrorCode::NotFound => None,
			Err(e) => return Err(e.into()),
		},
	};
	let Some(start) = start else {
		return Ok(Vec::new());
	};

	let mut rows = Vec::new();
	let mut paths = HashMap::new();
	for id in first_parents(&repo, start.id())? {
		let commit = repo.find_commit(id?)?;
		let words = match document {
			Some(document) => match changed_words(&repo, &commit, document, &mut paths)? {
				Some(words) => Some(words),
				None => continue,
			},
			None => None,
		};
		rows.push(Checkpoint {
			id: commit.id().to_string(),
			at: OffsetDateTime::from_unix_timestamp(commit.time().seconds())
				.unwrap_or(OffsetDateTime::UNIX_EPOCH),
			message: String::from_utf8_lossy(commit.message_bytes())
				.trim_end()
				.to_owned(),
			author: commit.author().name().unwrap_or_default().to_owned(),
			words,
		});
	}
	Ok(rows)
}

/// A document's full text at a version, or none when it is not there.
pub fn text(root: &Path, version: &str, document: Uuid) -> Result<Option<String>> {
	let repo = Repository::open(root)?;
	let commit = repo.find_commit(Oid::from_str(version)?)?;
	let Some(blob) = text_in(&repo, &commit, document, &mut HashMap::new())? else {
		return Ok(None);
	};
	let blob = repo.find_blob(blob)?;
	Ok(Some(String::from_utf8_lossy(blob.content()).into_owned()))
}
