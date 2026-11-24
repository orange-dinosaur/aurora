//! Story order: the scenes as they happen rather than as the binder tells
//! them, and the markers the writer named stretches of it with.

use super::*;
use crate::project::Marker;
use crate::tree::Place;

/// What the timeline shows: the scenes in story order, each with its card,
/// and the markers whose scene is among them.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct Timeline {
	pub scenes: Vec<DocumentSummary>,
	pub markers: Vec<Marker>,
}

/// The scenes in story order, given in binder order with where each was
/// dragged. Undragged scenes keep the binder's order, and each dragged one
/// follows its anchor, so a chain of drags stays together. A scene whose
/// anchor is gone, or that is caught in a loop of anchors, keeps its binder
/// place.
pub fn story_order(scenes: &[(Uuid, Option<Place>)]) -> Vec<Uuid> {
	let known: HashSet<Uuid> = scenes.iter().map(|(id, _)| *id).collect();
	let mut placed: HashMap<Uuid, Place> = scenes
		.iter()
		.filter_map(|&(id, place)| place.map(|place| (id, place)))
		.filter(|(_, place)| match place {
			Place::First => true,
			Place::After(anchor) => known.contains(anchor),
		})
		.collect();

	// Following anchors from a scene in a loop comes back to it within as
	// many steps as there are scenes.
	let looped: Vec<Uuid> = placed
		.keys()
		.copied()
		.filter(|&start| {
			let mut at = start;
			for _ in 0..scenes.len() {
				match placed.get(&at) {
					Some(Place::After(anchor)) if *anchor == start => return true,
					Some(Place::After(anchor)) => at = *anchor,
					_ => return false,
				}
			}
			false
		})
		.collect();
	for id in looped {
		placed.remove(&id);
	}

	let mut following: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
	for (id, _) in scenes {
		if let Some(Place::After(anchor)) = placed.get(id) {
			following.entry(*anchor).or_default().push(*id);
		}
	}
	fn put(id: Uuid, following: &HashMap<Uuid, Vec<Uuid>>, order: &mut Vec<Uuid>) {
		order.push(id);
		for next in following.get(&id).into_iter().flatten() {
			put(*next, following, order);
		}
	}

	let mut order = Vec::with_capacity(scenes.len());
	let first = scenes
		.iter()
		.filter(|(id, _)| placed.get(id) == Some(&Place::First));
	let unplaced = scenes.iter().filter(|(id, _)| !placed.contains_key(id));
	for (id, _) in first.chain(unplaced) {
		put(*id, &following, &mut order);
	}
	order
}

/// Every document under `nodes` but the front and back matter, in binder
/// order, with where it was dragged.
fn scenes(nodes: &[Node], prefix: &str, found: &mut Vec<(Document, Option<Place>)>) {
	for node in nodes {
		match node {
			Node::Folder { kind, .. } if matter(*kind) => {}
			Node::Folder { name, children, .. } => {
				scenes(children, &format!("{prefix}{name}/"), found)
			}
			Node::Document {
				id,
				name,
				target,
				placed,
				..
			} => found.push((
				Document {
					id: *id,
					path: format!("{prefix}{name}"),
					target: *target,
				},
				*placed,
			)),
		}
	}
}

/// The scenes under `folder`, the Manuscript or one story of a collection, in
/// story order.
#[tauri::command(async)]
pub fn timeline(root: PathBuf, folder: Uuid) -> Result<Timeline> {
	absolute(&root)?;
	let manifest = read_manifest(&root)?;
	let Some(Node::Folder { children, .. }) = tree::find(&manifest.nodes, folder) else {
		return Err(Error::UnknownFolder);
	};
	let prefix = tree::path(&manifest.nodes, folder).unwrap_or_default();
	let mut found = Vec::new();
	scenes(children, &format!("{prefix}/"), &mut found);

	let places: Vec<(Uuid, Option<Place>)> = found
		.iter()
		.map(|(document, place)| (document.id, *place))
		.collect();
	let by_id: HashMap<Uuid, Document> = found
		.into_iter()
		.map(|(document, _)| (document.id, document))
		.collect();
	let counting_root = canonical(&root);
	let pages = Pages::of(&root, &manifest);

	Ok(Timeline {
		scenes: story_order(&places)
			.into_iter()
			.map(|id| card(&counting_root, pages.as_ref(), &by_id[&id]))
			.collect(),
		// A marker whose scene has gone stays on disk, in case the scene
		// comes back with an undo or a merge.
		markers: manifest
			.markers
			.into_iter()
			.filter(|marker| by_id.contains_key(&marker.before))
			.collect(),
	})
}

/// Puts a scene where the writer dropped it, or back in binder order with
/// `None`.
#[tauri::command]
pub fn place_scene(root: PathBuf, id: Uuid, place: Option<Place>) -> Result<()> {
	let mut manifest = editable(&root)?;
	match tree::find_mut(&mut manifest.nodes, id) {
		Some(Node::Document { placed, .. }) => *placed = place,
		_ => return Err(Error::UnknownDocument),
	}
	write_manifest(&root, &mut manifest)
}

fn scene_exists(manifest: &Manifest, id: Uuid) -> Result<()> {
	match tree::find(&manifest.nodes, id) {
		Some(Node::Document { .. }) => Ok(()),
		_ => Err(Error::UnknownDocument),
	}
}

fn marker(manifest: &mut Manifest, id: Uuid) -> Result<&mut Marker> {
	manifest
		.markers
		.iter_mut()
		.find(|marker| marker.id == id)
		.ok_or(Error::UnknownMarker)
}

/// A new marker just before the scene `before`.
#[tauri::command]
pub fn add_marker(root: PathBuf, name: String, before: Uuid) -> Result<Marker> {
	let mut manifest = editable(&root)?;
	scene_exists(&manifest, before)?;
	let added = Marker {
		id: Uuid::new_v4(),
		name: name.trim().to_owned(),
		before,
	};
	manifest.markers.push(added.clone());
	write_manifest(&root, &mut manifest)?;
	Ok(added)
}

#[tauri::command]
pub fn rename_marker(root: PathBuf, id: Uuid, name: String) -> Result<()> {
	let mut manifest = editable(&root)?;
	marker(&mut manifest, id)?.name = name.trim().to_owned();
	write_manifest(&root, &mut manifest)
}

/// Moves a marker to sit just before another scene.
#[tauri::command]
pub fn move_marker(root: PathBuf, id: Uuid, before: Uuid) -> Result<()> {
	let mut manifest = editable(&root)?;
	scene_exists(&manifest, before)?;
	marker(&mut manifest, id)?.before = before;
	write_manifest(&root, &mut manifest)
}

#[tauri::command]
pub fn delete_marker(root: PathBuf, id: Uuid) -> Result<()> {
	let mut manifest = editable(&root)?;
	marker(&mut manifest, id)?;
	manifest.markers.retain(|marker| marker.id != id);
	write_manifest(&root, &mut manifest)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn ids(count: usize) -> Vec<Uuid> {
		(0..count).map(|_| Uuid::new_v4()).collect()
	}

	/// The order as letters, scene 0 being `a`, so a failure reads at a glance.
	fn lettered(ids: &[Uuid], places: &[Option<Place>]) -> String {
		let scenes: Vec<_> = ids.iter().copied().zip(places.iter().copied()).collect();
		story_order(&scenes)
			.iter()
			.map(|id| (b'a' + ids.iter().position(|known| known == id).unwrap() as u8) as char)
			.collect()
	}

	#[test]
	fn with_no_drags_story_order_is_binder_order() {
		let ids = ids(4);
		assert_eq!(lettered(&ids, &[None; 4]), "abcd");
	}

	#[test]
	fn a_flashback_goes_first_and_a_scene_moves_after_another() {
		let ids = ids(4);
		let places = [None, None, Some(Place::After(ids[0])), Some(Place::First)];
		assert_eq!(lettered(&ids, &places), "dacb");
	}

	#[test]
	fn a_chain_of_drags_stays_together() {
		let ids = ids(5);
		// e follows d, and d follows a, so both come straight after a.
		let places = [
			None,
			None,
			None,
			Some(Place::After(ids[0])),
			Some(Place::After(ids[3])),
		];
		assert_eq!(lettered(&ids, &places), "adebc");
	}

	#[test]
	fn a_scene_whose_anchor_is_gone_keeps_its_binder_place() {
		let ids = ids(3);
		let places = [None, Some(Place::After(Uuid::new_v4())), None];
		assert_eq!(lettered(&ids, &places), "abc");
	}

	#[test]
	fn a_loop_of_anchors_falls_back_and_what_hangs_off_it_follows() {
		let ids = ids(4);
		// b and c each follow the other; d follows c.
		let places = [
			None,
			Some(Place::After(ids[2])),
			Some(Place::After(ids[1])),
			Some(Place::After(ids[2])),
		];
		assert_eq!(lettered(&ids, &places), "abcd");
		let places = [
			None,
			Some(Place::After(ids[2])),
			Some(Place::After(ids[1])),
			Some(Place::First),
		];
		assert_eq!(lettered(&ids, &places), "dabc");
	}

	#[test]
	fn a_place_reads_and_writes_as_agreed() {
		let id = Uuid::nil();
		assert_eq!(serde_json::to_string(&Place::First).unwrap(), "\"first\"");
		assert_eq!(
			serde_json::to_string(&Place::After(id)).unwrap(),
			format!("{{\"after\":\"{id}\"}}")
		);
	}

	fn project(parent: &Path) -> (PathBuf, Uuid, Vec<Uuid>) {
		let root = crate::project::create(
			parent,
			"Ithaca",
			crate::project::Format::Novel,
			OffsetDateTime::now_utc(),
		)
		.unwrap();
		let manuscript = read_manifest(&root)
			.unwrap()
			.nodes
			.iter()
			.find_map(|node| match node {
				Node::Folder { id, name, .. } if name == MANUSCRIPT => Some(*id),
				_ => None,
			})
			.unwrap();
		let added = ["One", "Two", "Three"]
			.map(|name| {
				create_document(root.clone(), manuscript, name.to_owned())
					.unwrap()
					.id
			})
			.to_vec();
		(root, manuscript, added)
	}

	/// The order of the scenes the test made, leaving out any a new project
	/// starts with.
	fn titles(root: &Path, folder: Uuid) -> Vec<String> {
		timeline(root.to_path_buf(), folder)
			.unwrap()
			.scenes
			.into_iter()
			.map(|scene| scene.document.title)
			.filter(|title| ["One", "Two", "Three"].contains(&title.as_str()))
			.collect()
	}

	#[test]
	fn placing_a_scene_moves_it_on_the_timeline_and_none_puts_it_back() {
		let parent = tempfile::tempdir().unwrap();
		let (root, manuscript, scenes) = project(parent.path());

		place_scene(root.clone(), scenes[2], Some(Place::First)).unwrap();
		assert_eq!(titles(&root, manuscript), ["Three", "One", "Two"]);
		assert!(
			fs::read_to_string(root.join(crate::project::MANIFEST_FILE))
				.unwrap()
				.contains("\"placed\": \"first\"")
		);

		place_scene(root.clone(), scenes[2], None).unwrap();
		assert_eq!(titles(&root, manuscript), ["One", "Two", "Three"]);
		assert!(
			!fs::read_to_string(root.join(crate::project::MANIFEST_FILE))
				.unwrap()
				.contains("placed")
		);
	}

	#[test]
	fn markers_are_added_renamed_moved_and_deleted() {
		let parent = tempfile::tempdir().unwrap();
		let (root, manuscript, scenes) = project(parent.path());

		let marker = add_marker(root.clone(), " Three years later ".to_owned(), scenes[1]).unwrap();
		assert_eq!(marker.name, "Three years later");
		rename_marker(root.clone(), marker.id, "Later".to_owned()).unwrap();
		move_marker(root.clone(), marker.id, scenes[2]).unwrap();
		assert_eq!(
			timeline(root.clone(), manuscript).unwrap().markers,
			[Marker {
				id: marker.id,
				name: "Later".to_owned(),
				before: scenes[2],
			}]
		);

		delete_marker(root.clone(), marker.id).unwrap();
		assert!(
			timeline(root.clone(), manuscript)
				.unwrap()
				.markers
				.is_empty()
		);
		assert!(matches!(
			delete_marker(root, marker.id),
			Err(Error::UnknownMarker)
		));
	}

	#[test]
	fn a_marker_whose_scene_is_gone_leaves_the_view_but_not_the_disk() {
		let parent = tempfile::tempdir().unwrap();
		let (root, manuscript, scenes) = project(parent.path());
		add_marker(root.clone(), "Later".to_owned(), scenes[1]).unwrap();

		let mut manifest = read_manifest(&root).unwrap();
		tree::remove(&mut manifest.nodes, scenes[1]);
		write_manifest(&root, &mut manifest).unwrap();

		assert!(
			timeline(root.clone(), manuscript)
				.unwrap()
				.markers
				.is_empty()
		);
		assert_eq!(read_manifest(&root).unwrap().markers.len(), 1);
	}
}
