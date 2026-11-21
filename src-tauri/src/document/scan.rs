use super::*;

/// What each of the project's sections holds, as a tree: every `.md` file
/// under it and every directory on the way to one, however deep. Sections keep
/// the order they are given and what a directory holds is sorted by name.
/// Anything else is ignored: other extensions, symlinks, hidden names and names
/// that are not valid UTF-8.
///
/// The disk has no ids to give, so every node comes back with a fresh one.
/// Matching what is here against what the project already knows is
/// `reconcile`'s job.
pub fn scan(root: &Path, folders: &[String]) -> Result<Vec<Node>> {
	let mut sections = Vec::new();

	for folder in folders {
		// A section the writer deleted is not an error; it simply is not there.
		let Some(children) = read_level(&root.join(folder))? else {
			continue;
		};
		sections.push(Node::Folder {
			id: Uuid::new_v4(),
			name: folder.clone(),
			kind: None,
			target: None,
			fields: Fields::new(),
			children,
		});
	}

	Ok(sections)
}

/// What one directory holds, or `None` when there is no such directory.
pub(super) fn read_level(dir: &Path) -> Result<Option<Vec<Node>>> {
	let entries = match fs::read_dir(dir) {
		Ok(entries) => entries,
		Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
		Err(e) => return Err(e.into()),
	};

	let mut found = Vec::new();
	for entry in entries {
		let entry = entry?;
		let name = entry.file_name();
		let Some(name) = name.to_str() else {
			continue;
		};
		// The trash lives inside the project without being part of it, and
		// nothing else hidden is the writer's work either.
		if name.starts_with('.') {
			continue;
		}

		// A symlink is neither, so it is passed over as it always has been.
		let entry_type = entry.file_type()?;
		if entry_type.is_dir() {
			found.push(Node::Folder {
				id: Uuid::new_v4(),
				name: name.to_owned(),
				kind: None,
				target: None,
				fields: Fields::new(),
				children: read_level(&entry.path())?.unwrap_or_default(),
			});
		} else if entry_type.is_file() && is_markdown(name) {
			found.push(Node::document(name));
		}
	}

	found.sort_by(|a, b| a.name().cmp(b.name()));
	Ok(Some(found))
}

pub(super) fn is_markdown(name: &str) -> bool {
	Path::new(name)
		.extension()
		.is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

/// The folders a document sits in, from its section down: `Manuscript`, `Part
/// One`, `Chapter 3`. This is the one thing a view says about where a document
/// is rather than what it is, and taking the path apart happens here so that
/// nothing outside Rust has to.
pub(super) fn trail_of(path: &str) -> Vec<String> {
	let mut names: Vec<String> = path.split('/').map(str::to_owned).collect();
	names.pop();
	names
}

/// Brings the manifest's tree back in step with what `scan` found. The
/// project's sections are its own and stay whether or not their folders are
/// still on disk; under them, a node whose file or directory is still there
/// keeps its id and its place, a vanished one is dropped, and anything the
/// project has not seen is adopted at the end of the level it was found in.
/// Returns whether anything changed.
pub fn reconcile(manifest: &mut Manifest, found: &[Node]) -> bool {
	let mut sections = Vec::new();
	let mut seen: HashSet<&str> = HashSet::new();

	for section in &manifest.nodes {
		// Only folders belong at the top of a project. A document up there is
		// something hand-editing put in, and it is not carried over.
		let Node::Folder {
			id,
			name,
			kind,
			target,
			fields,
			children,
		} = section
		else {
			continue;
		};
		// A section recorded twice — again only by hand — would otherwise show
		// twice in the sidebar over the one folder.
		if !seen.insert(name.as_str()) {
			continue;
		}

		// A section folder the writer deleted keeps its place in the project.
		// What was inside it has gone, which reads the same as an empty one.
		let below = match found.iter().find(|node| node.name() == name) {
			Some(Node::Folder { children, .. }) => children.as_slice(),
			_ => &[],
		};

		// The fields come from the manifest and never from disk: a folder is a
		// directory out there, and a directory says nothing about itself.
		sections.push(Node::Folder {
			id: *id,
			name: name.clone(),
			kind: *kind,
			target: *target,
			fields: fields.clone(),
			children: merge(children, below),
		});
	}

	let changed = sections != manifest.nodes;
	manifest.nodes = sections;
	// A matter folder the writer made in a file manager arrives here with no
	// kind, the way it would in a project written before kinds existed.
	adopt_matter(&mut manifest.nodes) || changed
}

/// One level of the project against the same level on disk.
pub(super) fn merge(known: &[Node], found: &[Node]) -> Vec<Node> {
	let mut merged = Vec::new();
	let mut seen: HashSet<&str> = HashSet::new();

	// One thing gone and one new thing of the same kind in the same folder is
	// a rename made outside Aurora, so the known node keeps its id, target and
	// place under the new name. Any other count is left as additions and
	// removals, since nothing says which went with which.
	let known_names: HashSet<&str> = known.iter().map(Node::name).collect();
	let missing: Vec<&Node> = known
		.iter()
		.filter(|node| !found.iter().any(|other| other.name() == node.name()))
		.collect();
	let unknown: Vec<&Node> = found
		.iter()
		.filter(|node| !known_names.contains(node.name()))
		.collect();
	let renamed = match (missing[..].first(), unknown[..].first()) {
		(Some(gone), Some(new))
			if missing.len() == 1
				&& unknown.len() == 1
				&& std::mem::discriminant(*gone) == std::mem::discriminant(*new) =>
		{
			Some((*gone, *new))
		}
		_ => None,
	};

	for node in known {
		// A name recorded twice would otherwise be two nodes over one file.
		if !seen.insert(node.name()) {
			continue;
		}
		let Some(on_disk) = found
			.iter()
			.find(|other| other.name() == node.name())
			.or_else(|| {
				renamed
					.filter(|(gone, _)| std::ptr::eq(*gone, node))
					.map(|(_, new)| new)
			})
		else {
			continue;
		};
		seen.insert(on_disk.name());

		merged.push(match (node, on_disk) {
			(
				Node::Folder {
					id,
					kind,
					target,
					fields,
					children,
					..
				},
				Node::Folder {
					name,
					children: below,
					..
				},
			) => Node::Folder {
				id: *id,
				name: name.clone(),
				kind: *kind,
				target: *target,
				fields: fields.clone(),
				children: merge(children, below),
			},
			(Node::Document { .. }, Node::Document { name: new, .. }) => {
				let mut kept = node.clone();
				if let Node::Document { name, .. } = &mut kept {
					name.clone_from(new);
				}
				kept
			}
			// A name that was a file and is now a directory, or the other way
			// about, is a new thing under an old name. What is on disk wins.
			_ => on_disk.clone(),
		});
	}

	for node in found {
		if seen.insert(node.name()) {
			merged.push(node.clone());
		}
	}

	merged
}

/// Reads the manifest and brings it back in step with the folder, writing it
/// back only when reconciliation actually changed something.
pub fn refresh(root: &Path) -> Result<Manifest> {
	let mut manifest = read_manifest(root)?;
	let found = scan(root, &manifest.folders())?;
	if reconcile(&mut manifest, &found) {
		write_manifest(root, &mut manifest)?;
	}
	Ok(manifest)
}
