//! Project export.
//!
//! A builder that traps your work is not a builder. Export copies a project out to a
//! directory the user chooses, skipping Kavach's own bookkeeping, so the result is an
//! ordinary source tree they can put in git, open in an editor, or hand to someone else.

use std::path::{Path, PathBuf};

use crate::path::list_project_files;

#[derive(Debug, PartialEq, Eq)]
pub enum ExportError {
    UnknownProject(String),
    DestinationInsideProject(String),
    Relative(String),
    Io(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::UnknownProject(id) => write!(f, "no project {id} in this workspace"),
            ExportError::DestinationInsideProject(p) => {
                write!(f, "destination {p} is inside the project; choose a directory outside it")
            }
            ExportError::Relative(p) => write!(f, "destination {p} must be an absolute path"),
            ExportError::Io(m) => write!(f, "cannot export: {m}"),
        }
    }
}

impl std::error::Error for ExportError {}

#[derive(Debug)]
pub struct ExportResult {
    pub destination: String,
    pub files: Vec<String>,
    pub bytes: u64,
}

/// Copy a project's files to `destination`.
pub fn export_project(workspace: &Path, project_id: &str, destination: &str) -> Result<ExportResult, ExportError> {
    let dest = Path::new(destination);
    if !dest.is_absolute() {
        return Err(ExportError::Relative(destination.to_string()));
    }

    let project_dir = workspace.join("projects").join(project_id);
    if !project_dir.is_dir() {
        return Err(ExportError::UnknownProject(project_id.to_string()));
    }

    // Canonicalise both sides so a symlinked destination cannot smuggle the copy back inside.
    let canonical_project = std::fs::canonicalize(&project_dir).unwrap_or_else(|_| project_dir.clone());
    if dest.starts_with(&canonical_project) || dest.starts_with(&project_dir) {
        return Err(ExportError::DestinationInsideProject(destination.to_string()));
    }

    std::fs::create_dir_all(dest).map_err(|e| ExportError::Io(e.to_string()))?;

    let mut files = Vec::new();
    let mut bytes = 0u64;
    for rel in list_project_files(&project_dir) {
        if rel.starts_with(".kavach/") {
            continue;
        }
        let source: PathBuf = project_dir.join(&rel);
        let target = dest.join(&rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ExportError::Io(e.to_string()))?;
        }
        std::fs::copy(&source, &target).map_err(|e| ExportError::Io(format!("{rel}: {e}")))?;
        bytes += std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
        files.push(rel);
    }
    files.sort();

    Ok(ExportResult { destination: dest.to_string_lossy().to_string(), files, bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::write_project_file;

    fn workspace(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kavach-export-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn exports_a_project_tree_without_internal_state() {
        let ws = workspace("ok");
        let project = ws.join("projects").join("p-1");
        write_project_file(&project, "src/store.js", "export const x = 1;\n").unwrap();
        write_project_file(&project, "package.json", "{}\n").unwrap();
        write_project_file(&project, ".kavach/checkpoints/cp-1/src/store.js", "old").unwrap();

        let dest = ws.join("out");
        let result = export_project(&ws, "p-1", &dest.to_string_lossy()).unwrap();
        assert_eq!(result.files, vec!["package.json".to_string(), "src/store.js".to_string()]);
        assert!(dest.join("src/store.js").exists());
        assert!(!dest.join(".kavach").exists(), "internal state must not be exported");
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn refuses_an_unknown_project() {
        let ws = workspace("unknown");
        let err = export_project(&ws, "nope", &ws.join("out").to_string_lossy()).unwrap_err();
        assert!(matches!(err, ExportError::UnknownProject(_)));
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn refuses_a_destination_inside_the_project() {
        let ws = workspace("inside");
        let project = ws.join("projects").join("p-1");
        write_project_file(&project, "a.js", "x").unwrap();
        let inside = project.join("export");
        let err = export_project(&ws, "p-1", &inside.to_string_lossy()).unwrap_err();
        assert!(matches!(err, ExportError::DestinationInsideProject(_)));
        std::fs::remove_dir_all(&ws).ok();
    }

    #[test]
    fn refuses_a_relative_destination() {
        let ws = workspace("relative");
        let project = ws.join("projects").join("p-1");
        write_project_file(&project, "a.js", "x").unwrap();
        let err = export_project(&ws, "p-1", "out").unwrap_err();
        assert!(matches!(err, ExportError::Relative(_)));
        std::fs::remove_dir_all(&ws).ok();
    }
}
