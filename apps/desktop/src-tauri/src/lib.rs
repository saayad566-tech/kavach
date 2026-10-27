//! Kavach desktop shell.
//!
//! The window renders `apps/desktop/ui` — plain HTML and JavaScript, no bundler. Every
//! privileged operation crosses into Rust through the commands in `commands.rs`, which
//! delegate to `kavach-host`.
//!
//! Security posture: the window is granted no filesystem, shell or HTTP permission; it can
//! only call the registered commands, and those apply the command allow-list, the timeout
//! clamp and the path guard.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if let Ok(dir) = app.path().app_data_dir() {
                let _ = std::fs::create_dir_all(&dir);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::workspace_root,
            commands::probe_hardware,
            commands::disk_usage,
            commands::run_command,
            commands::write_project_file,
            commands::read_project_file,
            commands::list_files,
            commands::delete_project_file,
            commands::hash_file,
            commands::verify_model,
            commands::install_model,
            commands::extract_requirements,
            commands::create_project,
            commands::build_project,
            commands::probe_runtime,
            commands::security_scan,
            commands::export_project,
            commands::create_checkpoint,
            commands::list_projects,
            commands::list_checkpoints,
            commands::restore_checkpoint,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kavach");
}
