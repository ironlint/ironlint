//! Shared file publication. Locks and ownership decisions belong to callers.
use anyhow::{bail, Context, Result};
use std::fs::{File, Metadata, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "filesystem/locks.rs"]
mod locks;
pub use locks::ResourceLocks;

/// Inspect without following a final symlink, including a dangling one.
pub fn regular_file(path: &Path) -> Result<Option<Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => Ok(Some(meta)),
        Ok(_) => bail!("refusing non-regular or symlink target {}", path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("inspecting {}", path.display())),
    }
}

/// Atomically replace a regular file, retaining its permissions.
pub fn replace(path: &Path, bytes: &[u8]) -> Result<()> {
    replace_inner(path, bytes, false)
}

/// Like [`replace`], but explicitly activate owner execution on Unix.
/// New files receive 0755; existing permission bits are retained with owner+x.
pub fn replace_executable(path: &Path, bytes: &[u8]) -> Result<()> {
    replace_inner(path, bytes, true)
}

fn replace_inner(path: &Path, bytes: &[u8], executable: bool) -> Result<()> {
    let existing = regular_file(path)?;
    let mut guard = prepare_temp(path, bytes, existing.map(|m| m.permissions()), executable)?;
    regular_file(path)?;
    std::fs::rename(&guard.path, path).with_context(|| format!("publishing {}", path.display()))?;
    guard.armed = false;
    published(path)
}

/// Publish complete bytes only when the final path does not exist. Existing
/// entries, including dangling symlinks, are preserved. Caller owns its lock.
pub fn create_once(path: &Path, bytes: &[u8], permissions: std::fs::Permissions) -> Result<bool> {
    let mut guard = prepare_temp(path, bytes, Some(permissions), false)?;
    match std::fs::hard_link(&guard.path, path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(e) => {
            return Err(e).with_context(|| format!("exclusively publishing {}", path.display()))
        }
    }
    std::fs::remove_file(&guard.path).with_context(|| {
        format!(
            "published {} but temporary cleanup failed; inspect and retry",
            path.display()
        )
    })?;
    guard.armed = false;
    published(path)?;
    Ok(true)
}

fn prepare_temp(
    path: &Path,
    bytes: &[u8],
    permissions: Option<std::fs::Permissions>,
    executable: bool,
) -> Result<Temporary> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let (mut file, guard) = create_temp(path, temporary_path)?;
    checkpoint(Stage::Created, path)?;
    file.write_all(bytes)
        .with_context(|| format!("writing temporary for {}", path.display()))?;
    let new_file = permissions.is_none();
    if let Some(permissions) = permissions {
        file.set_permissions(permissions)?;
    }
    if executable {
        make_executable(&file, new_file)?;
    }
    file.flush()
        .with_context(|| format!("flushing temporary for {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("syncing temporary for {}", path.display()))?;
    checkpoint(Stage::BeforePublish, path)?;
    Ok(guard)
}

fn published(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    checkpoint(Stage::Published, path)
        .and_then(|()| sync_directory(parent))
        .with_context(|| {
            format!(
                "published {} but directory durability could not be confirmed; inspect and retry",
                path.display()
            )
        })
}

fn temporary_path(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".ironlint-tmp.{}.{n}", std::process::id()));
    path.with_file_name(name)
}

struct Temporary {
    path: PathBuf,
    armed: bool,
}
impl Drop for Temporary {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn create_temp(
    path: &Path,
    mut candidate: impl FnMut(&Path) -> PathBuf,
) -> Result<(File, Temporary)> {
    loop {
        let temp = candidate(path);
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => {
                return Ok((
                    file,
                    Temporary {
                        path: temp,
                        armed: true,
                    },
                ))
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => {
                return Err(e).with_context(|| format!("creating temporary for {}", path.display()))
            }
        }
    }
}

#[cfg(unix)]
fn make_executable(file: &File, new_file: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if new_file {
        0o755
    } else {
        file.metadata()?.permissions().mode() | 0o100
    };
    file.set_permissions(std::fs::Permissions::from_mode(mode))
        .context("setting executable permissions")
}
#[cfg(not(unix))]
fn make_executable(_: &File, _: bool) -> Result<()> {
    Ok(())
}

/// Directory durability is qualified on Unix; Windows is compile-only.
pub fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    Created,
    BeforePublish,
    Published,
}

fn checkpoint(stage: Stage, path: &Path) -> Result<()> {
    #[cfg(test)]
    tests::check_fault(stage, path)?;
    #[cfg(not(test))]
    let _ = (stage, path);
    Ok(())
}

#[cfg(test)]
#[path = "filesystem/tests.rs"]
pub(crate) mod tests;
