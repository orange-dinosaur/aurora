pub mod document;
pub mod history;
pub mod project;
pub mod store;
pub mod tree;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
	tauri::Builder::default()
		.plugin(tauri_plugin_opener::init())
		.plugin(tauri_plugin_dialog::init())
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
