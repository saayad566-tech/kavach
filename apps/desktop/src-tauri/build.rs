// Tauri build script.
//
// Two jobs here.
//
// 1. `tauri_build` generates the ACL schemas and embeds the configuration.
//
// 2. `AppManifest::commands` restricts which of *our* commands the webview may call.
//    Without it, Tauri's default is that every command registered with `invoke_handler` is
//    callable from every window and webview — the capabilities file constrains Tauri's own
//    plugins, not our application commands. The list below must match
//    `kavach_host::COMMAND_NAMES` exactly, plus the one shell-only command; the offline
//    checker (`scripts/check-desktop-config.py`) enforces that so the two cannot drift.
//
// If a future Tauri release changes this API, the safe fallback is the single line
//     tauri_build::build()
// which restores the default (all commands callable) without breaking anything else.

// `AppManifest::commands` takes `&'static [&'static str]` (verified against
// tauri-build 2.7.4, src/acl.rs:104), so this must be a static slice — a local array
// borrowed for the call would not compile.
static COMMANDS: &[&str] = &[
        // Shell-only: only the Tauri app knows where its app-data directory is.
    "workspace_root",
        // Everything else is a one-line delegation to kavach-host.
    "probe_hardware",
    "disk_usage",
    "run_command",
    "write_project_file",
    "read_project_file",
    "list_files",
    "delete_project_file",
    "hash_file",
    "verify_model",
    "install_model",
    "extract_requirements",
    "create_project",
    "build_project",
    "probe_runtime",
    "security_scan",
    "export_project",
    "list_projects",
    "list_checkpoints",
    "restore_checkpoint",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
