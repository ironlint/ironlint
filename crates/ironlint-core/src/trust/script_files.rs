//! Ordered file metadata and bounded, cooperative content reads.
use super::policy_hash::{classify_entry, EntryKind};
use crate::deadline;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{Metadata, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

pub(super) const READ_BUFFER: usize = 64 * 1024;

#[derive(Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}
impl Stamp {
    fn from(meta: &Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            len: meta.len(),
            modified: meta.modified().ok(),
            #[cfg(unix)]
            identity: (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ScriptFile {
    pub(super) path: PathBuf,
    pub(super) rel: String,
    stamp: Stamp,
}
impl ScriptFile {
    fn capture(path: PathBuf, rel: String) -> Result<Self> {
        let meta = std::fs::symlink_metadata(&path)?;
        if !meta.is_file() {
            bail!("refusing symlink or non-regular file {}", path.display());
        }
        Ok(Self {
            path,
            rel,
            stamp: Stamp::from(&meta),
        })
    }
    pub(super) fn len(&self) -> u64 {
        self.stamp.len
    }
    fn confirm(&self, meta: &Metadata) -> Result<()> {
        if !meta.is_file() || Stamp::from(meta) != self.stamp {
            bail!(
                "file changed or disappeared during verification ({})",
                self.path.display()
            );
        }
        Ok(())
    }
}

pub(super) fn collect(dir: &Path, until: Option<Instant>) -> Result<Vec<ScriptFile>> {
    let mut files = Vec::new();
    collect_into(dir, dir, until, &mut files)?;
    files.sort_by(|a, b| a.rel.cmp(&b.rel));
    checkpoint(Point::Listed, dir)?;
    Ok(files)
}

fn collect_into(
    root: &Path,
    dir: &Path,
    until: Option<Instant>,
    files: &mut Vec<ScriptFile>,
) -> Result<()> {
    deadline::check(until)?;
    if !matches!(classify_entry(dir)?, EntryKind::Dir) {
        bail!(
            "scripts directory changed during traversal ({})",
            dir.display()
        );
    }
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        deadline::check(until)?;
        let path = entry?.path();
        match classify_entry(&path)? {
            EntryKind::Dir => collect_into(root, &path, until, files)?,
            EntryKind::File => {
                let rel = path
                    .strip_prefix(root)?
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                files.push(ScriptFile::capture(path, rel)?);
            }
            EntryKind::Missing => bail!(
                "scripts dir entry disappeared mid-walk ({})",
                path.display()
            ),
        }
    }
    deadline::check(until)
}

/// Feed content through one fixed buffer; never stage a script's bytes.
pub(super) fn stream(
    entry: &ScriptFile,
    until: Option<Instant>,
    mut consume: impl FnMut(&[u8]),
) -> Result<[u8; 32]> {
    deadline::check(until)?;
    checkpoint(Point::Opening, &entry.path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let mut file = options
        .open(&entry.path)
        .with_context(|| format!("opening regular file {}", entry.path.display()))?;
    entry.confirm(&file.metadata()?)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0u8; READ_BUFFER].into_boxed_slice();
    let mut read = 0u64;
    loop {
        deadline::check(until)?;
        let n = match file.read(&mut buffer) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            value => value.with_context(|| format!("reading {}", entry.path.display()))?,
        };
        if n == 0 {
            break;
        }
        read += n as u64;
        if read > entry.len() {
            bail!(
                "file size changed during verification ({})",
                entry.path.display()
            );
        }
        consume(&buffer[..n]);
        digest.update(&buffer[..n]);
        checkpoint(Point::Chunk, &entry.path)?;
    }
    if read != entry.len() {
        bail!(
            "file size changed during verification ({})",
            entry.path.display()
        );
    }
    checkpoint(Point::Complete, &entry.path)?;
    entry.confirm(&file.metadata()?)?;
    entry.confirm(&std::fs::symlink_metadata(&entry.path)?)?;
    deadline::check(until)?;
    Ok(digest.finalize().into())
}

pub(super) fn read_policy(path: &Path, until: Option<Instant>) -> Result<Vec<u8>> {
    let entry = ScriptFile::capture(path.to_path_buf(), String::new())?;
    let mut bytes = Vec::new();
    stream(&entry, until, |chunk| bytes.extend_from_slice(chunk))?;
    Ok(bytes)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Point {
    Listed,
    Opening,
    Chunk,
    Complete,
}
fn checkpoint(point: Point, path: &Path) -> Result<()> {
    #[cfg(test)]
    tests::checkpoint(point, path)?;
    #[cfg(not(test))]
    let _ = (point, path);
    Ok(())
}

#[cfg(test)]
pub(super) mod tests;
