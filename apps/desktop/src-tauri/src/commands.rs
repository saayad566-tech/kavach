//! Tauri command wrappers.
//!
//! Every function is a one-line delegation to `kavach_host::commands`, which is the
//! implementation compiled and tested by `cargo test --offline`. Nothing is re-implemented
//! here, so the tested code and the shipped code are the same code.

use kavach_host::commands as host;

/// Absolute path of the workspace directory.
///
/// Lives in the shell rather than the host crate because only the Tauri app knows where
/// its app-data directory is. The config checker allows exactly this one exception.
#[tauri::command]
pub fn workspace_root(app: tauri::AppHandle) -> String {
    use tauri::Manager;
    match app.path().app_data_dir() {
        Ok(dir) => {
            let _ = std::fs::create_dir_all(&dir);
            host::json_ok_path(&dir.to_string_lossy())
        }
        Err(e) => host::json_err(&format!("cannot resolve app data dir: {e}"), "io"),
    }
}

#[tauri::command]
pub fn probe_hardware(workdir: String) -> String {
    host::probe_hardware(&workdir)
}

#[tauri::command]
pub fn disk_usage(path: String) -> String {
    host::disk_usage(&path)
}

/// Run a build or test command. The allow-list, the timeout clamp and the "no shell" rule
/// are all enforced inside the host crate.
#[tauri::command]
pub fn run_command(cwd: String, command: String, args: Vec<String>, timeout_ms: u64) -> String {
    host::run_command(&cwd, &command, &args, timeout_ms)
}

#[tauri::command]
pub fn write_project_file(root: String, rel_path: String, content: String) -> String {
    host::write_file(&root, &rel_path, &content)
}

#[tauri::command]
pub fn read_project_file(root: String, rel_path: String) -> String {
    host::read_file(&root, &rel_path)
}

#[tauri::command]
pub fn list_files(root: String) -> String {
    host::list_files(&root)
}

#[tauri::command]
pub fn delete_project_file(root: String, rel_path: String) -> String {
    host::delete_file(&root, &rel_path)
}

#[tauri::command]
pub fn hash_file(path: String) -> String {
    host::hash(&path)
}

#[tauri::command]
pub fn verify_model(path: String, expected_sha256: String) -> String {
    host::verify_model(&path, &expected_sha256)
}

#[tauri::command]
pub fn install_model(
    model_id: String,
    source_file: String,
    destination_dir: String,
    expected_sha256: String,
    licence_accepted: bool,
    licence_name: String,
) -> String {
    host::install_model(&model_id, &source_file, &destination_dir, &expected_sha256, licence_accepted, &licence_name)
}

#[tauri::command]
pub fn extract_requirements(prompt: String) -> String {
    host::extract_requirements(&prompt)
}

#[tauri::command]
pub fn create_project(workspace: String, prompt: String) -> String {
    host::create_project(&workspace, &prompt)
}

#[tauri::command]
pub fn build_project(workspace: String, project_id: String, prompt: String) -> String {
    host::build_project(&workspace, &project_id, &prompt)
}

#[tauri::command]
pub fn list_projects(workspace: String) -> String {
    host::list_projects(&workspace)
}

#[tauri::command]
pub fn list_checkpoints(workspace: String, project_id: String) -> String {
    host::list_checkpoints(&workspace, &project_id)
}

#[tauri::command]
pub fn restore_checkpoint(workspace: String, project_id: String, checkpoint_id: String) -> String {
    host::restore_checkpoint(&workspace, &project_id, &checkpoint_id)
}

pub fn command_names() -> Vec<&'static str> {
    kavach_host::COMMAND_NAMES.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> String {
        std::env::temp_dir().to_string_lossy().to_string()
    }

    #[test]
    fn commands_delegate_to_the_tested_host() {
        let tmp = temp_dir();
        assert!(probe_hardware(tmp.clone()).starts_with("{\"ok\":true"));
        assert!(run_command(tmp.clone(), "rm".into(), vec!["-rf".into(), "/".into()], 1000).contains("permission-denied"));
        let guard = format!("{tmp}/kavach-tauri-guard");
        assert!(write_project_file(guard, "../escape".into(), "x".into()).contains("path-rejected"));
        assert!(extract_requirements("hi".into()).contains("too short"));
    }

    #[test]
    fn command_list_matches_the_host() {
        assert_eq!(command_names(), kavach_host::COMMAND_NAMES.to_vec());
        assert_eq!(command_names().len(), 18);
    }
}
