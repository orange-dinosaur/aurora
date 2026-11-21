use super::*;

/// Putting back writes files only, so what it changed shows as changes not
/// yet in the next checkpoint.
pub(super) fn open_for_putting_back(root: &Path) -> Result<Repository> {
	if !root.is_absolute() {
		return Err(Error::RelativePath);
	}
	let repo = Repository::open(root)?;
	refuse_while_merging(&repo)?;
	Ok(repo)
}

/// Keeps changes not yet in a checkpoint, so putting back never loses them.
/// Putting back to the last checkpoint is how the writer drops those changes,
/// so that keeps nothing.
pub(super) fn keep_before_putting_back(root: &Path, version: &str) -> Result<()> {
	if version != "HEAD" && pending(root)? {
		// Everything, not just what the writer staged, or the rest still goes.
		stage(&Repository::open(root)?)?;
		checkpoint(root, "Before putting back", &author(root)?)?;
	}
	Ok(())
}

/// Where a version had a document: the nearest of its old folders the project
/// still has, and its place there, after the document it used to follow if
/// that is in the same folder now.
pub(super) fn destination(then: &Manifest, now: &Manifest, id: Uuid) -> Result<(Uuid, usize)> {
	let trail = tree::trail(&then.nodes, id).ok_or(Error::UnknownDocument)?;
	let parent = trail
		.iter()
		.rev()
		.skip(1)
		.map(|node| node.id())
		.find(|folder| tree::children(&now.nodes, *folder).is_some())
		.ok_or(Error::UnknownFolder)?;
	let siblings = tree::parent(&then.nodes, id)
		.and_then(|above| tree::children(&then.nodes, above.id()))
		.unwrap_or_default();
	let children = tree::children(&now.nodes, parent).unwrap_or_default();
	let at = siblings
		.iter()
		.position(|node| node.id() == id)
		.unwrap_or(0);
	let index = siblings[..at]
		.iter()
		.rev()
		.find_map(|sibling| children.iter().position(|c| c.id() == sibling.id()))
		.map_or(0, |before| before + 1);
	Ok((parent, index))
}

/// Puts one document back as a version had it: its text, its name, its folder
/// and its takes. One the version did not have goes to the Trash, and one it
/// had that has since been deleted comes back under its old id.
#[tauri::command]
pub fn put_back_document(root: PathBuf, version: String, id: Uuid) -> Result<()> {
	let repo = open_for_putting_back(&root)?;
	keep_before_putting_back(&root, &version)?;
	let (files, then) = side(&repo, Some(&version))?;
	let now = read_manifest(&root)?;
	let Some(node @ Node::Document { takes, chosen, .. }) = tree::find(&then.nodes, id) else {
		return match tree::find(&now.nodes, id) {
			Some(Node::Document { .. }) => delete_document(root, id),
			_ => Err(Error::UnknownDocument),
		};
	};

	let path = tree::path(&then.nodes, id).ok_or(Error::UnknownDocument)?;
	let blob = blob_at(&files, &path).ok_or(Error::DocumentMissing)?;
	let text = repo.find_blob(blob)?.content().to_vec();
	let mut take_texts = BTreeMap::new();
	for take in takes.keys() {
		let path = format!("{}/{take}.md", takes_folder(id));
		let blob = blob_at(&files, &path).ok_or(Error::DocumentMissing)?;
		take_texts.insert(*take, repo.find_blob(blob)?.content().to_vec());
	}

	let (parent, index) = destination(&then, &now, id)?;
	match tree::find(&now.nodes, id) {
		None => recreate_document(&root, parent, index, node.clone(), &text)?,
		Some(Node::Folder { .. }) => return Err(Error::UnknownDocument),
		Some(Node::Document { name, .. }) => {
			let text = String::from_utf8(text).map_err(|_| Error::NotText)?;
			let here = tree::parent(&now.nodes, id).map(Node::id);
			let moving = here != Some(parent);
			// Another document here may have the old name by now, so it is
			// taken back only once this one has left.
			let taken_here = here
				.and_then(|here| tree::children(&now.nodes, here))
				.unwrap_or_default()
				.iter()
				.any(|other| other.id() != id && other.name() == node.name());
			if moving && taken_here {
				move_node(root.clone(), id, parent, index)?;
			}
			if name != node.name() {
				rename_document(root.clone(), id, node.name().to_owned())?;
			}
			if moving && !taken_here {
				move_node(root.clone(), id, parent, index)?;
			}
			write_document(root.clone(), id, text)?;
		}
	}
	restore(&root, id, takes.clone(), *chosen, &take_texts)
}

/// The names of the takes that putting a document back would remove for good,
/// in order. A document the version lacks goes to the Trash with its takes, so
/// that removes none.
#[tauri::command]
pub fn put_back_removes(root: PathBuf, version: String, id: Uuid) -> Result<Vec<String>> {
	let repo = open_for_putting_back(&root)?;
	let (_, then) = side(&repo, Some(&version))?;
	let now = read_manifest(&root)?;
	let (Some(Node::Document { takes: kept, .. }), Some(Node::Document { takes, .. })) =
		(tree::find(&then.nodes, id), tree::find(&now.nodes, id))
	else {
		return Ok(Vec::new());
	};
	Ok(in_order(takes)
		.into_iter()
		.filter(|(take, _)| !kept.contains_key(take))
		.map(|(_, details)| details.name.clone())
		.collect())
}

/// The takes that putting the whole project back would remove for good, in
/// binder order and ready to show, as "Take 3 of Low water". Here a document
/// the version lacks goes with its takes rather than to the Trash, so they count.
#[tauri::command]
pub fn put_back_everything_removes(root: PathBuf, version: String) -> Result<Vec<String>> {
	let repo = open_for_putting_back(&root)?;
	let (_, then) = side(&repo, Some(&version))?;
	let now = read_manifest(&root)?;

	let mut gone = Vec::new();
	for document in now.documents() {
		let Some(Node::Document { takes, .. }) = tree::find(&now.nodes, document.id) else {
			continue;
		};
		let kept = match tree::find(&then.nodes, document.id) {
			Some(Node::Document { takes, .. }) => Some(takes),
			_ => None,
		};
		let name = document.title();
		gone.extend(
			in_order(takes)
				.into_iter()
				.filter(|(take, _)| !kept.is_some_and(|kept| kept.contains_key(take)))
				.map(|(_, details)| format!("{} of {name}", details.name)),
		);
	}
	Ok(gone)
}

/// Puts back the book's details and its cover as a version had them.
#[tauri::command]
pub fn put_back_book(root: PathBuf, version: String) -> Result<()> {
	let repo = open_for_putting_back(&root)?;
	keep_before_putting_back(&root, &version)?;
	let (files, then) = side(&repo, Some(&version))?;
	for name in COVER_NAMES {
		match blob_at(&files, name).filter(|_| then.book.cover == name) {
			Some(blob) => write_atomic(&root.join(name), repo.find_blob(blob)?.content())?,
			None => remove_if_there(&root.join(name))?,
		}
	}
	let mut manifest = read_manifest(&root)?;
	manifest.book = Book {
		identifier: manifest.book.identifier,
		..then.book
	};
	write_manifest(&root, &mut manifest)
}

/// Puts the whole project back as a version had it. Stats and the Trash are
/// never part of a version, so they stay as they are.
#[tauri::command]
pub fn put_back_everything(root: PathBuf, version: String) -> Result<()> {
	let repo = open_for_putting_back(&root)?;
	keep_before_putting_back(&root, &version)?;
	let (files, then) = side(&repo, Some(&version))?;
	let was = read_manifest(&root)?;

	// Removing untracked files takes documents added since the last
	// checkpoint too. Stats and the Trash are excluded, so they stay.
	repo.checkout_tree(
		files.as_object(),
		Some(
			CheckoutBuilder::new()
				.force()
				.remove_untracked(true)
				.update_index(false),
		),
	)?;
	settle_folders(&root, &was, &then)
}

/// Git keeps files, not folders, so after a checkout the folders `was` had
/// and `is` does not are taken away, and empty ones `is` has are made.
pub(super) fn settle_folders(root: &Path, was: &Manifest, is: &Manifest) -> Result<()> {
	let folders = |manifest: &Manifest| {
		let mut placed = Vec::new();
		place(&manifest.nodes, None, "", &mut placed);
		placed
			.into_iter()
			.filter(|(_, node)| node.folder)
			.map(|(id, node)| (id, node.path))
			// The manifest is a file the writer can edit.
			.filter(|(_, path)| {
				Path::new(path)
					.components()
					.all(|c| matches!(c, Component::Normal(_)))
			})
			.collect::<Vec<_>>()
	};
	let (was, is) = (folders(was), folders(is));
	// A folder renamed only in case is the same folder to a filesystem that
	// ignores case, so the checkout left it under its old name.
	let new: HashMap<Uuid, &String> = is.iter().map(|(id, path)| (*id, path)).collect();
	for (id, old) in &was {
		if let Some(new) = new.get(id)
			&& *new != old
			&& new.to_lowercase() == old.to_lowercase()
		{
			// On a filesystem that tells case apart, both exist and this fails.
			let _ = fs::rename(root.join(old), root.join(new));
		}
	}
	let was: Vec<String> = was.into_iter().map(|(_, path)| path).collect();
	let is: Vec<String> = is.into_iter().map(|(_, path)| path).collect();
	let mut gone: Vec<&String> = was.iter().filter(|path| !is.contains(path)).collect();
	gone.sort_by_key(|path| std::cmp::Reverse(path.len()));
	for path in gone {
		// Only an empty folder goes, so a failure leaves nothing lost.
		let _ = fs::remove_dir(root.join(path));
	}
	for path in &is {
		fs::create_dir_all(root.join(path))?;
	}
	Ok(())
}
