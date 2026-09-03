pub mod document;
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
			project::close_project,
			project::open_project,
			project::recent_projects,
			document::list_documents,
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
			document::set_document_target,
			document::delete_document,
			document::list_trash,
			document::restore_from_trash,
			document::purge_trash_entry,
			document::reorder_document,
			store::read_preferences,
			store::write_preferences
		])
		.run(tauri::generate_context!())
		.expect("error while running tauri application");
}
