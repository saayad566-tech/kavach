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

pub struct Store {
    pub workspace: PathBuf,
    pub projects: Vec<ProjectEntry>,
}

/// Seconds since the epoch, for human-facing timestamps.
fn now() -> String {
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

impl Store {
    fn index_path(workspace: &Path) -> PathBuf {
        workspace.join("index.tsv")
    }

    pub fn load(workspace: &Path) -> Store {
        let mut projects: Vec<ProjectEntry> = Vec::new();
        if let Ok(text) = std::fs::read_to_string(Self::index_path(workspace)) {
            for line in text.lines() {
                let fields: Vec<&str> = line.split('\t').collect();
                match fields.first().copied() {
                    Some("P") if fields.len() >= 7 => projects.push(ProjectEntry {
                        id: fields[1].into(),
                        name: fields[2].into(),
                        app_kind: fields[3].into(),
                        created_at: fields[4].into(),
                        prompt: fields[5].into(),
                        checkpoints: Vec::new(),
                        tasks_verified: fields[6].split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
                        tasks_unverified: Vec::new(),
                    }),
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
        Store { workspace: workspace.to_path_buf(), projects }
    }

    pub fn save(&self) -> Result<(), String> {
        let mut out = String::new();
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
        // Write to a temporary file and rename, so a crash mid-write cannot leave a
        // half-written index. Rename is atomic within a filesystem on every platform we
        // ship; the old index survives if the process dies before the rename.
        let target = Self::index_path(&self.workspace);
        let temp = self.workspace.join("index.tsv.tmp");
        write_project_file(&self.workspace, "index.tsv.tmp", &out).map_err(|e| e.to_string())?;
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
        assert!(before.starts_with("P\t"));

        // Simulate a crash between writing the temp file and renaming: the index must be
        // unchanged and still parseable.
        std::fs::write(ws.join("index.tsv.tmp"), "P\tbroken\t").unwrap();
        let reloaded = Store::load(&ws);
        assert_eq!(reloaded.projects.len(), 1, "a stale temp file must not affect the index");
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
