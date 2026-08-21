pub mod project;
pub mod store;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
	tauri::Builder::default()
		.plugin(tauri_plugin_opener::init())
		.plugin(tauri_plugin_dialog::init())
		.invoke_handler(tauri::generate_handler![
			project::create_project,
			project::last_project,
			project::forget_project,
			project::open_project,
			project::recent_projects
		])
		.run(tauri::generate_context!())
		.expect("error while running tauri application");
}
