use super::*;

/// Turns a document id into a path on disk, refusing anything that does not
/// end up inside the project. `aurora.json` is an ordinary file a writer can
/// edit, so the path it records is not to be trusted.
pub(crate) fn resolve(manifest: &Manifest, root: &Path, id: Uuid) -> Result<PathBuf> {
	let relative = match tree::find(&manifest.nodes, id) {
		Some(Node::Document { .. }) => {
			tree::path(&manifest.nodes, id).ok_or(Error::UnknownDocument)?
		}
		// A folder is not a document, and neither is an id nothing carries.
		_ => return Err(Error::UnknownDocument),
	};

	within(&root.canonicalize()?, &relative)
}

/// A document's path under a root that is already canonical. Canonicalising
/// both sides resolves `..` and follows symlinks, so the comparison is
/// between two real locations. Tree reads call this with one root for every
/// document rather than canonicalising it again each time.
pub(super) fn within(base: &Path, relative: &str) -> Result<PathBuf> {
	let path = base
		.join(relative)
		.canonicalize()
		.map_err(|e| match e.kind() {
			io::ErrorKind::NotFound => Error::DocumentMissing,
			_ => Error::Io(e),
		})?;

	if !path.starts_with(base) {
		return Err(Error::OutsideProject);
	}

	Ok(path)
}

/// An existing `path`, once it is known to lead somewhere under `base`.
pub(super) fn under(base: &Path, path: &Path) -> Result<PathBuf> {
	let path = path.canonicalize()?;
	if !path.starts_with(base.canonicalize()?) {
		return Err(Error::OutsideProject);
	}
	Ok(path)
}

/// Makes `dir` and what is missing above it, but only once the nearest part
/// that exists is known to lead under `base`. Checking afterwards would leave
/// the folders already made at the end of a symlink.
pub(super) fn make_inside(base: &Path, dir: &Path) -> Result<()> {
	let near = dir
		.ancestors()
		.find(|above| above.exists())
		.ok_or(Error::BadDocumentPath)?;
	under(base, near)?;
	fs::create_dir_all(dir)?;
	Ok(())
}

/// Refuses a project folder given as a relative path.
pub(super) fn absolute(root: &Path) -> Result<()> {
	if root.is_absolute() {
		Ok(())
	} else {
		Err(Error::RelativePath)
	}
}

/// Turns a relative path into somewhere this project is willing to keep a
/// document: a Markdown file under one of the format's sections, at whatever
/// depth. `base` is the project root, or the trash inside it, which is laid out
/// the same way. The path may come from the frontend or be walked out of the
/// manifest, and neither is trusted.
pub(crate) fn document_path(manifest: &Manifest, base: &Path, path: &str) -> Result<PathBuf> {
	project_path(manifest, base, path, true)
}

/// The same walk for a directory, whose last part is a folder name rather than
/// a file and so is not Markdown.
pub(super) fn folder_path(manifest: &Manifest, base: &Path, path: &str) -> Result<PathBuf> {
	project_path(manifest, base, path, false)
}

pub(super) fn project_path(
	manifest: &Manifest,
	base: &Path,
	path: &str,
	markdown: bool,
) -> Result<PathBuf> {
	let Some((section, rest)) = path.split_once('/') else {
		return Err(Error::BadDocumentPath);
	};

	let ordinary = |part: &str| {
		!part.starts_with('.')
			&& !part.contains('\\')
			&& matches!(
				Path::new(part).components().collect::<Vec<_>>()[..],
				[std::path::Component::Normal(_)]
			)
	};

	let mut parts = rest.split('/').peekable();
	let mut file = base.join(section);
	if !ordinary(section) || !manifest.folders().iter().any(|folder| folder == section) {
		return Err(Error::BadDocumentPath);
	}

	while let Some(part) = parts.next() {
		// Only the last part is the file, and only it has to be Markdown.
		if !ordinary(part) || (markdown && parts.peek().is_none() && !is_markdown(part)) {
			return Err(Error::BadDocumentPath);
		}
		file.push(part);
	}

	Ok(file)
}
