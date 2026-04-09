mod llm_probe;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            llm_probe::download_model,
            llm_probe::list_models,
            llm_probe::run_probe
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
