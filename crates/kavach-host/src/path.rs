//! Path safety — the single choke point for every path derived from model output or a
//! prompt. A path that escapes the project root is a security event.

use std::path::{Component, Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum PathError {
    Absolute(String),
    Escapes(String),
    Empty,
}

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathError::Absolute(p) => write!(f, "absolute paths are not written into a project: {p}"),
            PathError::Escapes(p) => write!(f, "path escapes the project root: {p}"),
            PathError::Empty => write!(f, "empty path"),
        }
    }
}

impl std::error::Error for PathError {}

/// Resolve `rel` inside `root`, rejecting absolute paths, `..` traversal and empty input.
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, PathError> {
    if rel.trim().is_empty() {
        return Err(PathError::Empty);
    }
    let candidate = Path::new(rel);
    if candidate.is_absolute() || rel.starts_with('\\') || (rel.len() > 1 && rel.as_bytes()[1] == b':') {
        return Err(PathError::Absolute(rel.to_string()));
    }

    let mut out = root.to_path_buf();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir => return Err(PathError::Escapes(rel.to_string())),
            Component::RootDir | Component::Prefix(_) => return Err(PathError::Absolute(rel.to_string())),
        }
    }
    if !out.starts_with(root) {
        return Err(PathError::Escapes(rel.to_string()));
    }
    Ok(out)
}

/// Confirm that symlinks do not lead outside the project.
///
/// `safe_join` is lexical. A symlink planted inside a project would still be followed, so
/// the nearest existing ancestor is canonicalised and compared against the canonical root.
pub fn assert_real_path_inside(root: &Path, target: &Path) -> Result<(), PathError> {
    let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut probe = target.to_path_buf();
    loop {
        if probe.exists() {
            match std::fs::canonicalize(&probe) {
                Ok(canonical) => {
                    if !canonical.starts_with(&canonical_root) {
                        return Err(PathError::Escapes(target.to_string_lossy().to_string()));
                    }
                    return Ok(());
                }
                Err(_) => return Err(PathError::Escapes(target.to_string_lossy().to_string())),
            }
        }
        match probe.parent() {
            Some(parent) => probe = parent.to_path_buf(),
            None => return Ok(()),
        }
    }
}

/// Write a file inside the project, creating parent directories.
///
/// The root is created first: `assert_real_path_inside` canonicalises the root, and a root
/// that does not exist yet cannot be canonicalised — which made writes into a fresh
/// subdirectory (a checkpoint, say) fail for the wrong reason.
pub fn write_project_file(root: &Path, rel: &str, content: &str) -> Result<PathBuf, PathError> {
    let target = safe_join(root, rel)?;
    std::fs::create_dir_all(root).map_err(|_| PathError::Escapes(root.to_string_lossy().to_string()))?;
    assert_real_path_inside(root, &target)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|_| PathError::Escapes(rel.to_string()))?;
    }
    std::fs::write(&target, content).map_err(|_| PathError::Escapes(rel.to_string()))?;
    Ok(target)
}

/// Delete a file inside the project.
pub fn delete_project_file(root: &Path, rel: &str) -> Result<PathBuf, PathError> {
    let target = safe_join(root, rel)?;
    assert_real_path_inside(root, &target)?;
    std::fs::remove_file(&target).map_err(|_| PathError::Escapes(rel.to_string()))?;
    Ok(target)
}

/// List project files, excluding build and dependency directories.
pub fn list_project_files(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        let ignored = ["node_modules", ".git", "dist", "target", ".kavach"];
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if ignored.contains(&name.as_str()) {
                continue;
            }
            let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
            if entry.path().is_dir() {
                walk(&entry.path(), &rel, out);
            } else if entry.path().is_file() {
                out.push(rel);
            }
        }
    }
    let mut files = Vec::new();
    walk(root, "", &mut files);
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kavach-path-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        let root = Path::new("/tmp/project");
        assert!(matches!(safe_join(root, "../../etc/passwd"), Err(PathError::Escapes(_))));
        assert!(matches!(safe_join(root, "/etc/passwd"), Err(PathError::Absolute(_))));
        assert!(matches!(safe_join(root, "C:\\Windows\\system.ini"), Err(PathError::Absolute(_))));
        assert!(matches!(safe_join(root, "   "), Err(PathError::Empty)));
        assert_eq!(safe_join(root, "src/a.ts").unwrap(), PathBuf::from("/tmp/project/src/a.ts"));
    }

    #[test]
    fn writes_only_inside_the_root() {
        let root = tmp("write");
        assert!(write_project_file(&root, "src/a.txt", "hi").is_ok());
        assert!(write_project_file(&root, "../escape.txt", "no").is_err());
        assert!(!root.parent().unwrap().join("escape.txt").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_a_symlink_that_escapes_the_root() {
        let root = tmp("sym");
        let outside = tmp("outside");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
        #[cfg(not(unix))]
        return;

        assert!(write_project_file(&root, "escape/pwned.txt", "no").is_err());
        assert!(!outside.join("pwned.txt").exists());
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    #[test]
    fn writes_into_a_fresh_nested_directory() {
        let root = tmp("nested");
        let deep = root.join(".kavach").join("checkpoints").join("cp-1");
        assert!(write_project_file(&deep, "src/a.js", "x").is_ok(), "must create the whole path");
        assert!(deep.join("src/a.js").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn lists_and_deletes_project_files() {
        let root = tmp("list");
        write_project_file(&root, "src/a.js", "x").unwrap();
        write_project_file(&root, "docs/b.md", "y").unwrap();
        assert_eq!(list_project_files(&root), vec!["docs/b.md".to_string(), "src/a.js".to_string()]);
        delete_project_file(&root, "src/a.js").unwrap();
        assert_eq!(list_project_files(&root), vec!["docs/b.md".to_string()]);
        std::fs::remove_dir_all(&root).ok();
    }
}
