pub mod archive;
pub mod book;
pub mod document;
pub mod docx;
pub mod epub;
pub mod history;
pub mod project;
pub mod store;
pub mod text;
pub mod tree;

/// Asks every screen holding unwritten text to write it before the app quits.
const FLUSH_BEFORE_EXIT: &str = "flush-before-exit";

/// How long the quit waits for those writes before going anyway.
const FLUSH_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// Tauri hands the window's minimum to GTK as a hint, and on GNOME under
/// Wayland the hint is lost. A size request on the window's contents is how
/// every GTK app gets its own minimum across, so the config's is set that way.
#[cfg(target_os = "linux")]
fn hold_minimum(app: &tauri::App) -> tauri::Result<()> {
	use gtk::prelude::WidgetExt;
	use tauri::Manager;

	let Some(config) = app.config().app.windows.first() else {
		return Ok(());
	};
	let (Some(width), Some(height)) = (config.min_width, config.min_height) else {
		return Ok(());
	};
	if let Some(window) = app.get_webview_window(&config.label) {
		window
			.default_vbox()?
			.set_size_request(width as i32, height as i32);
	}
	Ok(())
}

#[cfg(not(target_os = "linux"))]
fn hold_minimum(_app: &tauri::App) -> tauri::Result<()> {
	Ok(())
}

/// A dev build says so in its titlebar, and reads and writes its own store.
#[cfg(debug_assertions)]
fn mark_as_dev(app: &tauri::App) -> tauri::Result<()> {
	use tauri::Manager;

	let _ = project::seed_dev_store(app.handle());
	if let Some(window) = app.get_webview_window("main") {
		window.set_title("Aurora Dev")?;
	}
	Ok(())
}

#[cfg(not(debug_assertions))]
fn mark_as_dev(_app: &tauri::App) -> tauri::Result<()> {
	Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
	tauri::Builder::default()
		.plugin(tauri_plugin_opener::init())
		.plugin(tauri_plugin_dialog::init())
		.plugin(tauri_plugin_updater::Builder::new().build())
		.plugin(tauri_plugin_process::init())
		.setup(|app| {
			hold_minimum(app)?;
			mark_as_dev(app)?;
			Ok(())
		})
		.invoke_handler(tauri::generate_handler![
			project::format_layouts,
			project::create_project,
			project::last_project,
			project::forget_project,
			project::trash_project,
			project::close_project,
			project::open_project,
			project::recent_projects,
			project::read_book,
			project::write_book,
			project::set_cover,
			project::read_cover,
			project::clear_cover,
			book::book_extent,
			book::export_book,
			document::document_tree,
			document::refresh_documents,
			document::read_document,
			document::read_all_documents,
			document::write_document,
			document::restore_document,
			document::folder_overview,
			document::create_document,
			document::create_folder,
			document::rename_document,
			document::rename_folder,
			document::set_document_target,
			document::set_folder_target,
			document::folder_progress,
			document::folder_fields,
			document::set_folder_fields,
			document::delete_document,
			document::delete_folder,
			document::list_trash,
			document::restore_from_trash,
			document::purge_trash_entry,
			document::move_node,
			history::read_history,
			history::append_session,
			store::read_preferences,
			store::write_preferences,
			store::read_expanded,
			store::write_expanded,
			store::read_folded,
			store::write_folded
		])
		.build(tauri::generate_context!())
		.expect("error while building tauri application")
		.run(|handle, event| {
			// Closing the window waits for the frontend's flush, but quitting
			// (macOS ⌘Q, the Dock, logging out) does not go through the window
			// at all. Hold the quit, ask for the writes, and go when they land.
			if let tauri::RunEvent::ExitRequested { api, code, .. } = &event
				&& code.is_none()
			{
				api.prevent_exit();
				let handle = handle.clone();
				std::thread::spawn(move || {
					use tauri::Emitter;
					let _ = handle.emit(FLUSH_BEFORE_EXIT, ());
					// A frontend that never answers must not wedge the app.
					std::thread::sleep(FLUSH_GRACE);
					handle.exit(0);
				});
			}
		});
}
