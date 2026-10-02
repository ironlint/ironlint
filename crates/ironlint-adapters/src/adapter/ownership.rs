//! Narrow install recovery: one intent record per locked artifact group.
use super::materialize::{
    atomic_write, read_sidecar, sha256_hex, sidecar_path, write_sidecar, AdapterSidecar,
};
use super::ops::InstallResult;
use crate::filesystem::{regular_file, replace_executable, sync_directory};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

type Files = BTreeMap<String, String>;

#[derive(Serialize, Deserialize)]
struct PendingInstall {
    previous: Files,
    desired: Files,
}

pub fn pending_sidecar_path(dir: &Path) -> PathBuf {
    dir.join(".ironlint-adapter.pending.json")
}

struct Ownership {
    sidecar: Option<AdapterSidecar>,
    pending: Option<PendingInstall>,
}
impl Ownership {
    fn read(dir: &Path) -> Result<Self> {
        regular_file(&sidecar_path(dir))?;
        let path = pending_sidecar_path(dir);
        let pending = if regular_file(&path)?.is_some() {
            Some(
                serde_json::from_slice(&std::fs::read(&path)?)
                    .with_context(|| format!("parsing {}", path.display()))?,
            )
        } else {
            None
        };
        Ok(Self {
            sidecar: read_sidecar(dir)?,
            pending,
        })
    }

    fn owns(&self, name: &str, hash: &str) -> bool {
        self.sidecar
            .as_ref()
            .is_some_and(|s| s.files.get(name).is_some_and(|h| h == hash))
            || self.pending.as_ref().is_some_and(|p| {
                p.previous.get(name).is_some_and(|h| h == hash)
                    || p.desired.get(name).is_some_and(|h| h == hash)
            })
    }
}

fn directory_is_safe(dir: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(dir) {
        Ok(meta) => Ok(meta.is_dir()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e).with_context(|| format!("inspecting {}", dir.display())),
    }
}

fn read_hash(path: &Path) -> Result<Option<String>> {
    if regular_file(path)?.is_none() {
        return Ok(None);
    }
    Ok(Some(sha256_hex(
        &std::fs::read(path).with_context(|| format!("reading {}", path.display()))?,
    )))
}

fn preserved(path: &Path) -> InstallResult {
    InstallResult::Skipped(format!(
        "preserved edited, foreign, or non-regular {}; review manually",
        path.display()
    ))
}

/// Caller holds the group lock. Intent is published before any artifact write.
pub(super) fn install_files(
    dir: &Path,
    sources: &[(&str, &[u8])],
    executable: bool,
) -> Result<InstallResult> {
    if !directory_is_safe(dir)? {
        return Ok(preserved(dir));
    }
    let ownership = Ownership::read(dir)?;
    let desired: Files = sources
        .iter()
        .map(|(name, bytes)| ((*name).into(), sha256_hex(bytes)))
        .collect();
    let mut previous = Files::new();
    for (name, _) in sources {
        let path = dir.join(name);
        if let Some(hash) = read_hash(&path)? {
            if !ownership.owns(name, &hash) {
                return Ok(preserved(&path));
            }
            previous.insert((*name).into(), hash);
        }
    }
    let existed = !previous.is_empty();
    let unchanged = previous == desired;
    if unchanged
        && ownership.pending.is_none()
        && ownership
            .sidecar
            .as_ref()
            .is_some_and(|s| s.files == desired)
    {
        return Ok(InstallResult::AlreadyPresent);
    }
    let intent = PendingInstall {
        previous,
        desired: desired.clone(),
    };
    atomic_write(
        &pending_sidecar_path(dir),
        &serde_json::to_vec_pretty(&intent)?,
    )?;
    for (name, bytes) in sources {
        if intent.previous.get(*name) == desired.get(*name) {
            continue;
        }
        let path = dir.join(name);
        if executable {
            replace_executable(&path, bytes)?;
        } else {
            atomic_write(&path, bytes)?;
        }
    }
    write_sidecar(dir, &AdapterSidecar { files: desired })?;
    std::fs::remove_file(pending_sidecar_path(dir))
        .context("installed artifacts but could not remove recovery record; retry")?;
    sync_directory(dir).context(
        "installed artifacts but recovery cleanup durability could not be confirmed; retry",
    )?;
    Ok(if !existed {
        InstallResult::Installed
    } else if unchanged {
        InstallResult::AlreadyPresent
    } else {
        InstallResult::Updated
    })
}

/// Preflight all files before removing any, retaining foreign edits on failure.
pub(super) fn remove_owned_files(dir: &Path, names: &[&str]) -> Result<InstallResult> {
    if !directory_is_safe(dir)? {
        return Ok(preserved(dir));
    }
    if !dir.exists() {
        return Ok(InstallResult::Installed);
    }
    let ownership = Ownership::read(dir)?;
    if ownership.sidecar.is_none() && ownership.pending.is_none() {
        return Ok(preserved(dir));
    }
    let mut files = Vec::new();
    for name in names {
        let path = dir.join(name);
        if let Some(hash) = read_hash(&path)? {
            if !ownership.owns(name, &hash) {
                return Ok(preserved(&path));
            }
            files.push(path);
        }
    }
    for file in files {
        std::fs::remove_file(&file)
            .with_context(|| format!("removing owned {}", file.display()))?;
    }
    for record in [sidecar_path(dir), pending_sidecar_path(dir)] {
        match std::fs::remove_file(&record) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("removing {}", record.display())),
        }
    }
    match std::fs::remove_dir(dir) {
        Ok(()) => Ok(InstallResult::Installed),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(InstallResult::Installed),
        Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
            Ok(InstallResult::Skipped(format!(
                "preserved additional content in {}; review manually",
                dir.display()
            )))
        }
        Err(e) => Err(e).with_context(|| format!("removing {}", dir.display())),
    }
}

#[cfg(test)]
mod tests;
