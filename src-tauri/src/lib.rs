pub mod document;
pub mod project;
pub mod store;

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
			document::refresh_documents,
			document::read_document,
			document::write_document,
			document::restore_document,
			document::section_overview,
			document::create_document
		])
		.run(tauri::generate_context!())
		.expect("error while running tauri application");
}
