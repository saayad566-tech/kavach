//! The command surface.
//!
//! Plain Rust functions taking and returning strings, so they compile and test without
//! Tauri. The Tauri wrappers in `apps/desktop/src-tauri/src/commands.rs` are one-line
//! delegations to these, which means the code that is tested is the code that ships.
//!
//! Every command returns `{"ok":true,...}` or `{"ok":false,"error":"...","kind":"..."}`.

use std::path::Path;

use crate::builder;
use crate::exec::{run, ExecOptions};
use crate::install::{install_from_file, verify_installed, InstallRequest};
use crate::json::{err, ok, Json};
use crate::path::{delete_project_file, list_project_files, safe_join, write_project_file};
use crate::probe;
use crate::sha256::hash_file;
use crate::store::Store;

/// Commands a generated project may run. A model-authored `rm`, `curl` or `sh` is refused
/// before it is spawned.
pub const ALLOWED_COMMANDS: &[&str] = &[
    "node", "npm", "npx", "pnpm", "yarn", "python3", "python", "pytest", "cargo", "git", "tsc", "vite", "go", "make",
];

pub const MAX_TIMEOUT_MS: u64 = 600_000;

/// Envelope helper for shell-only commands that must return the same JSON shape.
pub fn json_ok_path(path: &str) -> String {
    ok(Json::object().str("path", path))
}

/// Envelope helper for errors raised outside this crate.
pub fn json_err(message: &str, kind: &str) -> String {
    err(message, kind)
}

fn gpus_json(probe: &probe::Probe) -> String {
    let items: Vec<String> = probe
        .gpus
        .iter()
        .map(|g| {
            Json::object()
                .str("vendor", &g.vendor)
                .str("name", &g.name)
                .raw("vramBytes", &g.vram_bytes.map(|v| v.to_string()).unwrap_or_else(|| "null".into()))
                .strings("acceleration", &g.acceleration)
                .bool("unifiedMemory", g.unified_memory)
                .finish()
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// `probe_hardware(workdir)`.
pub fn probe_hardware(workdir: &str) -> String {
    let p = probe::probe(workdir);
    let sources: Vec<String> = p
        .sources
        .iter()
        .map(|(k, v)| format!("\"{}\":\"{}\"", crate::json::escape(k), crate::json::escape(v)))
        .collect();
    ok(Json::object()
        .str("platform", &p.platform)
        .str("arch", &p.arch)
        .str("cpuModel", &p.cpu_model)
        .num("logicalCores", p.logical_cores as i64)
        .strings("instructionSets", &p.instruction_sets)
        .uint("ramTotalBytes", p.ram_total_bytes)
        .uint("ramAvailableBytes", p.ram_available_bytes)
        .raw("gpus", &gpus_json(&p))
        .uint("diskTotalBytes", p.disk.total_bytes)
        .uint("diskAvailableBytes", p.disk.available_bytes)
        .str("probedAt", &p.probed_at)
        .strings("unmeasured", &p.unmeasured)
        .raw("sources", &format!("{{{}}}", sources.join(","))))
}

/// `disk_usage(path)`.
pub fn disk_usage(path: &str) -> String {
    match probe::disk_usage(path) {
        Some(u) => ok(Json::object().uint("totalBytes", u.total_bytes).uint("availableBytes", u.available_bytes)),
        None => err(&format!("cannot read disk usage for {path}"), "io"),
    }
}

/// `run_command(cwd, command, timeout_ms, args...)`, with the permission policy applied.
pub fn run_command(cwd: &str, command: &str, args: &[String], timeout_ms: u64) -> String {
    let base = Path::new(command)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| command.to_string());

    if !ALLOWED_COMMANDS.contains(&base.as_str()) {
        return err(
            &format!(
                "command '{base}' is not in the allowed command list; refusing to execute it. Allowed: {}",
                ALLOWED_COMMANDS.join(", ")
            ),
            "permission-denied",
        );
    }
    if !Path::new(cwd).is_dir() {
        return err(&format!("working directory does not exist: {cwd}"), "invalid");
    }

    let timeout = timeout_ms.clamp(1_000, MAX_TIMEOUT_MS);
    match run(command, args, ExecOptions { cwd: Some(cwd.to_string()), timeout_ms: timeout, max_output_bytes: 256 * 1024 }) {
        Ok(outcome) => ok(Json::object()
            .str("command", &outcome.command)
            .num("exitCode", outcome.exit_code as i64)
            .str("stdout", &outcome.stdout)
            .str("stderr", &outcome.stderr)
            .uint("durationMs", outcome.duration_ms)
            .bool("timedOut", outcome.timed_out)
            .bool("truncated", outcome.truncated)),
        Err(e) => err(&e.to_string(), "spawn-failed"),
    }
}

/// `write_project_file(root, rel_path, content)`.
pub fn write_file(root: &str, rel_path: &str, content: &str) -> String {
    match write_project_file(Path::new(root), rel_path, content) {
        Ok(path) => ok(Json::object().str("path", &path.to_string_lossy())),
        Err(e) => err(&e.to_string(), "path-rejected"),
    }
}

/// `read_project_file(root, rel_path)`.
pub fn read_file(root: &str, rel_path: &str) -> String {
    match safe_join(Path::new(root), rel_path) {
        Ok(path) => match std::fs::read_to_string(&path) {
            Ok(content) => ok(Json::object().str("content", &content)),
            Err(e) => err(&format!("cannot read {rel_path}: {e}"), "io"),
        },
        Err(e) => err(&e.to_string(), "path-rejected"),
    }
}

/// `list_files(root)`.
pub fn list_files(root: &str) -> String {
    ok(Json::object().strings("files", &list_project_files(Path::new(root))))
}

/// `delete_project_file(root, rel_path)`.
pub fn delete_file(root: &str, rel_path: &str) -> String {
    match delete_project_file(Path::new(root), rel_path) {
        Ok(_) => ok(Json::object().str("path", rel_path)),
        Err(e) => err(&e.to_string(), "path-rejected"),
    }
}

/// `hash_file(path)`.
pub fn hash(path: &str) -> String {
    match hash_file(Path::new(path)) {
        Ok(digest) => ok(Json::object().str("sha256", &digest)),
        Err(e) => err(&e.to_string(), "io"),
    }
}

/// `verify_model(path, expected_sha256)`.
pub fn verify_model(path: &str, expected_sha256: &str) -> String {
    match verify_installed(path, expected_sha256) {
        Ok(matches) => ok(Json::object().bool("matches", matches)),
        Err(e) => err(&e.to_string(), "io"),
    }
}

/// `install_model(...)`.
pub fn install_model(
    model_id: &str,
    source_file: &str,
    destination_dir: &str,
    expected_sha256: &str,
    licence_accepted: bool,
    licence_name: &str,
) -> String {
    match install_from_file(InstallRequest {
        model_id,
        source_file,
        destination_dir,
        expected_sha256,
        licence_accepted,
        licence_name,
    }) {
        Ok(result) => ok(Json::object()
            .str("modelId", &result.model_id)
            .str("path", &result.path)
            .uint("sizeBytes", result.size_bytes)
            .str("sha256", &result.sha256)
            .str("licenceName", &result.licence_name)),
        Err(e) => {
            let kind = match &e {
                crate::install::InstallError::LicenceNotAccepted(_) => "licence-not-accepted",
                crate::install::InstallError::HashMismatch { .. } => "hash-mismatch",
                crate::install::InstallError::Io(_) => "io",
            };
            err(&e.to_string(), kind)
        }
    }
}

/// `extract_requirements(prompt)` — what the builder understood, before anything is built.
pub fn extract_requirements(prompt: &str) -> String {
    if prompt.trim().len() < 8 {
        return err("the request is too short to extract requirements from", "invalid");
    }
    let reqs = builder::extract_requirements(prompt);
    let features: Vec<String> = reqs
        .features
        .iter()
        .map(|f| {
            Json::object()
                .str("id", &f.id)
                .str("statement", &f.statement)
                .str("acceptance", &f.acceptance)
                .finish()
        })
        .collect();
    let plan: Vec<String> = builder::build_plan(&reqs)
        .iter()
        .map(|t| {
            Json::object()
                .str("id", &t.id)
                .str("title", &t.title)
                .strings("acceptance", &t.acceptance)
                .strings("dependsOn", &t.depends_on)
                .finish()
        })
        .collect();
    ok(Json::object()
        .str("appKind", &reqs.app_kind)
        .str("summary", &reqs.summary)
        .strings("entities", &reqs.entities)
        .str("dataSensitivity", &reqs.data_sensitivity)
        .bool("offlineRequired", reqs.offline_required)
        .strings("assumptions", &reqs.assumptions)
        .raw("features", &format!("[{}]", features.join(",")))
        .raw("plan", &format!("[{}]", plan.join(","))))
}

/// `build_project(workspace, project_id, prompt)` — the whole pipeline.
pub fn build_project(workspace: &str, project_id: &str, prompt: &str) -> String {
    if prompt.trim().len() < 8 {
        return err("the request is too short to build from", "invalid");
    }
    let mut store = Store::load(Path::new(workspace));
    let dir = store.project_dir(project_id);

    // Snapshot before mutating, so a bad build can be undone.
    let checkpoint = store.checkpoint(project_id, "before build").map(|c| c.id).unwrap_or_default();

    let report = builder::build_project(project_id, &dir, prompt);
    let _ = store.record_outcome(project_id, report.verified.clone(), report.unverified.clone());

    let mut payload = report.to_json();
    // Splice the checkpoint id into the report object.
    payload.pop();
    payload.push_str(&format!(",\"checkpointId\":\"{}\"}}", crate::json::escape(&checkpoint)));
    ok(crate::json::Json::object().raw("report", &payload))
}

/// `create_project(workspace, prompt)`.
pub fn create_project(workspace: &str, prompt: &str) -> String {
    if prompt.trim().len() < 8 {
        return err("the request is too short to create a project from", "invalid");
    }
    let reqs = builder::extract_requirements(prompt);
    let mut store = Store::load(Path::new(workspace));
    let name: String = reqs.summary.chars().take(60).collect();
    let project = store.create_project(prompt, &reqs.app_kind, name.trim());
    if let Err(e) = store.save() {
        return err(&e, "io");
    }
    ok(Json::object()
        .str("id", &project.id)
        .str("name", &project.name)
        .str("appKind", &project.app_kind)
        .str("createdAt", &project.created_at))
}

/// `list_projects(workspace)`.
pub fn list_projects(workspace: &str) -> String {
    let store = Store::load(Path::new(workspace));
    let items: Vec<String> = store
        .projects
        .iter()
        .map(|p| {
            Json::object()
                .str("id", &p.id)
                .str("name", &p.name)
                .str("appKind", &p.app_kind)
                .str("createdAt", &p.created_at)
                .num("verifiedTasks", p.tasks_verified.len() as i64)
                .num("unverifiedTasks", p.tasks_unverified.len() as i64)
                .finish()
        })
        .collect();
    ok(Json::object().raw("projects", &format!("[{}]", items.join(","))))
}

/// `list_checkpoints(workspace, project_id)`.
pub fn list_checkpoints(workspace: &str, project_id: &str) -> String {
    let store = Store::load(Path::new(workspace));
    match store.find(project_id) {
        Some(project) => {
            let items: Vec<String> = project
                .checkpoints
                .iter()
                .map(|c| {
                    Json::object()
                        .str("id", &c.id)
                        .str("label", &c.label)
                        .str("treeHash", &c.tree_hash)
                        .str("createdAt", &c.created_at)
                        .finish()
                })
                .collect();
            ok(Json::object().raw("checkpoints", &format!("[{}]", items.join(","))))
        }
        None => err(&format!("unknown project {project_id}"), "not-found"),
    }
}

/// `restore_checkpoint(workspace, project_id, checkpoint_id)`.
pub fn restore_checkpoint(workspace: &str, project_id: &str, checkpoint_id: &str) -> String {
    let mut store = Store::load(Path::new(workspace));
    match store.restore(project_id, checkpoint_id) {
        Ok((restored, removed)) => ok(Json::object().strings("restored", &restored).strings("removed", &removed)),
        Err(e) => err(&e, "restore-failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kavach-cmd-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn probe_returns_real_json() {
        let out = probe_hardware("/tmp");
        assert!(out.starts_with("{\"ok\":true"), "{out}");
        assert!(out.contains("\"unmeasured\":["));
    }

    #[test]
    fn run_command_respects_the_allow_list() {
        let dir = tmp("allow");
        let allowed = run_command(&dir.to_string_lossy(), "node", &["-e".into(), "console.log(7*6)".into()], 30_000);
        assert!(allowed.contains("42"), "{allowed}");
        let denied = run_command(&dir.to_string_lossy(), "rm", &["-rf".into(), "/".into()], 5_000);
        assert!(denied.contains("permission-denied"), "{denied}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_command_clamps_timeout_and_validates_cwd() {
        let dir = tmp("timeout");
        let out = run_command(&dir.to_string_lossy(), "node", &["-e".into(), "setTimeout(()=>{},60000)".into()], 1_000);
        assert!(out.contains("\"timedOut\":true"), "{out}");
        let bad = run_command("/nonexistent-xyz", "node", &[], 1_000);
        assert!(bad.contains("\"kind\":\"invalid\""), "{bad}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_commands_are_path_guarded() {
        let dir = tmp("files");
        let root = dir.to_string_lossy().to_string();
        assert!(write_file(&root, "src/a.js", "ok").contains("\"ok\":true"));
        assert!(read_file(&root, "src/a.js").contains("ok"));
        assert!(list_files(&root).contains("src/a.js"));
        assert!(write_file(&root, "../escape", "no").contains("path-rejected"));
        assert!(delete_file(&root, "src/a.js").contains("\"ok\":true"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn model_commands_enforce_the_licence_gate() {
        let dir = tmp("model");
        let src = dir.join("w.bin");
        std::fs::write(&src, b"weights").unwrap();
        let hash = crate::sha256::hash_bytes(b"weights");
        let denied = install_model("m", &src.to_string_lossy(), &dir.join("models").to_string_lossy(), &hash, false, "Gemma Terms of Use");
        assert!(denied.contains("licence-not-accepted"), "{denied}");
        let allowed = install_model("m", &src.to_string_lossy(), &dir.join("models").to_string_lossy(), &hash, true, "Apache-2.0");
        assert!(allowed.contains("\"ok\":true"), "{allowed}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn requirements_command_reports_entities_and_plan() {
        let out = extract_requirements("Build a web app to manage invoices and clients with search");
        assert!(out.contains("\"appKind\":\"web-app\""), "{out}");
        assert!(out.contains("Invoice") || out.contains("invoice"), "{out}");
        assert!(out.contains("\"plan\":["), "{out}");
        assert!(extract_requirements("hi").contains("too short"));
    }

    #[test]
    fn end_to_end_build_through_the_command_surface() {
        let ws = tmp("e2e");
        let created = create_project(&ws.to_string_lossy(), "Build a web app to manage invoices and clients with search");
        assert!(created.contains("\"id\":\"p-"), "{created}");
        // Pull the id out without a JSON parser.
        let start = created.find("\"id\":\"").unwrap() + 6;
        let end = created[start..].find('"').unwrap() + start;
        let project_id = created[start..end].to_string();

        let built = build_project(&ws.to_string_lossy(), &project_id, "Build a web app to manage invoices and clients with search");
        assert!(built.contains("\"ok\":true"), "{built}");
        assert!(built.contains("\"stage\":\"verify\",\"ok\":true"), "{built}");
        assert!(built.contains("checkpointId"), "{built}");

        let files = list_files(&ws.join("projects").join(&project_id).to_string_lossy());
        assert!(files.contains("src/store.js"), "{files}");
        assert!(files.contains("test/store.test.js"), "{files}");

        let projects = list_projects(&ws.to_string_lossy());
        assert!(projects.contains(&project_id), "{projects}");

        let checkpoints = list_checkpoints(&ws.to_string_lossy(), &project_id);
        assert!(checkpoints.contains("before build"), "{checkpoints}");
        std::fs::remove_dir_all(&ws).ok();
    }
}
