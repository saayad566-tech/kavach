//! Project index and checkpoints.
//!
//! Deliberately a file, not a database engine: linking SQLite into the desktop binary
//! would add a native dependency that has to be found on every platform, which is one of
//! the most common reasons a Windows build fails. A line-oriented index plus a copy of
//! the tree is enough for a single-user local application, and it can be read by a human.

use std::path::{Path, PathBuf};

use crate::path::{list_project_files, write_project_file};

#[derive(Debug, Clone)]
pub struct CheckpointEntry {
    pub id: String,
    pub label: String,
    pub tree_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ProjectEntry {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub app_kind: String,
    pub created_at: String,
    pub checkpoints: Vec<CheckpointEntry>,
    pub tasks_verified: Vec<String>,
    pub tasks_unverified: Vec<String>,
}

/// The states a task can be in.
///
/// The brief requires eight, and requires that "verified" is reachable only with evidence:
/// generating code is not evidence, and neither is an agent claiming success.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskState {
    Planned,
    Ready,
    InProgress,
    Blocked,
    Failed,
    NeedsVerification,
    Verified,
    Cancelled,
}

impl TaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskState::Planned => "planned",
            TaskState::Ready => "ready",
            TaskState::InProgress => "in-progress",
            TaskState::Blocked => "blocked",
            TaskState::Failed => "failed",
            TaskState::NeedsVerification => "needs-verification",
            TaskState::Verified => "verified",
            TaskState::Cancelled => "cancelled",
        }
    }

    pub fn parse(text: &str) -> TaskState {
        match text {
            "ready" => TaskState::Ready,
            "in-progress" => TaskState::InProgress,
            "blocked" => TaskState::Blocked,
            "failed" => TaskState::Failed,
            "verified" => TaskState::Verified,
            "cancelled" => TaskState::Cancelled,
            // Anything unrecognised is treated as needing verification rather than as done:
            // a corrupt state must never read as success.
            _ => TaskState::NeedsVerification,
        }
    }
}

/// One durable task record.
pub struct TaskEntry {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub state: TaskState,
    pub acceptance: Vec<String>,
    pub depends_on: Vec<String>,
    pub evidence: Vec<String>,
    pub updated_at: String,
}

/// Kinds of memory note. These mirror the categories the brief lists: decisions, lessons,
/// bugs, limitations and changes.
pub struct NoteEntry {
    pub id: String,
    pub project_id: String,
    pub kind: String,
    pub text: String,
    pub evidence: String,
    pub created_at: String,
}

/// A model the user has installed on this machine.
///
/// Workspace-level, not per project: a 4 GB file belongs to the machine, and two projects
/// sharing one copy is the behaviour a user expects.
pub struct ModelEntry {
    pub id: String,
    pub spec_id: String,
    pub quant: String,
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub licence: String,
    pub added_at: String,
    pub active: bool,
}

pub struct Store {
    pub workspace: PathBuf,
    pub projects: Vec<ProjectEntry>,
    pub tasks: Vec<TaskEntry>,
    pub notes: Vec<NoteEntry>,
    pub models: Vec<ModelEntry>,
}

/// Seconds since the epoch, for human-facing timestamps.
pub fn now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

/// A unique id.
///
/// Second-resolution was not enough: two checkpoints taken inside the same second got the
/// same id, so the second silently overwrote the first snapshot while the index kept both
/// entries. Nanoseconds plus a process-local counter makes a collision implausible.
fn unique_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{nanos:x}-{seq:x}")
}

fn clean(value: &str) -> String {
    value.replace(['\t', '\n', '\r'], " ")
}

/// A 64-bit rolling fingerprint of the tree contents: enough to detect drift.
fn tree_hash(files: &[(String, String)]) -> String {
    let mut h1: u64 = 0xcbf29ce484222325;
    let mut h2: u64 = 0x9e3779b97f4a7c15;
    for (path, content) in files {
        for byte in path.bytes().chain([0u8]).chain(content.bytes()) {
            h1 ^= byte as u64;
            h1 = h1.wrapping_mul(0x100000001b3);
            h2 = h2.rotate_left(7) ^ (byte as u64).wrapping_mul(0x9e3779b97f4a7c15);
        }
    }
    format!("{h1:016x}{h2:016x}")
}

/// First line of every index written by this version. Absent in files written before it,
/// which still load — the header is how a future version can migrate rather than guess.
pub const INDEX_HEADER: &str = "# kavach-index v1";

fn split_list(text: &str) -> Vec<String> {
    text.split([';', ','])
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .map(String::from)
        .collect()
}

impl Store {
    /// Record or update a task. Returns an error when a transition would claim success
    /// without evidence — the rule the brief states, enforced rather than documented.
    pub fn upsert_task(&mut self, task: TaskEntry) -> Result<(), String> {
        if task.state == TaskState::Verified && task.evidence.is_empty() {
            return Err(format!(
                "task {} cannot be verified without evidence: generating code is not proof",
                task.id
            ));
        }
        match self.tasks.iter_mut().find(|t| t.project_id == task.project_id && t.id == task.id) {
            Some(existing) => *existing = task,
            None => self.tasks.push(task),
        }
        Ok(())
    }

    pub fn add_note(&mut self, note: NoteEntry) {
        self.notes.push(note);
    }

    pub fn tasks_for(&self, project_id: &str) -> Vec<&TaskEntry> {
        self.tasks.iter().filter(|t| t.project_id == project_id).collect()
    }

    pub fn notes_for(&self, project_id: &str) -> Vec<&NoteEntry> {
        self.notes.iter().filter(|n| n.project_id == project_id).collect()
    }

    /// Add or replace a model record. Only one model is active at a time, so activating one
    /// deactivates the rest — a switch, not a toggle.
    pub fn register_model(&mut self, model: ModelEntry) {
        if model.active {
            for existing in self.models.iter_mut() {
                existing.active = false;
            }
        }
        match self.models.iter_mut().find(|m| m.id == model.id) {
            Some(existing) => *existing = model,
            None => self.models.push(model),
        }
    }

    pub fn activate_model(&mut self, id: &str) -> Result<(), String> {
        if !self.models.iter().any(|m| m.id == id) {
            return Err(format!("no installed model with id {id}"));
        }
        for model in self.models.iter_mut() {
            model.active = model.id == id;
        }
        Ok(())
    }

    pub fn active_model(&self) -> Option<&ModelEntry> {
        self.models.iter().find(|m| m.active)
    }

    pub fn forget_model(&mut self, id: &str) -> bool {
        let before = self.models.len();
        self.models.retain(|m| m.id != id);
        self.models.len() != before
    }

    pub fn model_bytes(&self) -> u64 {
        self.models.iter().map(|m| m.size_bytes).sum()
    }

    fn index_path(workspace: &Path) -> PathBuf {
        workspace.join("index.tsv")
    }

    fn backup_path(workspace: &Path) -> PathBuf {
        workspace.join("index.tsv.bak")
    }

    /// Parse an index body. Returns the projects, tasks, notes and how many lines were
    /// recognisable, so a caller can tell "empty store" from "corrupt file".
    fn parse(text: &str) -> (Vec<ProjectEntry>, usize, Vec<TaskEntry>, Vec<NoteEntry>, Vec<ModelEntry>) {
        let mut projects: Vec<ProjectEntry> = Vec::new();
        let mut tasks: Vec<TaskEntry> = Vec::new();
        let mut notes: Vec<NoteEntry> = Vec::new();
        let mut models: Vec<ModelEntry> = Vec::new();
        let mut valid = 0usize;
        {
            for line in text.lines() {
                if line.trim().is_empty() || line.starts_with('#') {
                    continue;
                }
                let fields: Vec<&str> = line.split('\t').collect();
                match fields.first().copied() {
                    Some("P") if fields.len() >= 7 => {
                        valid += 1;
                        projects.push(ProjectEntry {
                        id: fields[1].into(),
                        name: fields[2].into(),
                        app_kind: fields[3].into(),
                        created_at: fields[4].into(),
                        prompt: fields[5].into(),
                        checkpoints: Vec::new(),
                        tasks_verified: fields[6].split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
                        tasks_unverified: Vec::new(),
                        });
                    }
                    Some("M") if fields.len() >= 10 => {
                        valid += 1;
                        models.push(ModelEntry {
                            id: fields[1].into(),
                            spec_id: fields[2].into(),
                            quant: fields[3].into(),
                            path: fields[4].into(),
                            size_bytes: fields[5].parse().unwrap_or(0),
                            sha256: fields[6].into(),
                            licence: fields[7].into(),
                            added_at: fields[8].into(),
                            active: fields[9] == "1",
                        });
                    }
                    Some("T") if fields.len() >= 9 => {
                        valid += 1;
                        tasks.push(TaskEntry {
                            project_id: fields[1].into(),
                            id: fields[2].into(),
                            state: TaskState::parse(fields[3]),
                            title: fields[4].into(),
                            acceptance: split_list(fields[5]),
                            depends_on: split_list(fields[6]),
                            evidence: split_list(fields[7]),
                            updated_at: fields[8].into(),
                        });
                    }
                    Some("N") if fields.len() >= 7 => {
                        valid += 1;
                        notes.push(NoteEntry {
                            project_id: fields[1].into(),
                            id: fields[2].into(),
                            kind: fields[3].into(),
                            text: fields[4].into(),
                            evidence: fields[5].into(),
                            created_at: fields[6].into(),
                        });
                    }
                    Some("C") if fields.len() >= 6 => {
                        if let Some(project) = projects.iter_mut().find(|p| p.id == fields[1]) {
                            project.checkpoints.push(CheckpointEntry {
                                id: fields[2].into(),
                                label: fields[3].into(),
                                tree_hash: fields[4].into(),
                                created_at: fields[5].into(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        (projects, valid, tasks, notes, models)
    }

    /// Load the index, falling back to the backup when the primary is unreadable.
    ///
    /// A torn write is now unlikely (temp file plus rename), but a disk error, a killed
    /// process between two saves or an edited file are all still possible. Losing every
    /// project because one file is corrupt is not acceptable for a local application.
    pub fn load(workspace: &Path) -> Store {
        let mut recovered = Vec::new();
        for path in [Self::index_path(workspace), Self::backup_path(workspace)] {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let (projects, valid, tasks, notes, models) = Self::parse(&text);
            let meaningful = text
                .lines()
                .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
                .count();
            if valid > 0 || meaningful == 0 {
                return Store { workspace: workspace.to_path_buf(), projects, tasks, notes, models };
            }
            // The file has content but nothing parsed: try the backup instead.
            recovered.push(path);
        }
        Store { workspace: workspace.to_path_buf(), projects: Vec::new(), tasks: Vec::new(), notes: Vec::new(), models: Vec::new() }
    }

    pub fn save(&self) -> Result<(), String> {
        let mut out = String::from(INDEX_HEADER);
        out.push('\n');
        for project in &self.projects {
            out.push_str(&format!(
                "P\t{}\t{}\t{}\t{}\t{}\t{}\n",
                project.id,
                clean(&project.name),
                clean(&project.app_kind),
                project.created_at,
                clean(&project.prompt),
                project.tasks_verified.join(",")
            ));
            for cp in &project.checkpoints {
                out.push_str(&format!(
                    "C\t{}\t{}\t{}\t{}\t{}\n",
                    project.id,
                    cp.id,
                    clean(&cp.label),
                    cp.tree_hash,
                    cp.created_at
                ));
            }
        }
        for model in &self.models {
            out.push_str(&format!(
                "M\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                model.id,
                model.spec_id,
                model.quant,
                clean(&model.path),
                model.size_bytes,
                model.sha256,
                clean(&model.licence),
                model.added_at,
                if model.active { "1" } else { "0" }
            ));
        }
        for task in &self.tasks {
            out.push_str(&format!(
                "T\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                task.project_id,
                task.id,
                task.state.as_str(),
                clean(&task.title),
                clean(&task.acceptance.join(";")),
                task.depends_on.join(","),
                clean(&task.evidence.join(";")),
                task.updated_at
            ));
        }
        for note in &self.notes {
            out.push_str(&format!(
                "N\t{}\t{}\t{}\t{}\t{}\t{}\n",
                note.project_id,
                note.id,
                clean(&note.kind),
                clean(&note.text),
                clean(&note.evidence),
                note.created_at
            ));
        }
        // Write to a temporary file and rename, so a crash mid-write cannot leave a
        // half-written index. Rename is atomic within a filesystem on every platform we
        // ship; the old index survives if the process dies before the rename.
        let target = Self::index_path(&self.workspace);
        let backup = Self::backup_path(&self.workspace);
        let temp = self.workspace.join("index.tsv.tmp");
        write_project_file(&self.workspace, "index.tsv.tmp", &out).map_err(|e| e.to_string())?;
        // Keep the previous good index before replacing it, so a later corruption is
        // recoverable rather than fatal.
        if target.exists() {
            std::fs::copy(&target, &backup).map_err(|e| format!("cannot back up the index: {e}"))?;
        }
        std::fs::rename(&temp, &target).map_err(|e| format!("cannot replace the index: {e}"))
    }

    pub fn project_dir(&self, id: &str) -> PathBuf {
        self.workspace.join("projects").join(id)
    }

    pub fn find(&self, id: &str) -> Option<&ProjectEntry> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub fn create_project(&mut self, prompt: &str, app_kind: &str, name: &str) -> ProjectEntry {
        let stamp = now();
        let entry = ProjectEntry {
            id: format!("p-{stamp}-{}", self.projects.len() + 1),
            name: name.to_string(),
            prompt: prompt.to_string(),
            app_kind: app_kind.to_string(),
            created_at: stamp,
            checkpoints: Vec::new(),
            tasks_verified: Vec::new(),
            tasks_unverified: Vec::new(),
        };
        self.projects.push(entry.clone());
        let _ = std::fs::create_dir_all(self.project_dir(&entry.id));
        entry
    }

    /// Snapshot the tree before a run, so a bad build can be undone.
    pub fn checkpoint(&mut self, project_id: &str, label: &str) -> Result<CheckpointEntry, String> {
        let dir = self.project_dir(project_id);
        let files = list_project_files(&dir);
        let mut snapshot: Vec<(String, String)> = Vec::new();
        for rel in &files {
            if rel.starts_with(".kavach/") {
                continue;
            }
            let content = std::fs::read_to_string(dir.join(rel)).unwrap_or_default();
            snapshot.push((rel.clone(), content));
        }
        let hash = tree_hash(&snapshot);
        let entry = CheckpointEntry {
            id: unique_id("cp"),
            label: label.to_string(),
            tree_hash: hash,
            created_at: now(),
        };
        for (rel, content) in &snapshot {
            write_project_file(&dir.join(".kavach").join("checkpoints").join(&entry.id), rel, content)
                .map_err(|e| e.to_string())?;
        }
        if let Some(project) = self.projects.iter_mut().find(|p| p.id == project_id) {
            project.checkpoints.push(entry.clone());
        }
        self.save()?;
        Ok(entry)
    }

    /// Restore a checkpoint exactly: files added since the snapshot are removed.
    pub fn restore(&mut self, project_id: &str, checkpoint_id: &str) -> Result<(Vec<String>, Vec<String>), String> {
        let dir = self.project_dir(project_id);
        let snapshot_dir = dir.join(".kavach").join("checkpoints").join(checkpoint_id);
        let snapshot = list_project_files(&snapshot_dir);
        let current = list_project_files(&dir);

        let mut removed = Vec::new();
        for rel in &current {
            if rel.starts_with(".kavach/") {
                continue;
            }
            if !snapshot.contains(rel) {
                crate::path::delete_project_file(&dir, rel).map_err(|e| e.to_string())?;
                removed.push(rel.clone());
            }
        }
        let mut restored = Vec::new();
        for rel in &snapshot {
            let content = std::fs::read_to_string(snapshot_dir.join(rel)).unwrap_or_default();
            write_project_file(&dir, rel, &content).map_err(|e| e.to_string())?;
            restored.push(rel.clone());
        }
        Ok((restored, removed))
    }

    pub fn record_outcome(&mut self, project_id: &str, verified: Vec<String>, unverified: Vec<String>) -> Result<(), String> {
        if let Some(project) = self.projects.iter_mut().find(|p| p.id == project_id) {
            project.tasks_verified = verified;
            project.tasks_unverified = unverified;
        }
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kavach-store-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_persists_and_reloads_a_project() {
        let ws = tmp("persist");
        let mut store = Store::load(&ws);
        let project = store.create_project("Build a web app to manage invoices", "web-app", "Invoice app");
        write_project_file(&store.project_dir(&project.id), "src/a.js", "x").unwrap();
        store.save().unwrap();

        let reloaded = Store::load(&ws);
        let found = reloaded.find(&project.id).expect("project should reload");
        assert_eq!(found.name, "Invoice app");
        assert!(found.prompt.contains("invoices"));
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn checkpoint_restores_the_tree_exactly() {
        let ws = tmp("checkpoint");
        let mut store = Store::load(&ws);
        let project = store.create_project("A web app to track suppliers", "web-app", "Suppliers");
        let dir = store.project_dir(&project.id);
        write_project_file(&dir, "src/a.js", "v1").unwrap();

        let cp = store.checkpoint(&project.id, "before run").unwrap();
        write_project_file(&dir, "src/a.js", "v2").unwrap();
        write_project_file(&dir, "src/extra.js", "oops").unwrap();

        let (restored, removed) = store.restore(&project.id, &cp.id).unwrap();
        assert!(restored.contains(&"src/a.js".to_string()));
        assert!(removed.contains(&"src/extra.js".to_string()));
        assert_eq!(std::fs::read_to_string(dir.join("src/a.js")).unwrap(), "v1");
        assert!(!dir.join("src/extra.js").exists());
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn checkpoint_ids_are_unique_within_a_second() {
        let ws = tmp("unique");
        let mut store = Store::load(&ws);
        let project = store.create_project("A web app to track orders", "web-app", "Orders");
        write_project_file(&store.project_dir(&project.id), "a.js", "x").unwrap();

        let first = store.checkpoint(&project.id, "one").unwrap();
        let second = store.checkpoint(&project.id, "two").unwrap();
        let third = store.checkpoint(&project.id, "three").unwrap();
        let mut ids = vec![first.id.clone(), second.id.clone(), third.id.clone()];
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 3, "checkpoint ids must not collide: {first:?} {second:?} {third:?}");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn a_failed_save_leaves_the_previous_index_intact() {
        let ws = tmp("atomic");
        let mut store = Store::load(&ws);
        store.create_project("A web app to track orders", "web-app", "Orders");
        store.save().unwrap();
        let before = std::fs::read_to_string(ws.join("index.tsv")).unwrap();
        assert!(before.starts_with(INDEX_HEADER));
        assert!(before.contains("\nP\t"), "the header must be followed by the records");

        // Simulate a crash between writing the temp file and renaming: the index must be
        // unchanged and still parseable.
        std::fs::write(ws.join("index.tsv.tmp"), "P\tbroken\t").unwrap();
        let reloaded = Store::load(&ws);
        assert_eq!(reloaded.projects.len(), 1, "a stale temp file must not affect the index");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn the_index_carries_a_version_header() {
        let ws = tmp("header");
        let mut store = Store::load(&ws);
        store.create_project("A web app to track orders", "web-app", "Orders");
        store.save().unwrap();
        let text = std::fs::read_to_string(ws.join("index.tsv")).unwrap();
        assert!(text.starts_with(INDEX_HEADER), "got {text:.60}");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn an_index_without_a_header_still_loads() {
        // Files written by an earlier version have no header; they must not be rejected.
        let ws = tmp("legacy");
        std::fs::write(
            ws.join("index.tsv"),
            "P\tp-old\tLegacy\tweb-app\t1700000000\tBuild a legacy app\tT-01\n",
        )
        .unwrap();
        let store = Store::load(&ws);
        assert_eq!(store.projects.len(), 1);
        assert_eq!(store.projects[0].name, "Legacy");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn a_corrupt_index_recovers_from_the_backup() {
        let ws = tmp("recover");
        let mut store = Store::load(&ws);
        store.create_project("First project", "web-app", "First");
        store.save().unwrap();                       // backup is written on the next save
        store.create_project("Second project", "web-app", "Second");
        store.save().unwrap();

        assert_eq!(Store::load(&ws).projects.len(), 2);
        assert!(ws.join("index.tsv.bak").exists(), "a backup should exist after two saves");

        // Corrupt the primary the way a bad disk or an interrupted editor would.
        std::fs::write(ws.join("index.tsv"), "garbage\x00not an index\n").unwrap();
        let recovered = Store::load(&ws);
        assert_eq!(recovered.projects.len(), 1, "should fall back to the backup's single project");
        assert_eq!(recovered.projects[0].name, "First");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn an_empty_store_is_not_mistaken_for_corruption() {
        let ws = tmp("empty");
        assert_eq!(Store::load(&ws).projects.len(), 0);
        let mut store = Store::load(&ws);
        store.create_project("Only project", "web-app", "Only");
        store.save().unwrap();
        assert_eq!(Store::load(&ws).projects.len(), 1);
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn tree_hash_changes_when_content_changes() {
        let a = tree_hash(&[("a".into(), "1".into())]);
        let b = tree_hash(&[("a".into(), "2".into())]);
        assert_ne!(a, b);
        assert_eq!(a, tree_hash(&[("a".into(), "1".into())]));
    }
}
