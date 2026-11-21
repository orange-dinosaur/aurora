use super::*;

/// A merge waiting for its conflicts to be settled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Merging {
	/// The version being merged in, or none once no version ends at the
	/// checkpoint being merged.
	pub name: Option<String>,
	/// What the checkpoint that finishes the merge says, until the writer
	/// changes it.
	pub message: String,
	pub conflicts: Vec<merge::Conflict>,
}

/// The checkpoint being merged in, while a merge is under way.
pub(super) fn merge_head(repo: &Repository) -> Result<Option<Commit<'_>>> {
	match repo.find_reference("MERGE_HEAD") {
		Ok(merged) => Ok(Some(merged.peel_to_commit()?)),
		Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
		Err(e) => Err(e.into()),
	}
}

/// Refuses while a merge is waiting to be settled, which is when staging,
/// putting back and changing versions would settle or drop its conflicts.
pub(super) fn refuse_while_merging(repo: &Repository) -> Result<()> {
	if merge_head(repo)?.is_some() {
		return Err(Error::MergeUnderway);
	}
	Ok(())
}

/// Whether a merge is waiting to be settled. A project with no repository has
/// no versions to merge.
pub(crate) fn merge_underway(root: &Path) -> Result<bool> {
	match Repository::open(root) {
		Ok(repo) => Ok(merge_head(&repo)?.is_some()),
		Err(e) if e.code() == ErrorCode::NotFound => Ok(false),
		Err(e) => Err(e.into()),
	}
}

/// The checkpoint both versions grew from, or none for histories that never
/// met.
pub(super) fn merge_base<'r>(
	repo: &'r Repository,
	ours: &Commit,
	theirs: &Commit,
) -> Result<Option<Commit<'r>>> {
	match repo.merge_base(ours.id(), theirs.id()) {
		Ok(id) => Ok(Some(repo.find_commit(id)?)),
		Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
		Err(e) => Err(e.into()),
	}
}

pub(super) fn merge_side<'r>(
	repo: &'r Repository,
	commit: Option<&Commit<'r>>,
) -> Result<(Tree<'r>, merge::Side)> {
	let (tree, manifest) = with_manifest(repo, commit.map(Commit::tree).transpose()?)?;
	let blobs = blobs_of(&tree, &manifest);
	let cover = blob_at(&tree, &manifest.book.cover);
	Ok((
		tree,
		merge::Side {
			manifest,
			blobs,
			cover,
		},
	))
}

/// Where conflict entries keep their stage in an index entry's flags.
pub(super) const STAGE_SHIFT: u16 = 12;
pub(super) const STAGE_MASK: u16 = 0x3000;

pub(super) fn staged_entry(
	repo: &Repository,
	path: &str,
	id: Oid,
	stage: u16,
) -> Result<IndexEntry> {
	let mut entry = entry(path, id, repo.find_blob(id)?.size());
	entry.flags |= stage << STAGE_SHIFT;
	Ok(entry)
}

/// Merges another version into the one the writer is in. A clean merge adds
/// its own checkpoint, even when git could fast-forward. Otherwise the merge
/// stays under way: a document both changed in the same place keeps this
/// version's text on disk with its three sides in `.git/index`, and any other
/// conflict leaves `aurora.json` conflicted the same way.
pub(super) fn merge(root: &Path, slug: &str, author: &str) -> Result<Option<Merging>> {
	if pending(root)? {
		return Err(Error::ChangesPending);
	}
	let signature = signature(author)?;
	let repo = Repository::open(root)?;
	let Some(head) = head_commit(&repo)? else {
		return Err(Error::NothingKept);
	};
	let theirs = repo
		.find_branch(slug, BranchType::Local)?
		.get()
		.peel_to_commit()?;
	let base = merge_base(&repo, &head, &theirs)?;
	if base.as_ref().map(Commit::id) == Some(theirs.id()) {
		return Err(Error::AlreadyMerged);
	}
	if theirs.tree()?.get_path(Path::new(MANIFEST_FILE)).is_err() {
		return Err(Error::NoManifest);
	}
	let (base_tree, base) = merge_side(&repo, base.as_ref())?;
	let (our_tree, ours) = merge_side(&repo, Some(&head))?;
	let (their_tree, theirs_side) = merge_side(&repo, Some(&theirs))?;
	let merged = merge::manifests(&base, &ours, &theirs_side, &[]);

	// Git merges by path only the files no manifest names. Documents merge
	// by id, so a moved or renamed one keeps its text.
	let owned: HashSet<String> = [&base, &ours, &theirs_side]
		.iter()
		.flat_map(|side| side.manifest.documents())
		.map(|document| document.path)
		.chain(
			std::iter::once(MANIFEST_FILE)
				.chain(COVER_NAMES)
				.map(str::to_owned),
		)
		.collect();
	let others = repo.merge_trees(&base_tree, &our_tree, &their_tree, None)?;
	let mut index = repo.index()?;
	index.clear()?;
	for other in others.iter() {
		if !owned.contains(&*String::from_utf8_lossy(&other.path)) {
			index.add(&other)?;
		}
	}

	for document in merged.manifest.documents() {
		let path = &document.path;
		let id = document.id;
		let (b, o, t) = (
			base.blobs.get(&id),
			ours.blobs.get(&id),
			theirs_side.blobs.get(&id),
		);
		let (o, t) = match (o, t) {
			(Some(&o), Some(&t)) => (o, t),
			(Some(&only), None) | (None, Some(&only)) => {
				index.add(&staged_entry(&repo, path, only, 0)?)?;
				continue;
			}
			(None, None) => continue,
		};
		if let Some(Some(text)) = merge::three_way(&b.copied(), &Some(o), &Some(t)) {
			index.add(&staged_entry(&repo, path, text, 0)?)?;
			continue;
		}
		let ancestor = b.map(|&b| staged_entry(&repo, path, b, 1)).transpose()?;
		let (here, there) = (
			staged_entry(&repo, path, o, 2)?,
			staged_entry(&repo, path, t, 3)?,
		);
		let text = match &ancestor {
			Some(ancestor) => Some(repo.merge_file_from_index(ancestor, &here, &there, None)?)
				.filter(|text| text.is_automergeable()),
			None => None,
		};
		match text {
			Some(text) => {
				index.add(&staged_entry(&repo, path, repo.blob(text.content())?, 0)?)?;
			}
			None => {
				for side in ancestor.iter().chain([&here, &there]) {
					index.add(side)?;
				}
			}
		}
	}

	if let Some(cover) = merged.cover {
		index.add(&staged_entry(&repo, &merged.manifest.book.cover, cover, 0)?)?;
	}
	// From here on the project is changing, so the merge is recorded first and
	// whatever fails below leaves one to abort.
	let message = format!("Merged {}", version_name(&repo, slug)?);
	fs::write(repo.path().join("MERGE_MSG"), format!("{message}\n"))?;
	fs::write(repo.path().join("MERGE_HEAD"), format!("{}\n", theirs.id()))?;
	settle_takes(
		&repo,
		root,
		&mut index,
		&merged.manifest,
		&our_tree,
		&their_tree,
	)?;
	// With a conflict, ours here is the merge with this version's side of
	// each, so it is what the writer sees until they choose.
	let manifest = repo.blob(&to_json(&merged.manifest)?)?;
	match merged.conflicts.is_empty() {
		true => index.add(&staged_entry(&repo, MANIFEST_FILE, manifest, 0)?)?,
		false => {
			if let Some(base) = blob_at(&base_tree, MANIFEST_FILE) {
				index.add(&staged_entry(&repo, MANIFEST_FILE, base, 1)?)?;
			}
			index.add(&staged_entry(&repo, MANIFEST_FILE, manifest, 2)?)?;
			if let Some(theirs) = blob_at(&their_tree, MANIFEST_FILE) {
				index.add(&staged_entry(&repo, MANIFEST_FILE, theirs, 3)?)?;
			}
		}
	}

	// Nothing was pending, so removing untracked files only takes what the
	// merge moved away. Stats and the Trash are excluded and stay.
	index.write()?;
	repo.checkout_index(
		Some(&mut index),
		Some(
			CheckoutBuilder::new()
				.force()
				.allow_conflicts(true)
				.use_ours(true)
				.remove_untracked(true),
		),
	)?;
	settle_folders(root, &ours.manifest, &merged.manifest)?;

	if index.has_conflicts() {
		return merging(root);
	}

	let tree = repo.find_tree(index.write_tree()?)?;
	repo.commit(
		Some("HEAD"),
		&signature,
		&signature,
		&message,
		&tree,
		&[&head, &theirs],
	)?;
	repo.cleanup_state()?;
	Ok(None)
}

/// Makes `.takes/` match a merged manifest, in the index and on disk. The merge
/// screen cannot settle a path no document owns, so a conflict here is dropped
/// and the take's file comes from this version, or else the other. That also
/// returns a file one side deleted when the merge kept its take, and anything
/// no take names goes.
pub(super) fn settle_takes(
	repo: &Repository,
	root: &Path,
	index: &mut Index,
	manifest: &Manifest,
	ours: &Tree,
	theirs: &Tree,
) -> Result<()> {
	let owners: HashMap<String, Uuid> = tree::walk(&manifest.nodes)
		.flat_map(|node| match node {
			Node::Document { id, takes, .. } => takes
				.keys()
				.map(|take| (format!("{}/{take}.md", takes_folder(*id)), *id))
				.collect(),
			Node::Folder { .. } => Vec::new(),
		})
		.collect();
	let named: HashSet<String> = owners.keys().cloned().collect();
	let held: Vec<(String, u16)> = index
		.iter()
		.map(|entry| {
			(
				String::from_utf8_lossy(&entry.path).into_owned(),
				stage_of(&entry),
			)
		})
		.filter(|(path, _)| path.starts_with(&format!("{TAKES_DIR}/")))
		.collect();

	for (path, stage) in held {
		let unnamed = !named.contains(&path);
		if stage != 0 || unnamed {
			index.remove(Path::new(&path), i32::from(stage))?;
		}
		if unnamed {
			let file = root.join(&path);
			remove_if_there(&file)?;
			// The last file out takes the folder with it.
			if let Some(folder) = file.parent() {
				let _ = fs::remove_dir(folder);
			}
		}
	}
	for path in named {
		if index.get_path(Path::new(&path), 0).is_some() {
			continue;
		}
		// A take the merge made has no file in either version: it stands for
		// the document's merged text.
		let made = || -> Result<Option<Oid>> {
			let Some(document) = tree::path(&manifest.nodes, owners[&path]) else {
				return Ok(None);
			};
			let held = index
				.get_path(Path::new(&document), 0)
				.or_else(|| index.get_path(Path::new(&document), 2));
			let Some(held) = held else {
				return Ok(None);
			};
			let text = String::from_utf8_lossy(repo.find_blob(held.id)?.content()).into_owned();
			Ok(Some(repo.blob(body(&text).as_bytes())?))
		};
		let found = blob_at(ours, &path).or_else(|| blob_at(theirs, &path));
		let Some(blob) = found.map_or_else(made, |blob| Ok(Some(blob)))? else {
			continue;
		};
		index.add(&staged_entry(repo, &path, blob, 0)?)?;
		let file = root.join(&path);
		if let Some(folder) = file.parent() {
			fs::create_dir_all(folder)?;
		}
		write_atomic(&file, repo.find_blob(blob)?.content())?;
	}
	Ok(())
}

/// The longest file outside the binder a merge card shows as text.
pub(super) const SHOWN_FILE_SIZE: usize = 64 * 1024;

/// Settles a file no manifest names the way one side has it, `"mine"` or
/// `"theirs"`, on disk and in the next checkpoint.
pub(super) fn settle_file(root: &Path, path: &str, side: &str) -> Result<()> {
	let repo = Repository::open(root)?;
	let mut index = repo.index()?;
	let mut found = None;
	for conflict in index.conflicts()? {
		let conflict = conflict?;
		let sides = [&conflict.our, &conflict.their, &conflict.ancestor];
		if sides
			.into_iter()
			.flatten()
			.any(|entry| entry.path == path.as_bytes())
		{
			found = Some(conflict);
		}
	}
	// Only a path git has conflicted is touched, so the frontend cannot name
	// any other file. With no merge under way there is none.
	let conflict = found.ok_or(Error::BadDocumentPath)?;
	let kept = match side {
		"mine" => conflict.our,
		"theirs" => conflict.their,
		_ => return Err(Error::BadDocumentPath),
	};
	let file = root.join(path);
	index.conflict_remove(Path::new(path))?;
	match kept {
		Some(entry) => {
			if let Some(folder) = file.parent() {
				fs::create_dir_all(folder)?;
			}
			write_atomic(&file, repo.find_blob(entry.id)?.content())?;
			index.add(&staged_entry(&repo, path, entry.id, 0)?)?;
		}
		None => remove_if_there(&file)?,
	}
	index.write()?;
	Ok(())
}

pub(super) fn merging(root: &Path) -> Result<Option<Merging>> {
	let repo = Repository::open(root)?;
	let Some(merged) = merge_head(&repo)? else {
		return Ok(None);
	};

	let mut name = None;
	for branch in repo.branches(Some(BranchType::Local))? {
		let (branch, _) = branch?;
		if branch.get().target() == Some(merged.id())
			&& let Some(slug) = branch.name()?
		{
			name = Some(version_name(&repo, slug)?);
		}
	}

	// The merged manifest on disk places each document first, then this
	// version and the other.
	let mut ids = HashMap::new();
	let manifests = [
		read_manifest(root)?,
		side(&repo, Some("HEAD"))?.1,
		side(&repo, Some(&merged.id().to_string()))?.1,
	];
	for document in manifests.iter().flat_map(Manifest::documents) {
		ids.entry(document.path).or_insert(document.id);
	}
	let mut conflicts = Vec::new();
	let mut manifest = false;
	for conflict in repo.index()?.conflicts()? {
		let conflict = conflict?;
		let sides = [&conflict.our, &conflict.their, &conflict.ancestor];
		let Some(side) = sides.into_iter().flatten().next() else {
			continue;
		};
		let path = String::from_utf8_lossy(&side.path).into_owned();
		if path == MANIFEST_FILE {
			manifest = true;
			continue;
		}
		let shown = |side: &Option<IndexEntry>| -> Result<Option<String>> {
			let Some(side) = side else {
				return Ok(None);
			};
			let blob = repo.find_blob(side.id)?;
			Ok(Some(blob.content())
				.filter(|bytes| bytes.len() <= SHOWN_FILE_SIZE)
				.and_then(|bytes| String::from_utf8(bytes.to_vec()).ok()))
		};
		conflicts.push(match ids.get(&path) {
			Some(&id) => merge::Conflict::Text { id: Some(id), path },
			None => merge::Conflict::File {
				ours: shown(&conflict.our)?,
				theirs: shown(&conflict.their)?,
				path,
			},
		});
	}
	// The rest are worked out again from the three versions, which do not
	// change while the merge is under way.
	if manifest && let Some(head) = head_commit(&repo)? {
		let base = merge_base(&repo, &head, &merged)?;
		let (_, base) = merge_side(&repo, base.as_ref())?;
		let (_, ours) = merge_side(&repo, Some(&head))?;
		let (_, theirs) = merge_side(&repo, Some(&merged))?;
		conflicts.extend(merge::manifests(&base, &ours, &theirs, &[]).conflicts);
	}

	Ok(Some(Merging {
		name,
		message: fs::read_to_string(repo.path().join("MERGE_MSG"))
			.unwrap_or_default()
			.trim_end()
			.to_owned(),
		conflicts,
	}))
}

/// One stretch of a conflicted document: text both versions agree on, or a
/// spot where each has its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum Piece {
	Clean {
		text: String,
	},
	/// `field` names the front matter field a spot inside the head is about.
	Spot {
		ours: String,
		theirs: String,
		field: Option<String>,
	},
}

/// Long enough that no line a writer types is taken for a marker.
pub(super) const MARKER_SIZE: usize = 64;

/// A conflicted document in order, or none when it has no conflict. Git's
/// file merge marks each spot, and the label on its markers is made for this
/// call, so markers a writer typed stay text.
pub(super) fn spots(root: &Path, id: Uuid) -> Result<Option<Vec<Piece>>> {
	let repo = Repository::open(root)?;
	let path = tree::path(&read_manifest(root)?.nodes, id).ok_or(Error::UnknownDocument)?;
	let mut found = None;
	for conflict in repo.index()?.conflicts()? {
		let conflict = conflict?;
		if conflict
			.our
			.as_ref()
			.is_some_and(|our| our.path == path.as_bytes())
		{
			found = Some(conflict);
		}
	}
	let Some(conflict) = found else {
		return Ok(None);
	};

	let content = |side: &Option<IndexEntry>| -> Result<Vec<u8>> {
		Ok(match side {
			Some(side) => repo.find_blob(side.id)?.content().to_vec(),
			None => Vec::new(),
		})
	};
	let sides = [
		content(&conflict.ancestor)?,
		content(&conflict.our)?,
		content(&conflict.their)?,
	];
	let [base, ours, theirs] = sides.each_ref().map(|side| {
		let mut input = MergeFileInput::new();
		input.content(side);
		input
	});
	let label = Uuid::new_v4().to_string();
	let mut options = MergeFileOptions::new();
	options
		.our_label(&*label)
		.their_label(&*label)
		.marker_size(MARKER_SIZE as u16);
	let merged = git2::merge_file(&base, &ours, &theirs, Some(&mut options))?;
	Ok(Some(pieces(
		&String::from_utf8_lossy(merged.content()),
		&label,
	)))
}

pub(super) fn pieces(merged: &str, label: &str) -> Vec<Piece> {
	let open = format!("{} {label}", "<".repeat(MARKER_SIZE));
	// ponytail: the middle marker carries no label, so a line of exactly 64
	// "=" inside a spot splits it early; diff the sides here if that bites.
	let middle = "=".repeat(MARKER_SIZE);
	let close = format!("{} {label}", ">".repeat(MARKER_SIZE));

	let mut pieces = Vec::new();
	let (mut clean, mut ours, mut theirs) = (String::new(), String::new(), String::new());
	// Outside a spot, in this version's side, or in the other's.
	let mut side = None;
	for line in merged.split_inclusive('\n') {
		let bare = line.trim_end_matches(['\n', '\r']);
		match side {
			None if bare == open => {
				if !clean.is_empty() {
					pieces.push(Piece::Clean {
						text: std::mem::take(&mut clean),
					});
				}
				side = Some(false);
			}
			Some(false) if bare == middle => side = Some(true),
			Some(true) if bare == close => {
				pieces.push(Piece::Spot {
					ours: std::mem::take(&mut ours),
					theirs: std::mem::take(&mut theirs),
					field: None,
				});
				side = None;
			}
			None => clean.push_str(line),
			Some(false) => ours.push_str(line),
			Some(true) => theirs.push_str(line),
		}
	}
	if !clean.is_empty() {
		pieces.push(Piece::Clean { text: clean });
	}
	name_fields(&mut pieces);
	pieces
}

/// Names each spot that sits inside the front matter after the field it
/// changes, taken from the first line either side has.
pub(super) fn name_fields(pieces: &mut [Piece]) {
	let in_head = |before: &str| {
		let mut lines = before.lines();
		lines.next().is_some_and(|line| line.trim_end() == "---")
			&& lines.all(|line| line.trim_end() != "---")
	};
	let mut before = String::new();
	for piece in pieces {
		match piece {
			Piece::Clean { text } => before.push_str(text),
			Piece::Spot {
				ours,
				theirs,
				field,
			} => {
				if in_head(&before) {
					let line = if ours.is_empty() { &*theirs } else { &*ours };
					*field = line
						.lines()
						.next()
						.and_then(|line| line.split_once(':'))
						.map(|(key, _)| key)
						.filter(|key| !key.is_empty() && !key.starts_with([' ', '\t', '-', '#']))
						.map(str::to_owned);
				}
				before.push_str(ours);
			}
		}
	}
}

/// Writes a conflicted document as the writer settled it and puts it in the
/// next checkpoint, which takes it off the conflicts.
pub(super) fn settle(root: &Path, id: Uuid, text: &str) -> Result<()> {
	write_document(root.to_path_buf(), id, text.to_owned())?;
	let repo = Repository::open(root)?;
	let path = tree::path(&read_manifest(root)?.nodes, id).ok_or(Error::UnknownDocument)?;
	let mut index = repo.index()?;
	index.conflict_remove(Path::new(&path))?;
	index.add(&staged_entry(&repo, &path, repo.blob(text.as_bytes())?, 0)?)?;
	index.write()?;
	Ok(())
}

/// Merges the binder and the book again with the writer's sides, puts them in
/// the next checkpoint, and moves the files to match. A document still
/// conflicted keeps its three sides wherever it goes.
pub(super) fn settle_rest(root: &Path, theirs: &[merge::Conflict]) -> Result<()> {
	let repo = Repository::open(root)?;
	let (Some(head), Some(merged)) = (head_commit(&repo)?, merge_head(&repo)?) else {
		return Ok(());
	};
	let base = merge_base(&repo, &head, &merged)?;
	let (_, base) = merge_side(&repo, base.as_ref())?;
	let (our_tree, ours) = merge_side(&repo, Some(&head))?;
	let (their_tree, other) = merge_side(&repo, Some(&merged))?;
	let mut settled = merge::manifests(&base, &ours, &other, theirs);
	// ponytail: the binder is merged afresh, so a rename or move made while
	// the merge was under way is lost; carry `was` over if writers do that.
	// A document no checkpoint holds is the one exception: it is carried over
	// where it stands, or its file would be deleted below.
	let was = read_manifest(root)?;
	for document in was.documents() {
		let known = [&base, &ours, &other]
			.iter()
			.any(|side| tree::find(&side.manifest.nodes, document.id).is_some());
		if !known {
			take(&was.nodes, &mut settled.manifest.nodes, document.id);
		}
	}

	let paths = |manifest: &Manifest| -> HashMap<Uuid, String> {
		manifest
			.documents()
			.into_iter()
			.map(|document| (document.id, document.path))
			.collect()
	};
	let (from, to) = (paths(&was), paths(&settled.manifest));
	let mut index = repo.index()?;
	// Every file that moves is read before any is written, so one taking
	// another's old name loses nothing. The text on disk goes, so writing
	// done during the merge comes along. The old files go once the new ones
	// are down, so a failed write loses no text.
	let mut moving = Vec::new();
	let mut gone = Vec::new();
	for (id, old) in &from {
		let new = to.get(id);
		if new == Some(old) {
			continue;
		}
		let file = document_path(&was, root, old)?;
		let entries: Vec<IndexEntry> = index
			.iter()
			.filter(|entry| entry.path == old.as_bytes())
			.collect();
		for entry in &entries {
			index.remove(Path::new(old), i32::from(stage_of(entry)))?;
		}
		if let Some(new) = new {
			moving.push((new, fs::read(&file)?, entries));
			// Where the file system ignores case, a new name differing only
			// by case is this same file, which has to go before it is written.
			if new.to_lowercase() == old.to_lowercase() {
				remove_if_there(&file)?;
				continue;
			}
		}
		gone.push((old, file));
	}
	for (id, new) in &to {
		if from.contains_key(id) {
			continue;
		}
		let Some(&blob) = other.blobs.get(id).or(ours.blobs.get(id)) else {
			continue;
		};
		let entries = vec![staged_entry(&repo, new, blob, 0)?];
		moving.push((new, repo.find_blob(blob)?.content().to_vec(), entries));
	}
	let written: HashSet<&String> = moving.iter().map(|(path, ..)| *path).collect();
	for (path, text, entries) in &moving {
		let file = document_path(&settled.manifest, root, path)?;
		if let Some(folder) = file.parent() {
			fs::create_dir_all(folder)?;
		}
		write_atomic(&file, text)?;
		for entry in entries {
			index.add(&staged_entry(&repo, path, entry.id, stage_of(entry))?)?;
		}
	}
	for (old, file) in gone {
		if !written.contains(old) {
			remove_if_there(&file)?;
		}
	}

	let cover = settled
		.conflicts
		.iter()
		.any(|conflict| matches!(conflict, merge::Conflict::Cover { .. }));
	for name in COVER_NAMES.iter().filter(|_| cover) {
		match settled
			.cover
			.filter(|_| settled.manifest.book.cover == *name)
		{
			Some(blob) => {
				write_atomic(&root.join(name), repo.find_blob(blob)?.content())?;
				index.add(&staged_entry(&repo, name, blob, 0)?)?;
			}
			None => {
				remove_if_there(&root.join(name))?;
				index.remove_path(Path::new(name))?;
			}
		}
	}

	settle_takes(
		&repo,
		root,
		&mut index,
		&settled.manifest,
		&our_tree,
		&their_tree,
	)?;

	let manifest = to_json(&settled.manifest)?;
	write_atomic(&root.join(MANIFEST_FILE), &manifest)?;
	index.remove_path(Path::new(MANIFEST_FILE))?;
	let manifest = repo.blob(&manifest)?;
	index.add(&staged_entry(&repo, MANIFEST_FILE, manifest, 0)?)?;
	index.write()?;
	settle_folders(root, &was, &settled.manifest)
}

pub(super) fn stage_of(entry: &IndexEntry) -> u16 {
	(entry.flags & STAGE_MASK) >> STAGE_SHIFT
}

/// The cover the version the writer is in has, or the one being merged in.
pub(super) fn side_cover(root: &Path, theirs: bool) -> Result<Vec<u8>> {
	let repo = Repository::open(root)?;
	let commit = match theirs {
		true => merge_head(&repo)?,
		false => head_commit(&repo)?,
	};
	let (_, side) = merge_side(&repo, Some(&commit.ok_or(Error::CoverMissing)?))?;
	let cover = side.cover.ok_or(Error::CoverMissing)?;
	Ok(repo.find_blob(cover)?.content().to_vec())
}

/// Puts the project back as it was before the merge began. Without a merge
/// under way, nothing happens, so no writing is lost.
pub(super) fn abort(root: &Path) -> Result<()> {
	let repo = Repository::open(root)?;
	if merge_head(&repo)?.is_none() {
		return Ok(());
	}
	let head = head_commit(&repo)?.ok_or(Error::NothingKept)?;
	let was = read_manifest(root)?;
	repo.reset(head.as_object(), ResetType::Hard, None)?;
	repo.cleanup_state()?;
	settle_folders(root, &was, &side(&repo, Some("HEAD"))?.1)
}
