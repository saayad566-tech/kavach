//! Model installation: licence gate, then hash verification, then the copy.
//!
//! The host does not download. Fetching is the shell's job (or a package manager's), and
//! the host verifies whatever arrives — which keeps an HTTP client and TLS stack out of
//! the process that decides what to trust.

use std::path::Path;

use crate::sha256::hash_file;

#[derive(Debug)]
pub enum InstallError {
    LicenceNotAccepted(String),
    HashMismatch { expected: String, actual: String },
    Io(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::LicenceNotAccepted(name) => {
                write!(f, "refusing to install: the {name} licence has not been accepted")
            }
            InstallError::HashMismatch { expected, actual } => {
                write!(f, "hash mismatch: expected {expected}, got {actual}. The file was deleted.")
            }
            InstallError::Io(m) => write!(f, "io error: {m}"),
        }
    }
}

impl std::error::Error for InstallError {}

pub struct InstallRequest<'a> {
    pub model_id: &'a str,
    pub source_file: &'a str,
    pub destination_dir: &'a str,
    pub expected_sha256: &'a str,
    pub licence_accepted: bool,
    pub licence_name: &'a str,
}

#[derive(Debug)]
pub struct InstallResult {
    pub model_id: String,
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub licence_name: String,
}

pub fn install_from_file(request: InstallRequest) -> Result<InstallResult, InstallError> {
    if !request.licence_accepted {
        return Err(InstallError::LicenceNotAccepted(request.licence_name.to_string()));
    }
    let expected = request.expected_sha256.to_lowercase();
    if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(InstallError::Io("expected sha256 must be 64 hex characters".into()));
    }

    let source = Path::new(request.source_file);
    let metadata = std::fs::metadata(source).map_err(|e| InstallError::Io(e.to_string()))?;
    let dest_dir = Path::new(request.destination_dir);
    std::fs::create_dir_all(dest_dir).map_err(|e| InstallError::Io(e.to_string()))?;

    let actual = hash_file(source).map_err(|e| InstallError::Io(e.to_string()))?;
    if actual != expected {
        return Err(InstallError::HashMismatch { expected, actual });
    }

    let file_name = source
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("{}.bin", request.model_id));
    let mut target = dest_dir.join(&file_name);
    if target.exists() {
        target = dest_dir.join(format!("{}-{}", request.model_id, file_name));
    }
    std::fs::copy(source, &target).map_err(|e| InstallError::Io(e.to_string()))?;

    let copied = hash_file(&target).map_err(|e| InstallError::Io(e.to_string()))?;
    if copied != expected {
        std::fs::remove_file(&target).ok();
        return Err(InstallError::HashMismatch { expected, actual: copied });
    }

    Ok(InstallResult {
        model_id: request.model_id.to_string(),
        path: target.to_string_lossy().to_string(),
        size_bytes: metadata.len(),
        sha256: copied,
        licence_name: request.licence_name.to_string(),
    })
}

pub fn verify_installed(path: &str, expected_sha256: &str) -> Result<bool, InstallError> {
    let actual = hash_file(Path::new(path)).map_err(|e| InstallError::Io(e.to_string()))?;
    Ok(actual == expected_sha256.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kavach-install-{}-{}", tag, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn refuses_without_licence_acceptance() {
        let dir = tmp("lic");
        let src = dir.join("m.bin");
        std::fs::write(&src, b"weights").unwrap();
        let err = install_from_file(InstallRequest {
            model_id: "m",
            source_file: &src.to_string_lossy(),
            destination_dir: &dir.join("models").to_string_lossy(),
            expected_sha256: &crate::sha256::hash_bytes(b"weights"),
            licence_accepted: false,
            licence_name: "Llama 3.1 Community License",
        })
        .unwrap_err();
        assert!(matches!(err, InstallError::LicenceNotAccepted(_)));
        assert!(!dir.join("models").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn installs_and_verifies() {
        let dir = tmp("ok");
        let src = dir.join("m.bin");
        std::fs::write(&src, b"weights-bytes").unwrap();
        let hash = crate::sha256::hash_bytes(b"weights-bytes");
        let result = install_from_file(InstallRequest {
            model_id: "qwen",
            source_file: &src.to_string_lossy(),
            destination_dir: &dir.join("models").to_string_lossy(),
            expected_sha256: &hash,
            licence_accepted: true,
            licence_name: "Apache-2.0",
        })
        .unwrap();
        assert_eq!(result.sha256, hash);
        assert!(verify_installed(&result.path, &hash).unwrap());
        assert!(!verify_installed(&result.path, &"a".repeat(64)).unwrap());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_a_tampered_file() {
        let dir = tmp("tamper");
        let src = dir.join("m.bin");
        std::fs::write(&src, b"weights").unwrap();
        let models = dir.join("models");
        let err = install_from_file(InstallRequest {
            model_id: "m",
            source_file: &src.to_string_lossy(),
            destination_dir: &models.to_string_lossy(),
            expected_sha256: &"b".repeat(64),
            licence_accepted: true,
            licence_name: "MIT",
        })
        .unwrap_err();
        assert!(matches!(err, InstallError::HashMismatch { .. }));
        assert!(std::fs::read_dir(&models).map(|mut d| d.next().is_none()).unwrap_or(true));
        std::fs::remove_dir_all(&dir).ok();
    }
}
