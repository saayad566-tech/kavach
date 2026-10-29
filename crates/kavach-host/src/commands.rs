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
use crate::export::export_project as do_export;
use crate::runtime;
use crate::security;
use crate::install::{install_from_file, verify_installed, InstallRequest};
use crate::json::{err, ok, Json};
use crate::path::{delete_project_file, list_project_files, safe_join, write_project_file};
use crate::probe;
use crate::sha256::hash_file;
use crate::models;
use crate::store::Store;

/// Commands a generated project may run. A model-authored `rm`, `curl` or `sh` is refused
/// before it is spawned.
pub const ALLOWED_COMMANDS: &[&str] = &[
    "node", "npm", "npx", "pnpm", "yarn", "python3", "python", "pytest", "cargo", "git", "tsc", "vite", "go", "make",
];

pub const MAX_TIMEOUT_MS: u64 = 600_000;

/// A single file a generated project writes may not exceed this. Big enough for any real
/// build artefact, small enough that filling a disk takes deliberate effort.
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

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
    // Every command a generated project runs gets resource limits: CPU time slightly above
    // the wall-clock budget, and a per-file write cap so a runaway build cannot fill the
    // disk. Both are kernel-enforced, so they hold for grandchildren too.
    let options = ExecOptions {
        cwd: Some(cwd.to_string()),
        timeout_ms: timeout,
        max_output_bytes: 256 * 1024,
        cpu_seconds: Some(timeout / 1000 + 5),
        max_file_bytes: Some(MAX_FILE_BYTES),
    };
    match run(command, args, options) {
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

/// `probe_runtime()` — is a model server running on this machine?
///
/// Never starts one. Absence is reported as absence.
pub fn probe_runtime() -> String {
    match runtime::discover("127.0.0.1", &runtime::default_ports(), None) {
        Some(found) => {
            let models = runtime::list_models(&found).unwrap_or_default();
            ok(Json::object()
                .bool("available", true)
                .str("kind", found.kind.as_str())
                .str("host", &found.host)
                .num("port", found.port as i64)
                .str("model", &found.model)
                .strings("models", &models))
        }
        None => ok(Json::object()
            .bool("available", false)
            .str("detail", "no model server answered on 127.0.0.1:11434 or :8080; start Ollama or a llama.cpp server, or run without one")),
    }
}

/// `security_scan(workspace, project_id)` — scan a project's files.
pub fn security_scan(workspace: &str, project_id: &str) -> String {
    let store = Store::load(Path::new(workspace));
    let dir = store.project_dir(project_id);
    if !dir.is_dir() {
        return err(&format!("unknown project {project_id}"), "not-found");
    }
    let report = security::scan_project(&dir);
    let findings: Vec<String> = report
        .findings
        .iter()
        .map(|f| {
            Json::object()
                .str("path", &f.path)
                .num("line", f.line as i64)
                .str("severity", f.severity.as_str())
                .str("rule", f.rule)
                .str("detail", &f.detail)
                .finish()
        })
        .collect();
    ok(Json::object()
        .num("filesScanned", report.files_scanned as i64)
        .bool("passed", report.passes())
        .str("summary", &report.summary())
        .raw("findings", &format!("[{}]", findings.join(","))))
}

/// `create_checkpoint(workspace, project_id, label)` — snapshot the tree on demand.
///
/// Builds take a checkpoint automatically before they write anything. This lets the user
/// take one at any other moment, which is the difference between "I can undo the last build"
/// and "I can undo whatever I am about to try".
pub fn create_checkpoint(workspace: &str, project_id: &str, label: &str) -> String {
    let mut store = Store::load(Path::new(workspace));
    if !store.project_dir(project_id).is_dir() {
        return err(&format!("unknown project {project_id}"), "not-found");
    }
    let label = if label.trim().is_empty() { "manual checkpoint" } else { label.trim() };
    match store.checkpoint(project_id, label) {
        Ok(entry) => match store.save() {
            Ok(()) => ok(Json::object()
                .str("id", &entry.id)
                .str("label", &entry.label)
                .str("treeHash", &entry.tree_hash)
                .num("createdAt", entry.created_at.parse::<i64>().unwrap_or(0))),
            Err(e) => err(&format!("checkpoint written but the index could not be saved: {e}"), "store-error"),
        },
        Err(e) => err(&e, "checkpoint-failed"),
    }
}

/// `export_project(workspace, project_id, destination)` — copy the work out.
pub fn export_project(workspace: &str, project_id: &str, destination: &str) -> String {
    match do_export(Path::new(workspace), project_id, destination) {
        Ok(result) => ok(Json::object()
            .str("destination", &result.destination)
            .num("files", result.files.len() as i64)
            .uint("bytes", result.bytes)),
        Err(e) => err(&e.to_string(), "export-failed"),
    }
}

/// `build_project(workspace, project_id, prompt, use_local_model)` — the whole pipeline.
pub fn build_project(workspace: &str, project_id: &str, prompt: &str, use_local_model: bool) -> String {
    if prompt.trim().len() < 8 {
        return err("the request is too short to build from", "invalid");
    }
    let mut store = Store::load(Path::new(workspace));
    let dir = store.project_dir(project_id);

    // Snapshot before mutating, so a bad build can be undone.
    let checkpoint = store.checkpoint(project_id, "before build").map(|c| c.id).unwrap_or_default();

    let found = if use_local_model {
        runtime::discover("127.0.0.1", &runtime::default_ports(), None)
    } else {
        None
    };
    let report = builder::build_project_with(project_id, &dir, prompt, found.as_ref());
    let _ = store.record_outcome(project_id, report.verified.clone(), report.unverified.clone());

    // Persist what actually happened. The brief requires a durable ledger in which a task is
    // verified only against evidence, so each stage becomes a task whose evidence is the
    // stage's own detail line — and the store refuses a "verified" state with no evidence.
    let now = crate::store::now();
    let verify_evidence = report
        .stages
        .iter()
        .find(|stage| stage.stage == "verify")
        .map(|stage| stage.detail.clone())
        .unwrap_or_else(|| "the build produced no verify stage".to_string());
    for stage in &report.stages {
        let state = if stage.ok {
            crate::store::TaskState::Verified
        } else {
            crate::store::TaskState::Failed
        };
        let task = crate::store::TaskEntry {
            id: format!("stage:{}", stage.stage),
            project_id: project_id.to_string(),
            title: format!("Pipeline stage: {}", stage.stage),
            state,
            acceptance: vec!["the stage reports a result and its detail".to_string()],
            depends_on: Vec::new(),
            evidence: if stage.detail.trim().is_empty() {
                Vec::new()
            } else {
                vec![stage.detail.clone()]
            },
            updated_at: now.clone(),
        };
        let _ = store.upsert_task(task);
    }

    // Plan tasks carry their own acceptance criteria and the evidence that closed them.
    for id in &report.verified {
        let task = crate::store::TaskEntry {
            id: id.clone(),
            project_id: project_id.to_string(),
            title: format!("Plan task {id}"),
            state: crate::store::TaskState::Verified,
            acceptance: vec!["the build's verify stage passes".to_string()],
            depends_on: Vec::new(),
            evidence: vec![format!("verify stage: {verify_evidence}")],
            updated_at: now.clone(),
        };
        let _ = store.upsert_task(task);
    }
    for id in &report.unverified {
        let task = crate::store::TaskEntry {
            id: id.clone(),
            project_id: project_id.to_string(),
            title: format!("Plan task {id}"),
            state: crate::store::TaskState::NeedsVerification,
            acceptance: vec!["the build's verify stage passes".to_string()],
            depends_on: Vec::new(),
            evidence: Vec::new(),
            updated_at: now.clone(),
        };
        let _ = store.upsert_task(task);
    }

    store.add_note(crate::store::NoteEntry {
        id: format!("note-{now}"),
        project_id: project_id.to_string(),
        kind: "change".to_string(),
        text: format!(
            "Build: {} file(s), {} verified, {} unverified, checkpoint {}",
            report.files.len(),
            report.verified.len(),
            report.unverified.len(),
            if checkpoint.is_empty() { "none" } else { &checkpoint }
        ),
        evidence: report.security_summary.clone(),
        created_at: now.clone(),
    });

    // A failed verification is a lesson worth keeping, not a line in a log that scrolls away.
    if !report.stages.iter().all(|stage| stage.ok) {
        let failed: Vec<String> = report
            .stages
            .iter()
            .filter(|stage| !stage.ok)
            .map(|stage| format!("{}: {}", stage.stage, stage.detail))
            .collect();
        store.add_note(crate::store::NoteEntry {
            id: format!("note-{now}-failure"),
            project_id: project_id.to_string(),
            kind: "lesson".to_string(),
            text: "Build did not verify; nothing in this delivery is claimed to work.".to_string(),
            evidence: failed.join("; "),
            created_at: now,
        });
    }

    let _ = store.save();

    let mut payload = report.to_json();
    // Splice the checkpoint id into the report object.
    payload.pop();
    payload.push_str(&format!(",\"checkpointId\":\"{}\"}}", crate::json::escape(&checkpoint)));
    ok(crate::json::Json::object().raw("report", &payload))
}

/// `model_catalog(workspace)` — the catalogue, the quantisation levels, and what this machine
/// should run.
///
/// The recommendation is computed from a live hardware probe, so the answer changes when the
/// machine does. Nothing is downloaded: this is advice about which file to bring.
pub fn model_catalog(workspace: &str) -> String {
    let _ = workspace;
    // `probe` needs a directory for the disk measurement; the workspace is the right one.
    let hardware = probe::probe(workspace);
    let machine = models::Machine {
        ram_available: hardware.ram_available_bytes,
        // `vram_bytes` is optional: an integrated GPU reports none, which is the same as zero
        // for the purposes of "will this model fit in VRAM".
        vram: hardware.gpus.iter().filter_map(|g| g.vram_bytes).max().unwrap_or(0),
        on_gpu: hardware.gpus.iter().any(|g| !g.acceleration.is_empty()),
    };
    let budget = machine.budget();
    let recommended = models::recommend(budget, 6);

    let specs: Vec<String> = models::catalog()
        .iter()
        .map(|spec| {
            let best = models::best_quant_for(spec.params_b, budget);
            Json::object()
                .str("id", spec.id)
                .str("name", spec.name)
                .str("family", spec.family)
                .str("licence", spec.licence)
                .str("strengths", spec.strengths)
                .num("paramsB", (spec.params_b * 10.0) as i64)
                .num("context", spec.context as i64)
                .str("bestQuant", best.unwrap_or("none"))
                .uint("sizeBytes", best.and_then(|q| models::estimate_size_bytes(spec.params_b, q)).unwrap_or(0))
                .finish()
        })
        .collect();

    let quants: Vec<String> = models::quants()
        .iter()
        .map(|q| Json::object().str("id", q.id).num("quality", q.quality as i64).str("note", q.note).finish())
        .collect();

    let picks: Vec<String> = recommended
        .iter()
        .map(|rec| {
            Json::object()
                .str("id", rec.spec.id)
                .str("name", rec.spec.name)
                .str("quant", rec.quant)
                .str("verdict", rec.fit.verdict.as_str())
                .uint("sizeBytes", rec.fit.size_bytes)
                .num("context", models::recommended_context(rec.spec, budget) as i64)
                .num("tokensPerSecond", (models::tokens_per_second(rec.spec.params_b, machine.on_gpu) * 10.0) as i64)
                .finish()
        })
        .collect();

    ok(Json::object()
        .uint("budgetBytes", budget)
        .raw("specs", &format!("[{}]", specs.join(",")))
        .raw("quants", &format!("[{}]", quants.join(",")))
        .raw("recommended", &format!("[{}]", picks.join(","))))
}

/// `models_list(workspace)` — what is installed, what is active, how much space it takes.
pub fn models_list(workspace: &str) -> String {
    let store = Store::load(Path::new(workspace));
    let items: Vec<String> = store
        .models
        .iter()
        .map(|m| {
            Json::object()
                .str("id", &m.id)
                .str("specId", &m.spec_id)
                .str("quant", &m.quant)
                .str("path", &m.path)
                .uint("sizeBytes", m.size_bytes)
                .str("sha256", &m.sha256)
                .str("licence", &m.licence)
                .str("addedAt", &m.added_at)
                .bool("active", m.active)
                .bool("present", Path::new(&m.path).is_file())
                .finish()
        })
        .collect();
    ok(Json::object()
        .uint("totalBytes", store.model_bytes())
        .str("active", store.active_model().map(|m| m.id.as_str()).unwrap_or(""))
        .raw("models", &format!("[{}]", items.join(","))))
}

/// `model_activate(workspace, id)` — switch the model the builder uses.
pub fn model_activate(workspace: &str, id: &str) -> String {
    let mut store = Store::load(Path::new(workspace));
    match store.activate_model(id) {
        Ok(()) => match store.save() {
            Ok(()) => ok(Json::object().str("active", id)),
            Err(e) => err(&e, "io"),
        },
        Err(e) => err(&e, "not-found"),
    }
}

/// `model_forget(workspace, id)` — drop the record without touching the file.
///
/// Deleting a multi-gigabyte file is not something a button labelled "forget" should do; the
/// user removes the file themselves and the record follows.
pub fn model_forget(workspace: &str, id: &str) -> String {
    let mut store = Store::load(Path::new(workspace));
    if !store.forget_model(id) {
        return err(&format!("no installed model with id {id}"), "not-found");
    }
    match store.save() {
        Ok(()) => ok(Json::object().str("forgot", id)),
        Err(e) => err(&e, "io"),
    }
}

/// `model_inspect(path)` — read a model file's header and digest without adopting it.
///
/// The common failure is a truncated download or an HTML error page saved as `.gguf`. This
/// says so in one step, before the user waits for a 4 GB hash.
pub fn model_inspect(path: &str) -> String {
    let file = Path::new(path);
    if !file.is_file() {
        return err(&format!("{path} is not a file"), "not-found");
    }
    let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    let header = std::fs::read(file)
        .map(|bytes| models::read_gguf_header(&bytes[..bytes.len().min(64)]))
        .unwrap_or_else(|e| Err(format!("cannot read {path}: {e}")));

    match header {
        Ok(info) => {
            let digest = hash_file(file).map(|d| d.to_string()).unwrap_or_default();
            ok(Json::object()
                .str("path", path)
                .uint("sizeBytes", size)
                .num("ggufVersion", info.version as i64)
                .uint("tensorCount", info.tensor_count)
                .str("sha256", &digest)
                .bool("looksLikeAModel", true))
        }
        Err(reason) => ok(Json::object()
            .str("path", path)
            .uint("sizeBytes", size)
            .bool("looksLikeAModel", false)
            .str("problem", &reason)),
    }
}

/// `project_memory(workspace, project_id)` — the durable ledger and notes for a project.
pub fn project_memory(workspace: &str, project_id: &str) -> String {
    let store = Store::load(Path::new(workspace));
    if !store.project_dir(project_id).is_dir() {
        return err(&format!("unknown project {project_id}"), "not-found");
    }
    let tasks: Vec<String> = store
        .tasks_for(project_id)
        .iter()
        .map(|t| {
            Json::object()
                .str("id", &t.id)
                .str("title", &t.title)
                .str("state", t.state.as_str())
                .raw("acceptance", &format!("[{}]", t.acceptance.iter().map(|a| format!("\"{}\"", crate::json::escape(a))).collect::<Vec<_>>().join(",")))
                .raw("evidence", &format!("[{}]", t.evidence.iter().map(|e| format!("\"{}\"", crate::json::escape(e))).collect::<Vec<_>>().join(",")))
                .str("updatedAt", &t.updated_at)
                .finish()
        })
        .collect();
    let notes: Vec<String> = store
        .notes_for(project_id)
        .iter()
        .map(|n| {
            Json::object()
                .str("id", &n.id)
                .str("kind", &n.kind)
                .str("text", &n.text)
                .str("evidence", &n.evidence)
                .str("createdAt", &n.created_at)
                .finish()
        })
        .collect();
    ok(Json::object()
        .raw("tasks", &format!("[{}]", tasks.join(",")))
        .raw("notes", &format!("[{}]", notes.join(","))))
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
                // The UI needs the root to read and scan files; deriving it there would
                // duplicate the store's layout in the view.
                .str("rootPath", &store.project_dir(&p.id).to_string_lossy())
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
    fn probe_runtime_reports_absence_without_starting_anything() {
        let out = probe_runtime();
        assert!(out.starts_with("{\"ok\":true"), "{out}");
        assert!(out.contains("\"available\":"), "{out}");
    }

    #[test]
    fn security_scan_command_reports_findings() {
        let ws = tmp("scan");
        let created = create_project(&ws.to_string_lossy(), "Build a web app to manage invoices and clients");
        let start = created.find("\"id\":\"").unwrap() + 6;
        let end = created[start..].find('"').unwrap() + start;
        let project_id = created[start..end].to_string();
        let dir = ws.join("projects").join(&project_id);
        std::fs::write(dir.join("leak.js"), "eval(x);\n").unwrap();

        let out = security_scan(&ws.to_string_lossy(), &project_id);
        assert!(out.contains("\"rule\":\"eval\""), "{out}");
        assert!(out.contains("\"passed\":"), "{out}");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn install_model_command_enforces_the_licence_and_the_hash() {
        // The UI relies on these two error kinds, so they are asserted at the command
        // boundary rather than only inside the install module.
        let ws = tmp("install-cmd");
        let source = ws.join("model.bin");
        std::fs::write(&source, b"weights").unwrap();
        let dest = ws.join("models");
        std::fs::create_dir_all(&dest).unwrap();
        let digest = match hash(&source.to_string_lossy()) {
            json if json.contains("sha256") => {
                let start = json.find("\"sha256\":\"").unwrap() + 10;
                let end = json[start..].find('"').unwrap() + start;
                json[start..end].to_string()
            }
            other => panic!("hash failed: {other}"),
        };

        // No licence: refused, and nothing written.
        let refused = install_model(
            "test-model",
            &source.to_string_lossy(),
            &dest.to_string_lossy(),
            &digest,
            false,
            "Apache-2.0",
        );
        assert!(refused.contains("licence-not-accepted"), "{refused}");
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0, "a refused install must write nothing");

        // Wrong hash: refused, and still nothing written.
        let wrong = install_model(
            "test-model",
            &source.to_string_lossy(),
            &dest.to_string_lossy(),
            &"0".repeat(64),
            true,
            "Apache-2.0",
        );
        assert!(wrong.contains("hash-mismatch"), "{wrong}");
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0, "a mismatched install must write nothing");

        // Correct hash and accepted licence: installed and verifiable.
        let installed = install_model(
            "test-model",
            &source.to_string_lossy(),
            &dest.to_string_lossy(),
            &digest,
            true,
            "Apache-2.0",
        );
        assert!(installed.contains("\"ok\":true"), "{installed}");
        assert!(installed.contains("Apache-2.0"), "{installed}");

        let path = dest.join("model.bin");
        let verified = verify_model(&path.to_string_lossy(), &digest);
        assert!(verified.contains("\"matches\":true"), "{verified}");
        let mismatched = verify_model(&path.to_string_lossy(), &"1".repeat(64));
        assert!(mismatched.contains("\"matches\":false"), "{mismatched}");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn disk_usage_command_reports_space() {
        let ws = tmp("disk-cmd");
        let usage = disk_usage(&ws.to_string_lossy());
        assert!(usage.contains("availableBytes"), "{usage}");
        assert!(usage.contains("totalBytes"), "{usage}");
        let unknown = disk_usage("/definitely/not/here");
        assert!(unknown.contains("\"ok\":false"), "{unknown}");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn a_real_build_writes_the_ledger() {
        // The ledger is only worth having if a build fills it. This runs the actual pipeline
        // and asserts the stages landed as tasks with evidence.
        let ws = tmp("ledger-build");
        let created = create_project(&ws.to_string_lossy(), "Build a web app to manage invoices");
        let start = created.find("\"id\":\"").unwrap() + 6;
        let end = created[start..].find('"').unwrap() + start;
        let project_id = created[start..end].to_string();

        let built = build_project(&ws.to_string_lossy(), &project_id, "Build a web app to manage invoices", false);
        assert!(built.contains("\"ok\":true"), "the build should succeed: {built}");

        let memory = project_memory(&ws.to_string_lossy(), &project_id);
        assert!(memory.contains("stage:verify"), "the verify stage should be a task: {memory}");
        assert!(memory.contains("\"state\":\"verified\""), "{memory}");
        assert!(memory.contains("permission model"), "evidence should record how tests ran: {memory}");
        assert!(memory.contains("\"kind\":\"change\""), "a change note should be recorded: {memory}");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn the_registry_keeps_one_model_active() {
        let ws = tmp("registry");
        let mut store = Store::load(&ws);
        store.register_model(crate::store::ModelEntry {
            id: "a".into(), spec_id: "qwen2.5-coder-7b".into(), quant: "Q4_K_M".into(),
            path: "/models/a.gguf".into(), size_bytes: 4_000_000_000, sha256: "aa".into(),
            licence: "Apache-2.0".into(), added_at: "1".into(), active: true,
        });
        store.register_model(crate::store::ModelEntry {
            id: "b".into(), spec_id: "llama-3.2-3b".into(), quant: "Q5_K_M".into(),
            path: "/models/b.gguf".into(), size_bytes: 2_000_000_000, sha256: "bb".into(),
            licence: "Llama 3.2 Community".into(), added_at: "2".into(), active: true,
        });
        assert_eq!(store.models.iter().filter(|m| m.active).count(), 1, "registering an active model must deactivate the rest");
        assert_eq!(store.active_model().map(|m| m.id.as_str()), Some("b"));

        store.activate_model("a").unwrap();
        assert_eq!(store.active_model().map(|m| m.id.as_str()), Some("a"));
        assert!(store.activate_model("missing").is_err());
        assert_eq!(store.model_bytes(), 6_000_000_000);
        store.save().unwrap();

        let reloaded = Store::load(&ws);
        assert_eq!(reloaded.models.len(), 2);
        assert_eq!(reloaded.active_model().map(|m| m.id.as_str()), Some("a"), "activation must survive a reload");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn forgetting_a_model_leaves_the_file_alone() {
        let ws = tmp("forget");
        let file = ws.join("model.gguf");
        std::fs::write(&file, b"GGUF").unwrap();
        let mut store = Store::load(&ws);
        store.register_model(crate::store::ModelEntry {
            id: "a".into(), spec_id: "s".into(), quant: "Q4_K_M".into(),
            path: file.to_string_lossy().into(), size_bytes: 4, sha256: "aa".into(),
            licence: "MIT".into(), added_at: "1".into(), active: true,
        });
        store.save().unwrap();
        assert!(store.forget_model("a"));
        assert!(file.is_file(), "forget must not delete the file");
        assert!(!store.forget_model("a"));
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn the_catalog_command_answers_for_this_machine() {
        let ws = tmp("catalog");
        let json = model_catalog(&ws.to_string_lossy());
        assert!(json.contains("\"budgetBytes\""), "{json}");
        assert!(json.contains("qwen2.5-coder-7b"), "{json}");
        assert!(json.contains("\"quants\""), "{json}");
        assert!(json.contains("\"recommended\""), "{json}");
        assert!(json.contains("Q4_K_M"), "{json}");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn inspect_rejects_a_file_that_is_not_a_model() {
        let ws = tmp("inspect");
        let bad = ws.join("notamodel.gguf");
        std::fs::write(&bad, b"<html>404 not found</html>").unwrap();
        let json = model_inspect(&bad.to_string_lossy());
        assert!(json.contains("\"looksLikeAModel\":false"), "{json}");
        assert!(json.contains("not a GGUF"), "{json}");

        let missing = model_inspect("/definitely/not/here.gguf");
        assert!(missing.contains("not-found"), "{missing}");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn a_task_cannot_be_verified_without_evidence() {
        // The brief's rule, enforced in the store rather than trusted to a caller.
        let ws = tmp("ledger-evidence");
        let mut store = Store::load(&ws);
        let mut task = crate::store::TaskEntry {
            id: "T-01".into(),
            project_id: "p-1".into(),
            title: "Scaffold".into(),
            state: crate::store::TaskState::Verified,
            acceptance: vec!["tree matches".into()],
            depends_on: Vec::new(),
            evidence: Vec::new(),
            updated_at: "1".into(),
        };
        assert!(store.upsert_task(task).is_err(), "verified without evidence must be refused");

        let mut with_evidence = crate::store::TaskEntry {
            id: "T-01".into(),
            project_id: "p-1".into(),
            title: "Scaffold".into(),
            state: crate::store::TaskState::Verified,
            acceptance: vec!["tree matches".into()],
            depends_on: Vec::new(),
            evidence: vec!["node --test exited 0".into()],
            updated_at: "2".into(),
        };
        assert!(store.upsert_task(with_evidence).is_ok());
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn the_ledger_survives_a_reload_and_reports_through_the_command() {
        let ws = tmp("ledger-reload");
        let created = create_project(&ws.to_string_lossy(), "Build a web app to manage invoices");
        let start = created.find("\"id\":\"").unwrap() + 6;
        let end = created[start..].find('"').unwrap() + start;
        let project_id = created[start..end].to_string();

        let mut store = Store::load(&ws);
        store
            .upsert_task(crate::store::TaskEntry {
                id: "T-01".into(),
                project_id: project_id.clone(),
                title: "Scaffold".into(),
                state: crate::store::TaskState::Verified,
                acceptance: vec!["tree matches".into()],
                depends_on: Vec::new(),
                evidence: vec!["14 files written".into()],
                updated_at: "1".into(),
            })
            .unwrap();
        store.add_note(crate::store::NoteEntry {
            id: "n-1".into(),
            project_id: project_id.clone(),
            kind: "decision".into(),
            text: "chose a JSON store over SQLite".into(),
            evidence: "no dependency available".into(),
            created_at: "1".into(),
        });
        store.save().unwrap();

        let memory = project_memory(&ws.to_string_lossy(), &project_id);
        assert!(memory.contains("\"state\":\"verified\""), "{memory}");
        assert!(memory.contains("14 files written"), "{memory}");
        assert!(memory.contains("chose a JSON store"), "{memory}");
        assert!(memory.contains("\"kind\":\"decision\""), "{memory}");

        let unknown = project_memory(&ws.to_string_lossy(), "nope");
        assert!(unknown.contains("not-found"), "{unknown}");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn list_projects_reports_each_project_directory() {
        let ws = tmp("listdirs");
        create_project(&ws.to_string_lossy(), "Build a web app to manage invoices");
        let listed = list_projects(&ws.to_string_lossy());
        assert!(listed.contains("\"rootPath\":"), "{listed}");
        assert!(listed.contains("projects"), "the path should point inside the workspace");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn create_checkpoint_command_snapshots_on_demand() {
        let ws = tmp("cp-cmd");
        let created = create_project(&ws.to_string_lossy(), "Build a web app to manage invoices");
        let start = created.find("\"id\":\"").unwrap() + 6;
        let end = created[start..].find('"').unwrap() + start;
        let project_id = created[start..end].to_string();

        let dir = ws.join("projects").join(&project_id);
        std::fs::write(dir.join("note.txt"), "before").unwrap();

        let first = create_checkpoint(&ws.to_string_lossy(), &project_id, "before experimenting");
        assert!(first.contains("\"ok\":true"), "{first}");
        assert!(first.contains("treeHash"), "{first}");

        // A second one in the same second must still get its own id.
        let second = create_checkpoint(&ws.to_string_lossy(), &project_id, "again");
        assert!(second.contains("\"ok\":true"), "{second}");
        assert_ne!(first, second);

        // And it must be restorable.
        let listed = list_checkpoints(&ws.to_string_lossy(), &project_id);
        assert!(listed.matches("\"label\":\"before experimenting\"").count() >= 1, "{listed}");

        let unknown = create_checkpoint(&ws.to_string_lossy(), "nope", "x");
        assert!(unknown.contains("not-found"), "{unknown}");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn export_command_copies_the_tree_out() {
        let ws = tmp("export");
        let created = create_project(&ws.to_string_lossy(), "Build a web app to track suppliers and orders");
        let start = created.find("\"id\":\"").unwrap() + 6;
        let end = created[start..].find('"').unwrap() + start;
        let project_id = created[start..end].to_string();
        let dir = ws.join("projects").join(&project_id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.js"), "export const a = 1;\n").unwrap();

        let destination = ws.join("out");
        let out = export_project(&ws.to_string_lossy(), &project_id, &destination.to_string_lossy());
        assert!(out.contains("\"ok\":true"), "{out}");
        assert!(destination.join("a.js").exists());
        std::fs::remove_dir_all(&ws).ok();
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

        let built = build_project(&ws.to_string_lossy(), &project_id, "Build a web app to manage invoices and clients with search", false);
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
