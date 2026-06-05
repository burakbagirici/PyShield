#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      run_analysis_on_folder,
      run_gitnexus_status,
      run_gitnexus_analyze,
      run_gitnexus_query
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

#[tauri::command]
fn run_analysis_on_folder(
  folder_path: String,
  enable_external_tools: Option<bool>,
) -> Result<String, String> {
  ai_code_analyzer::run_analysis_on_folder_with_external_tools(
    &folder_path,
    enable_external_tools.unwrap_or(true),
  )
}

#[tauri::command]
fn run_gitnexus_status(folder_path: String) -> Result<String, String> {
  ai_code_analyzer::run_gitnexus_status_on_folder(&folder_path)
}

#[tauri::command]
fn run_gitnexus_analyze(folder_path: String) -> Result<String, String> {
  ai_code_analyzer::run_gitnexus_analyze_on_folder(&folder_path)
}

#[tauri::command]
fn run_gitnexus_query(folder_path: String, query: String) -> Result<String, String> {
  ai_code_analyzer::run_gitnexus_query_on_folder(&folder_path, &query)
}
