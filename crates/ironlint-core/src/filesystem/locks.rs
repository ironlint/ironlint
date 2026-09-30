use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

/// Ordered, persistent sibling locks serialize cooperating IronLint mutations.
/// Editors and other programs do not participate in these advisory locks.
pub struct ResourceLocks {
    _files: Vec<File>,
}

impl ResourceLocks {
    pub fn acquire(resources: &[&Path]) -> Result<Self> {
        let mut paths = resources
            .iter()
            .map(|p| lock_path(p))
            .collect::<Result<Vec<_>>>()?;
        paths.sort();
        paths.dedup();
        let mut files = Vec::new();
        for path in paths {
            super::regular_file(&path)?;
            let mut options = OpenOptions::new();
            options.create(true).truncate(false).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(nix::libc::O_NOFOLLOW);
            }
            let file = options
                .open(&path)
                .with_context(|| format!("opening lock {}", path.display()))?;
            #[cfg(unix)]
            fs4::FileExt::lock(&file).with_context(|| format!("locking {}", path.display()))?;
            files.push(file);
        }
        Ok(Self { _files: files })
    }
}

fn lock_path(resource: &Path) -> Result<PathBuf> {
    let parent = resource
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating lock directory {}", parent.display()))?;
    let mut name = resource
        .file_name()
        .context("resource must have a filename")?
        .to_os_string();
    name.push(".ironlint.lock");
    Ok(parent.canonicalize()?.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_are_canonical_ordered_and_deduplicated() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        let _guard = ResourceLocks::acquire(&[&b, &a, &a]).unwrap();
        assert_eq!(
            lock_path(&a).unwrap(),
            tmp.path().canonicalize().unwrap().join("a.ironlint.lock")
        );
        assert!(tmp.path().join("b.ironlint.lock").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_lock_is_refused_without_touching_target() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a");
        let target = tmp.path().join("target");
        std::fs::write(&target, "untouched").unwrap();
        std::os::unix::fs::symlink(&target, lock_path(&path).unwrap()).unwrap();
        assert!(ResourceLocks::acquire(&[&path]).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"untouched");
    }
}
