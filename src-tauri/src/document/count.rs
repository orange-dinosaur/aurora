use super::*;

/// What a tree read needs out of one document: how many words it holds and
/// whether the writer has left it in the book. Both come out of the same read,
/// so they are worked out and cached together.
#[derive(Clone, Copy)]
pub(super) struct Tally {
	words: usize,
	in_book: bool,
}

/// What one file looked like when it was last read.
#[derive(Clone, Copy)]
pub(super) struct Counted {
	modified: SystemTime,
	len: u64,
	tally: Tally,
}

/// Every file this process has counted, by canonical path, held until it
/// quits. A tree read stats every document and opens only the ones that moved.
///
/// A command without `async` runs on the main thread, which is what keeps
/// manifest writes one at a time. The commands that only read run on Tauri's
/// thread pool and share this map, so it needs a lock. A `Mutex`
/// rather than an `RwLock`: a hit reads and a miss writes, each holds the map
/// for about as long as a hash lookup, and readers do not queue long enough
/// for the second lock to earn its keep. A panic while counting must not take
/// word counts away for the rest of the session, so a poisoned lock is taken
/// as it stands.
pub(super) static COUNTS: LazyLock<Mutex<HashMap<PathBuf, Counted>>> =
	LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) fn counted(path: &Path) -> Option<Counted> {
	COUNTS
		.lock()
		.unwrap_or_else(|poisoned| poisoned.into_inner())
		.get(path)
		.copied()
}

pub(super) fn remember(path: &Path, counted: Counted) {
	COUNTS
		.lock()
		.unwrap_or_else(|poisoned| poisoned.into_inner())
		.insert(path.to_path_buf(), counted);
}

/// The words in one document, from its path under a canonical root.
pub(super) fn words_in(root: &Path, relative: &str) -> usize {
	tally_of(root, relative).words
}

/// What one document amounts to, from its path under a canonical root. A file
/// that cannot be read counts as nothing rather than failing the whole tree,
/// and so does one the manifest points outside the project: `aurora.json` is a
/// file a writer can edit, so a name in it is not to be trusted. Either way it
/// stays in the book, because a file nobody can read is not a file anyone has
/// switched off.
pub(super) fn tally_of(root: &Path, relative: &str) -> Tally {
	let nothing = Tally {
		words: 0,
		in_book: true,
	};

	let Ok(path) = root.join(relative).canonicalize() else {
		return nothing;
	};
	if !path.starts_with(root) {
		return nothing;
	}

	// A stat costs a fraction of opening a file and splitting it, and a file
	// whose time and size have both stayed put has not been written since it
	// was counted. No timestamp at all means no caching: counting again is
	// only slow, while trusting a stamp that cannot go stale would be wrong.
	let stamp = fs::metadata(&path)
		.ok()
		.and_then(|data| data.modified().ok().map(|when| (when, data.len())));
	let held = stamp.and_then(|(modified, len)| {
		counted(&path).filter(|held| held.modified == modified && held.len == len)
	});

	if let Some(held) = held {
		return held.tally;
	}

	let Ok(bytes) = fs::read(&path) else {
		return nothing;
	};
	let text = String::from_utf8(bytes).unwrap_or_default();
	let tally = Tally {
		words: words_of(body(&text)),
		in_book: in_book(&text),
	};

	if let Some((modified, len)) = stamp {
		remember(
			&path,
			Counted {
				modified,
				len,
				tally,
			},
		);
	}

	tally
}

/// The words under `nodes`, however deep they sit, leaving out the Manuscript's
/// front and back matter. For a card that has to say what a folder amounts to
/// without building the views of everything in it.
///
/// The matter folders drop out here, where a parent adds its children up, and
/// nowhere else. Asked about one of them directly the walk starts inside it, so
/// it still says what it holds.
pub(super) fn words_under(root: &Path, nodes: &[Node], prefix: &str) -> usize {
	nodes
		.iter()
		.map(|node| match node {
			Node::Folder { kind, .. } if matter(*kind) => 0,
			Node::Folder { name, children, .. } => {
				words_under(root, children, &format!("{prefix}{name}/"))
			}
			Node::Document { name, .. } => words_in(root, &format!("{prefix}{name}")),
		})
		.sum()
}

/// The book's words: the Manuscript's, less its front and back matter, the same
/// total its heading in the binder shows.
pub(crate) fn manuscript_words(root: &Path, nodes: &[Node]) -> usize {
	let Ok(root) = root.canonicalize() else {
		return 0;
	};
	nodes
		.iter()
		.find_map(|node| match node {
			Node::Folder { name, children, .. } if name == MANUSCRIPT => {
				Some(words_under(&root, children, &format!("{MANUSCRIPT}/")))
			}
			_ => None,
		})
		.unwrap_or(0)
}

/// The tree under `prefix`, ready to send, and the words in the whole of it.
/// The prefix is how far down the walk has come, which is what turns a node's
/// name into its path. The total comes back alongside the views because a
/// folder's count is its children's added up, and adding them up on the way
/// out is what keeps each file to a single read.
pub(super) fn views(root: &Path, nodes: &[Node], prefix: &str) -> (Vec<NodeView>, usize) {
	let mut built = Vec::with_capacity(nodes.len());
	let mut total = 0;

	for node in nodes {
		match node {
			// The tree is about shape and counts, and the one thing the
			// sidebar draws differently. The rest of what the writer said
			// about a folder is asked for on its own, by the panel that shows
			// it.
			Node::Folder {
				id,
				name,
				kind,
				children,
				fields,
				target,
				..
			} => {
				let (children, words) = views(root, children, &format!("{prefix}{name}/"));
				// The front and back matter keep the count they have earned,
				// and the Manuscript above them does not take it: the total the
				// sidebar shows is the story.
				if !matter(*kind) {
					total += words;
				}
				built.push(NodeView::Folder {
					id: *id,
					name: name.clone(),
					kind: *kind,
					children,
					words,
					target: *target,
					in_book: folder_in_book(fields),
				});
			}
			Node::Document {
				id, name, target, ..
			} => {
				let document = Document {
					id: *id,
					path: format!("{prefix}{name}"),
					target: *target,
				};
				let tally = tally_of(root, &document.path);
				total += tally.words;
				built.push(NodeView::Document {
					document: (&document).into(),
					words: tally.words,
					in_book: tally.in_book,
				});
			}
		}
	}

	(built, total)
}
