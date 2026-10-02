use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use ironlint_core::hash::{sha256_digest_hex, sha256_hex};

/// Write `bytes` to `path` atomically (temp sibling + rename), creating parents.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    crate::filesystem::replace(path, bytes)
}

/// Copy `path` to `<path>.bak` only if the file exists and no backup exists yet.
pub fn backup_once(path: &Path) -> Result<()> {
    let Some(metadata) = crate::filesystem::regular_file(path)? else {
        return Ok(());
    };
    let bak = path.with_extension(format!(
        "{}.bak",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    let bytes =
        std::fs::read(path).with_context(|| format!("reading backup source {}", path.display()))?;
    crate::filesystem::create_once(&bak, &bytes, metadata.permissions())?;
    Ok(())
}

/// Per-harness integrity record, written beside the materialized artifacts.
///
/// No `#[serde(deny_unknown_fields)]`: sidecars written by older binaries carry
/// a `"version"` key that no longer maps to a field. Serde ignores it on read,
/// so those files still deserialize (staleness is now derived from the recorded
/// hashes, not a version counter).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterSidecar {
    /// filename -> "sha256:<hex>"
    pub files: BTreeMap<String, String>,
}

/// `<dir>/.ironlint-adapter.json`.
pub fn sidecar_path(dir: &Path) -> PathBuf {
    dir.join(".ironlint-adapter.json")
}

pub fn write_sidecar(dir: &Path, sidecar: &AdapterSidecar) -> Result<()> {
    let json =
        serde_json::to_string_pretty(sidecar).with_context(|| "serializing adapter sidecar")?;
    atomic_write(&sidecar_path(dir), json.as_bytes())
}

pub fn read_sidecar(dir: &Path) -> Result<Option<AdapterSidecar>> {
    let path = sidecar_path(dir);
    match std::fs::read_to_string(&path) {
        Ok(s) => Ok(Some(serde_json::from_str(&s).with_context(|| {
            format!("parsing adapter sidecar {}", path.display())
        })?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading adapter sidecar {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn sha256_hex_is_prefixed_and_stable() {
        assert_eq!(
            sha256_hex(b"hello"),
            "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn atomic_write_creates_parents_and_content() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("a/b/c.sh");
        atomic_write(&p, b"#!/bin/sh\n").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"#!/bin/sh\n");
    }

    #[test]
    fn d2_atomic_write_never_clobbers_a_colliding_temp_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let collision = path.with_extension("json.tmp");
        std::fs::write(&collision, b"another writer's bytes").unwrap();
        atomic_write(&path, b"new settings").unwrap();
        assert_eq!(
            std::fs::read(&collision).unwrap(),
            b"another writer's bytes"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"new settings");
    }

    #[cfg(unix)]
    #[test]
    fn d2_atomic_write_preserves_existing_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hook.sh");
        std::fs::write(&path, b"old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o751)).unwrap();
        atomic_write(&path, b"new").unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o751
        );
    }

    #[test]
    fn backup_once_preserves_first_original_only() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        std::fs::write(&p, b"original").unwrap();
        backup_once(&p).unwrap();
        std::fs::write(&p, b"changed").unwrap();
        backup_once(&p).unwrap(); // must NOT overwrite the pristine backup
        assert_eq!(
            std::fs::read(p.with_extension("json.bak")).unwrap(),
            b"original"
        );
    }

    #[test]
    fn backup_once_noop_when_file_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("missing.json");
        backup_once(&p).unwrap();
        assert!(!p.with_extension("json.bak").exists());
    }

    #[cfg(unix)]
    #[test]
    fn d2_backup_preserves_a_dangling_symlink_without_writing_its_target() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let backup = path.with_extension("json.bak");
        let target = tmp.path().join("foreign");
        std::fs::write(&path, b"settings").unwrap();
        std::os::unix::fs::symlink(&target, &backup).unwrap();
        backup_once(&path).unwrap();
        assert!(
            !target.exists(),
            "a dangling backup symlink must never redirect writes"
        );
        assert!(std::fs::symlink_metadata(&backup).unwrap().is_symlink());
    }

    #[test]
    fn d2_backup_publication_failure_never_leaves_partial_final_backup() {
        use crate::filesystem::{tests::fail_at, Stage};
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let backup = path.with_extension("json.bak");
        let original = vec![b'x'; 65536];
        std::fs::write(&path, &original).unwrap();
        let fault = fail_at(&backup, Stage::BeforePublish);
        assert!(backup_once(&path).is_err());
        assert!(
            !backup.exists(),
            "failed publication must leave no partial final backup"
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
        drop(fault);
        backup_once(&path).unwrap();
        assert_eq!(std::fs::read(&backup).unwrap(), original);
        std::fs::write(&path, b"later settings").unwrap();
        backup_once(&path).unwrap();
        assert_eq!(std::fs::read(&backup).unwrap(), original);
    }

    #[test]
    fn sidecar_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let mut files = BTreeMap::new();
        files.insert("hook.sh".to_string(), "sha256:abc".to_string());
        let sc = AdapterSidecar { files };
        write_sidecar(tmp.path(), &sc).unwrap();
        let back = read_sidecar(tmp.path()).unwrap().unwrap();
        assert_eq!(back.files.get("hook.sh").unwrap(), "sha256:abc");
    }

    #[test]
    fn read_sidecar_ignores_legacy_version_key() {
        // Back-compat: a sidecar written by a pre-5.21 binary carries a
        // `"version"` key with no matching field. It must still deserialize
        // (unknown key ignored) so an existing install keeps its integrity data.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            sidecar_path(tmp.path()),
            br#"{"version":7,"files":{"hook.sh":"sha256:abc"}}"#,
        )
        .unwrap();
        let back = read_sidecar(tmp.path()).unwrap().unwrap();
        assert_eq!(back.files.get("hook.sh").unwrap(), "sha256:abc");
    }

    #[test]
    fn read_sidecar_absent_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read_sidecar(tmp.path()).unwrap().is_none());
    }

    #[test]
    fn read_sidecar_malformed_json_is_err() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(sidecar_path(tmp.path()), b"{ not json").unwrap();
        assert!(read_sidecar(tmp.path()).is_err());
    }

    #[test]
    fn read_sidecar_io_error_is_err() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(sidecar_path(tmp.path())).unwrap();
        assert!(read_sidecar(tmp.path()).is_err());
    }
}
