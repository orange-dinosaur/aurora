use super::*;
use crate::project::Format;
use crate::script::{fountain, layout};

/// What a screenplay's Manuscript fills on the page, by document path. The
/// whole script is laid out at once, so the total is the PDF's page count. The
/// lines left empty at the page feet are shared out among the scenes by
/// length, all but the last page's, so the scenes and acts add up to within a
/// page of that total.
pub(crate) struct Pages {
	by_path: HashMap<String, f64>,
	total: usize,
}

impl Pages {
	/// Nothing unless the project is a screenplay, which counts in words.
	pub(crate) fn of(root: &Path, manifest: &Manifest) -> Option<Self> {
		if manifest.format != Format::Screenplay {
			return None;
		}
		let root = canonical(root);
		let mut paths = Vec::new();
		if let Some(children) = manifest.nodes.iter().find_map(|node| match node {
			Node::Folder { name, children, .. } if name == MANUSCRIPT => Some(children),
			_ => None,
		}) {
			collect(children, &format!("{MANUSCRIPT}/"), &mut paths);
		}

		// A file that cannot be read fills no lines, as it holds no words.
		let scenes: Vec<_> = paths
			.iter()
			.map(|path| {
				let text = within(&root, path)
					.ok()
					.and_then(|path| fs::read(path).ok())
					.and_then(|bytes| String::from_utf8(bytes).ok())
					.unwrap_or_default();
				fountain::parse(body(&text))
			})
			.collect();

		let laid = layout::layout(&scenes);
		let total = laid.pages.len();
		let lines: usize = laid.shares.iter().sum();
		// The last page ends where the writing does, so only the pages before
		// it count whole. A short scene on its own then reads as short.
		let last = laid.pages.last().map_or(0, Vec::len);
		let filled = total.saturating_sub(1) as f64 + last as f64 / layout::LINES_PER_PAGE as f64;
		let scale = if lines == 0 {
			0.0
		} else {
			filled / lines as f64
		};
		Some(Self {
			by_path: paths
				.into_iter()
				.zip(laid.shares)
				.map(|(path, share)| (path, share as f64 * scale))
				.collect(),
			total,
		})
	}

	/// The script's page count.
	pub(crate) fn total(&self) -> usize {
		self.total
	}

	/// A document's pages, or nothing outside the Manuscript.
	pub(crate) fn document(&self, path: &str) -> Option<f64> {
		self.by_path.get(path).copied()
	}

	/// The pages under a folder, or nothing outside the Manuscript.
	pub(crate) fn folder(&self, path: &str) -> Option<f64> {
		if path == MANUSCRIPT {
			return Some(self.total as f64);
		}
		if !path.starts_with(&format!("{MANUSCRIPT}/")) {
			return None;
		}
		let prefix = format!("{path}/");
		Some(
			self.by_path
				.iter()
				.filter(|(path, _)| path.starts_with(&prefix))
				.map(|(_, pages)| pages)
				.sum(),
		)
	}
}

/// Every document under `nodes` in binder order, leaving out the front and
/// back matter the way the word counts do.
fn collect(nodes: &[Node], prefix: &str, paths: &mut Vec<String>) {
	for node in nodes {
		match node {
			Node::Folder { kind, .. } if matter(*kind) => {}
			Node::Folder { name, children, .. } => {
				collect(children, &format!("{prefix}{name}/"), paths)
			}
			Node::Document { name, .. } => paths.push(format!("{prefix}{name}")),
		}
	}
}
