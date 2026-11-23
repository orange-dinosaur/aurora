use super::*;

/// The moment of a deletion, as the prefix on its file in the trash: sortable,
/// and made of nothing a filesystem objects to.
pub(super) fn stamp(at: OffsetDateTime) -> String {
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

/// Where the manifest's record of a trashed node waits, beside the trash and
/// at the same path as the entry it describes.
pub(super) fn node_record(root: &Path, entry: &str) -> PathBuf {
	root.join(TRASH_DIR)
		.join(NODES_DIR)
		.join(format!("{entry}.json"))
}

/// Takes away the folders left empty by an entry leaving, from the one it sat
/// in up to `top`, which stays.
pub(super) fn tidy(top: &Path, from: &Path) {
	let mut dir = from;
	while dir != top && dir.starts_with(top) && fs::remove_dir(dir).is_ok() {
		match dir.parent() {
			Some(above) => dir = above,
			None => break,
		}
	}
}

/// Moves a node into the project's trash and drops it from the manifest.
///
/// **The trash mirrors the project.** What is deleted lands at the path it sat
/// at, with the moment of the deletion in front of its own name, so a writer
/// looking in `.trash` sees the shape they deleted from. The node itself, with
/// its kind, fields, targets and order, is written as JSON to the same path
/// under `.trash/.nodes`, which is what lets a folder come back as it was. A
/// folder goes in as one directory, so what was inside it stays inside it.
/// Takes go along, to the same path under `.trash/.takes`.
///
/// The name keeps its stamp so that deleting two things called the same thing
/// does not lose the first.
pub(super) fn trash(
	root: &Path,
	mut manifest: Manifest,
	id: Uuid,
	at: OffsetDateTime,
) -> Result<()> {
	let folder = match tree::find(&manifest.nodes, id) {
		Some(Node::Folder { .. }) => true,
		Some(Node::Document { .. }) => false,
		None => return Err(Error::UnknownDocument),
	};
	// A section is the project's shape rather than something in it.
	if tree::parent(&manifest.nodes, id).is_none() {
		return Err(Error::SectionFixed);
	}

	let was = tree::path(&manifest.nodes, id).ok_or(Error::UnknownDocument)?;
	let (inside, name) = was.rsplit_once('/').ok_or(Error::BadDocumentPath)?;
	let from = if folder {
		folder_path(&manifest, root, &was)?
	} else {
		resolve(&manifest, root, id)?
	};

	// The folders above it are made in the trash as they are needed, so the
	// mirror only ever holds the paths something was actually deleted from.
	let into = root.join(TRASH_DIR).join(inside);
	// The trash is an ordinary folder a writer can replace with a symlink, so
	// where it actually leads is what decides whether this move stays inside
	// the project.
	make_inside(root, &into)?;

	let at = stamp(at);
	let mut stamped = format!("{at} {name}");
	// Two deletions within the same second would otherwise write over each
	// other.
	let mut again = 1;
	while into.join(&stamped).exists() {
		stamped = format!("{at}-{again} {name}");
		again += 1;
	}

	// The file moves first. If writing the manifest then fails, the next
	// refresh drops the node anyway, since it is no longer in the section. The
	// manifest follows at once, so a failure moving the takes cannot leave the
	// binder holding a row over a file that has gone; the takes move last,
	// from the manifest as it stood.
	fs::rename(&from, into.join(&stamped))?;
	let before = manifest.clone();
	if let Some(node) = tree::remove(&mut manifest.nodes, id) {
		let record = node_record(root, &format!("{inside}/{stamped}"));
		make_inside(root, record.parent().ok_or(Error::BadDocumentPath)?)?;
		write_json(&record, &node)?;
	}
	write_manifest(root, &mut manifest)?;
	takes::into_trash(root, &before, &was, &format!("{inside}/{stamped}"))
}

/// Deletes a document, which is to say puts it in the project's trash.
#[tauri::command]
pub fn delete_document(root: PathBuf, id: Uuid) -> Result<()> {
	let manifest = editable(&root)?;
	if !matches!(tree::find(&manifest.nodes, id), Some(Node::Document { .. })) {
		return Err(Error::UnknownDocument);
	}
	trash(&root, manifest, id, OffsetDateTime::now_utc())
}

/// Deletes a folder and everything in it. The subtree goes into the trash as
/// one directory and comes back out of it as one.
#[tauri::command]
pub fn delete_folder(root: PathBuf, id: Uuid) -> Result<()> {
	let manifest = editable(&root)?;
	if !matches!(tree::find(&manifest.nodes, id), Some(Node::Folder { .. })) {
		return Err(Error::UnknownFolder);
	}
	trash(&root, manifest, id, OffsetDateTime::now_utc())
}

/// Puts a node at a given place inside a folder, which may be the one it is
/// already in. An index past the end means the end.
///
/// Reordering and moving are one operation because they are one thing: where a
/// node sits is its parent and its place among that parent's children, and
/// changing either is the same edit. Staying put touches no files, so a
/// document whose file has gone can still be reordered; going somewhere else
/// takes the file, or the whole directory, with it.
///
/// The kind rules of [`tree::may_hold`] apply, and two rules of their own: a
/// section stays where it is, and nothing may be moved inside itself.
#[tauri::command]
pub fn move_node(root: PathBuf, id: Uuid, parent_id: Uuid, index: usize) -> Result<()> {
	let mut manifest = editable(&root)?;

	let (folder, kind, name) = match tree::find(&manifest.nodes, id) {
		Some(Node::Folder { kind, name, .. }) => (true, *kind, name.clone()),
		Some(Node::Document { name, .. }) => (false, None, name.clone()),
		None => return Err(Error::UnknownDocument),
	};

	let held = tree::parent(&manifest.nodes, id).map(Node::id);
	// The top level is the project's shape rather than something in it, and the
	// Manuscript is known by its name.
	let Some(held) = held else {
		return Err(Error::SectionFixed);
	};

	let chain = tree::trail(&manifest.nodes, parent_id).ok_or(Error::UnknownFolder)?;
	// A node cannot be put inside itself, and everything it holds is inside it.
	if chain.iter().any(|node| node.id() == id) {
		return Err(Error::MoveInsideItself);
	}
	// The section at the head of the chain is what decides whether kinds mean
	// anything where this is going.
	let in_manuscript = chain[0].name() == MANUSCRIPT;
	let (into, parent) = folder_at(&manifest, parent_id)?;
	if folder && !tree::may_hold(manifest.format, in_manuscript, parent, kind) {
		return Err(Error::FolderNotAllowed);
	}

	// The disk only comes into it when the node changes hands. A name that
	// collides is refused before anything moves.
	if parent_id != held {
		let taken = if folder {
			Error::AlreadyExists
		} else {
			Error::DocumentExists
		};
		if tree::children(&manifest.nodes, parent_id)
			.unwrap_or_default()
			.iter()
			.any(|node| node.name() == name)
		{
			return Err(taken);
		}

		let was = tree::path(&manifest.nodes, id).ok_or(Error::UnknownDocument)?;
		let path = format!("{into}/{name}");
		let (from, to) = if folder {
			(
				folder_path(&manifest, &root, &was)?,
				folder_path(&manifest, &root, &path)?,
			)
		} else {
			(
				document_path(&manifest, &root, &was)?,
				document_path(&manifest, &root, &path)?,
			)
		};

		// Where the folder above actually leads is what decides whether this
		// stays inside the project, the same as when something is made.
		let over = to.parent().ok_or(Error::BadDocumentPath)?;
		under(&root, over)?;
		if to.exists() {
			return Err(taken);
		}
		// Reordering a document whose file has gone is fine; carrying it to
		// another folder is not, because there is nothing to carry.
		if !folder && !from.exists() {
			return Err(Error::DocumentMissing);
		}

		fs::rename(&from, &to)?;
	}

	let moving = tree::remove(&mut manifest.nodes, id).ok_or(Error::UnknownDocument)?;
	let Some(Node::Folder { children, .. }) = tree::find_mut(&mut manifest.nodes, parent_id) else {
		unreachable!("the destination was a folder a moment ago")
	};
	children.insert(index.min(children.len()), moving);
	write_manifest(&root, &mut manifest)
}

/// Writes a document the manifest no longer has back into a folder, under the
/// id and name it had, so anything that still holds the id finds it again.
pub(crate) fn recreate_document(
	root: &Path,
	parent_id: Uuid,
	index: usize,
	node: Node,
	text: &[u8],
) -> Result<()> {
	let mut manifest = read_manifest(root)?;
	let into = tree::path(&manifest.nodes, parent_id).ok_or(Error::UnknownFolder)?;
	let to = document_path(&manifest, root, &format!("{into}/{}", node.name()))?;
	if to.exists() {
		return Err(Error::DocumentExists);
	}
	let folder = to.parent().ok_or(Error::BadDocumentPath)?;
	make_inside(root, folder)?;

	write_atomic(&to, text)?;
	let Some(Node::Folder { children, .. }) = tree::find_mut(&mut manifest.nodes, parent_id) else {
		return Err(Error::UnknownFolder);
	};
	children.insert(index.min(children.len()), node);
	write_manifest(root, &mut manifest)
}

/// Reads a trash file's name back: the moment it was deleted, and the name it
/// had before that. Anything that is not a stamp Aurora wrote is not one.
pub(super) fn unstamp(name: &str) -> Option<(OffsetDateTime, &str)> {
	let (token, was) = name.split_once(' ')?;
	let (date, rest) = token.split_once('-')?;
	// A second deletion in the same second carries `-1`, `-2` after the stamp.
	let (time, again) = match rest.split_once('-') {
		Some((time, again)) => (time, Some(again)),
		None => (rest, None),
	};

	fn digits(part: &str, len: usize) -> Option<&str> {
		(part.len() == len && part.bytes().all(|b| b.is_ascii_digit())).then_some(part)
	}
	let date = digits(date, 8)?;
	let time = digits(time, 6)?;
	if again.is_some_and(|n| n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit())) {
		return None;
	}
	let number = |from: usize, to: usize, of: &str| of[from..to].parse::<u32>().ok();

	let at = Date::from_calendar_date(
		number(0, 4, date)? as i32,
		Month::try_from(number(4, 6, date)? as u8).ok()?,
		number(6, 8, date)? as u8,
	)
	.ok()?
	.with_hms(
		number(0, 2, time)? as u8,
		number(2, 4, time)? as u8,
		number(4, 6, time)? as u8,
	)
	.ok()?
	.assume_utc();

	Some((at, was))
}

/// How many documents a trashed folder holds, at any depth. What the writer
/// wants to know before putting it back is how much of their work is in there.
pub(super) fn documents_inside(dir: &Path) -> Result<u32> {
	let mut total = 0;
	for entry in fs::read_dir(dir)? {
		let entry = entry?;
		let name = entry.file_name();
		let Some(name) = name.to_str() else {
			continue;
		};
		let kind = entry.file_type()?;
		if kind.is_dir() {
			total += documents_inside(&entry.path())?;
		} else if kind.is_file() && is_markdown(name) {
			total += 1;
		}
	}
	Ok(total)
}

/// One level of the trash, and every level under it that a deletion made on its
/// way in. A stamped name is what was actually deleted, so it becomes an entry
/// and the walk does not go inside it: a chapter of fourteen scenes is one row,
/// not fifteen.
pub(super) fn gather_trash(
	dir: &Path,
	at: &str,
	section: &str,
	found: &mut Vec<TrashEntry>,
) -> Result<()> {
	let entries = match fs::read_dir(dir) {
		Ok(entries) => entries,
		Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
		Err(e) => return Err(e.into()),
	};

	for entry in entries {
		let entry = entry?;
		let name = entry.file_name();
		let Some(name) = name.to_str() else {
			continue;
		};
		if name.starts_with('.') {
			continue;
		}

		// A symlink is neither a document nor a folder, here as anywhere else.
		let kind = entry.file_type()?;
		let directory = kind.is_dir();
		if !(directory || (kind.is_file() && is_markdown(name))) {
			continue;
		}

		let path = format!("{at}/{name}");
		match unstamp(name) {
			Some((deleted, was)) => found.push(TrashEntry {
				folder: section.to_owned(),
				title: without_extension(was).to_owned(),
				deleted: Some(deleted),
				inside: directory
					.then(|| documents_inside(&entry.path()))
					.transpose()?,
				path,
			}),
			// An unstamped directory is one the mirror needed, so what was
			// deleted is further down.
			None if directory => gather_trash(&entry.path(), &path, section, found)?,
			// A file somebody put there by hand is still shown, so it can at
			// least be got rid of.
			None => found.push(TrashEntry {
				folder: section.to_owned(),
				title: without_extension(name).to_owned(),
				deleted: None,
				inside: None,
				path,
			}),
		}
	}

	Ok(())
}

/// Everything in the project's trash, most recently deleted first.
#[tauri::command]
pub fn list_trash(root: PathBuf) -> Result<Vec<TrashEntry>> {
	absolute(&root)?;

	let manifest = read_manifest(&root)?;
	let trash = root.join(TRASH_DIR);
	let mut entries = Vec::new();
	for section in manifest.folders() {
		gather_trash(&trash.join(&section), &section, &section, &mut entries)?;
	}

	// Newest first, with anything undated behind the rest.
	entries.sort_by(|a, b| b.deleted.cmp(&a.deleted).then_with(|| a.path.cmp(&b.path)));
	Ok(entries)
}

/// Something in the trash, once the project is satisfied it is really in there.
/// A trashed folder is a directory and a trashed document is a file, so the
/// path is walked without holding its last part to the `.md` rule.
pub(super) fn trash_entry(manifest: &Manifest, root: &Path, path: &str) -> Result<PathBuf> {
	let file = folder_path(manifest, &root.join(TRASH_DIR), path)?;
	if !file.exists() {
		return Err(Error::DocumentMissing);
	}
	// The trash is an ordinary folder a writer can replace with a symlink, so
	// an entry has to lead somewhere under the trash itself.
	under(&root.join(TRASH_DIR), &file)?;
	Ok(file)
}

/// The deepest of the folders on this path that the project still has. The
/// section at its head is the last resort, and something the writer deleted
/// three folders down comes back as near to where it was as is left.
pub(super) fn nearest_folder(manifest: &Manifest, path: &str) -> String {
	let mut level = manifest.nodes.as_slice();
	let mut matched: Vec<&str> = Vec::new();

	for part in path.split('/') {
		let Some(Node::Folder { children, .. }) = level.iter().find(|node| node.name() == part)
		else {
			break;
		};
		matched.push(part);
		level = children;
	}

	if matched.is_empty() {
		path.split('/').next().unwrap_or_default().to_owned()
	} else {
		matched.join("/")
	}
}

/// Puts a deleted document or folder back where it came from, under the name it
/// had. The trash mirrors the project, so where it came from is the path it is
/// sitting at with the stamp taken off. A folder it was inside may have been
/// deleted too, and then it goes to the nearest one still standing rather than
/// failing. Anything already using that name is not written over.
#[tauri::command]
pub fn restore_from_trash(root: PathBuf, path: String) -> Result<()> {
	let manifest = editable(&root)?;
	let from = trash_entry(&manifest, &root, &path)?;

	let (inside, file) = path.rsplit_once('/').ok_or(Error::BadDocumentPath)?;
	let was = unstamp(file).map_or(file, |(_, was)| was);
	let back = format!("{}/{was}", nearest_folder(&manifest, inside));

	let directory = from.is_dir();
	let to = if directory {
		folder_path(&manifest, &root, &back)?
	} else {
		document_path(&manifest, &root, &back)?
	};
	if to.exists() {
		return Err(if directory {
			Error::AlreadyExists
		} else {
			Error::DocumentExists
		});
	}

	let folder = to.parent().ok_or(Error::BadDocumentPath)?;
	make_inside(&root, folder)?;

	fs::rename(&from, &to)?;

	// A record of the node lets it come back as it was, ids, kind, fields,
	// targets and order. One that cannot be read, or an entry from before
	// records were kept, is adopted from the disk like anything else.
	let record = node_record(&root, &path);
	let landed = nearest_folder(&manifest, inside);
	if let Some(node) = fs::read(&record)
		.ok()
		.and_then(|bytes| serde_json::from_slice::<Node>(&bytes).ok())
	{
		put_node_back(&root, &landed, node, was)?;
	}

	// Anything the record did not cover is adopted the way anything that
	// appears in a section is: at the end of the folder it landed in, and
	// inside a restored folder in the order the disk lists it.
	refresh(&root)?;
	takes::out_of_trash(&root, &path, &back)?;

	remove_if_there(&record)?;
	let trash = root.join(TRASH_DIR);
	tidy(&trash, from.parent().unwrap_or(&trash));
	tidy(&trash.join(NODES_DIR), record.parent().unwrap_or(&trash));
	Ok(())
}

/// Adds a node restored from the trash at the end of the folder at `landed`.
/// A folder's kind is dropped where the rules no longer let it stand.
pub(super) fn put_node_back(root: &Path, landed: &str, mut node: Node, name: &str) -> Result<()> {
	let mut manifest = read_manifest(root)?;
	let Some((parent, parent_kind)) =
		tree::walk(&manifest.nodes).find_map(|candidate| match candidate {
			Node::Folder { id, kind, .. }
				if tree::path(&manifest.nodes, *id).as_deref() == Some(landed) =>
			{
				Some((*id, *kind))
			}
			_ => None,
		})
	else {
		return Ok(());
	};

	let in_manuscript = landed.split('/').next() == Some(MANUSCRIPT);
	match &mut node {
		Node::Folder {
			kind, name: held, ..
		} => {
			if !tree::may_hold(manifest.format, in_manuscript, parent_kind, *kind) {
				*kind = None;
			}
			name.clone_into(held);
		}
		Node::Document { name: held, .. } => name.clone_into(held),
	}

	if let Some(Node::Folder { children, .. }) = tree::find_mut(&mut manifest.nodes, parent) {
		children.push(node);
	}
	write_manifest(root, &mut manifest)
}

/// Throws one thing in the trash away for good, a folder with everything that
/// went into it.
#[tauri::command]
pub fn purge_trash_entry(root: PathBuf, path: String) -> Result<()> {
	absolute(&root)?;

	let manifest = read_manifest(&root)?;
	let file = trash_entry(&manifest, &root, &path)?;
	if file.is_dir() {
		fs::remove_dir_all(&file)?;
	} else {
		fs::remove_file(&file)?;
	}
	takes::purge(&root, &path)?;

	let record = node_record(&root, &path);
	remove_if_there(&record)?;
	let trash = root.join(TRASH_DIR);
	tidy(&trash, file.parent().unwrap_or(&trash));
	tidy(&trash.join(NODES_DIR), record.parent().unwrap_or(&trash));
	Ok(())
}
