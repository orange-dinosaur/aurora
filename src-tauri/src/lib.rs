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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
	tauri::Builder::default()
		.plugin(tauri_plugin_opener::init())
		.plugin(tauri_plugin_dialog::init())
		.setup(|app| {
			hold_minimum(app)?;
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
		.run(tauri::generate_context!())
		.expect("error while running tauri application");
}
