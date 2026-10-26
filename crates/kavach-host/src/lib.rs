//! Kavach native host.
//!
//! No dependencies, by design: builds with `cargo build --offline`, so it compiles and
//! tests in a container with no crates.io access and can be audited without reading a
//! dependency tree.
//!
//! Layout:
//!   * `probe`    — what the machine actually is
//!   * `exec`     — argv-only execution with a tree-killing timeout
//!   * `path`     — the choke point for every model-derived path
//!   * `install`  — licence gate and hash verification before anything is trusted
//!   * `builder`  — requirements → plan → scaffold → verify → delivery report
//!   * `store`    — project index on disk, no database engine to link
//!   * `commands` — the surface the Tauri shell exposes

pub mod builder;
pub mod commands;
pub mod exec;
pub mod install;
pub mod json;
pub mod path;
pub mod probe;
pub mod sha256;
pub mod store;

/// Command names, shared by the Tauri handler list so the two cannot drift.
pub const COMMAND_NAMES: &[&str] = &[
    "probe_hardware",
    "disk_usage",
    "run_command",
    "write_project_file",
    "read_project_file",
    "list_files",
    "delete_project_file",
    "verify_model",
    "hash_file",
    "install_model",
    "extract_requirements",
    "create_project",
    "build_project",
    "list_projects",
    "list_checkpoints",
    "restore_checkpoint",
];
