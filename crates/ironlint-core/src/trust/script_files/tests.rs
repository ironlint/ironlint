use super::*;
use std::cell::RefCell;

type Hook = Box<dyn FnMut(Point, &Path) -> Result<()>>;
thread_local! { static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) }; }
pub(crate) struct HookGuard;
impl Drop for HookGuard {
    fn drop(&mut self) {
        HOOK.with(|h| *h.borrow_mut() = None);
    }
}
pub(crate) fn with_hook(hook: impl FnMut(Point, &Path) -> Result<()> + 'static) -> HookGuard {
    HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
    HookGuard
}
pub(super) fn checkpoint(point: Point, path: &Path) -> Result<()> {
    HOOK.with(|hook| {
        hook.borrow_mut()
            .as_mut()
            .map_or(Ok(()), |hook| hook(point, path))
    })
}

#[test]
fn large_binary_content_streams_through_one_fixed_buffer() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("big");
    let bytes = vec![0xff; READ_BUFFER * 20 + 3];
    std::fs::write(&path, &bytes).unwrap();
    let entries = collect(tmp.path(), None).unwrap();
    let mut total = 0;
    let mut largest = 0;
    let digest = stream(&entries[0], None, |chunk| {
        total += chunk.len();
        largest = largest.max(chunk.len());
    })
    .unwrap();
    assert_eq!(total, bytes.len());
    assert_eq!(largest, READ_BUFFER);
    assert_eq!(digest.as_slice(), Sha256::digest(&bytes).as_slice());
}

#[test]
fn shrinking_growing_or_replaced_files_are_refused() {
    for mutation in ["shrink", "grow", "replace", "disappear"] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("file");
        std::fs::write(&path, vec![b'x'; READ_BUFFER * 2]).unwrap();
        let entries = collect(tmp.path(), None).unwrap();
        let target = path.clone();
        let mut changed = false;
        let _hook = with_hook(move |point, path| {
            if point == Point::Chunk && path == target && !changed {
                changed = true;
                match mutation {
                    "shrink" => std::fs::write(path, b"small")?,
                    "grow" => {
                        use std::io::Write;
                        std::fs::OpenOptions::new()
                            .append(true)
                            .open(path)?
                            .write_all(b"extra")?;
                    }
                    "replace" => {
                        std::fs::remove_file(path)?;
                        std::fs::write(path, vec![b'y'; READ_BUFFER * 2])?;
                    }
                    _ => std::fs::remove_file(path)?,
                }
            }
            Ok(())
        });
        assert!(stream(&entries[0], None, |_| {}).is_err(), "{mutation}");
    }
}

#[test]
fn expiry_during_read_keeps_typed_total_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("file");
    std::fs::write(&path, vec![b'x'; READ_BUFFER * 2]).unwrap();
    let entries = collect(tmp.path(), None).unwrap();
    let _hook = with_hook(|point, _| {
        if point == Point::Chunk {
            return Err(crate::deadline::TotalTimeout.into());
        }
        Ok(())
    });
    assert!(stream(&entries[0], None, |_| {})
        .unwrap_err()
        .is::<crate::deadline::TotalTimeout>());
    assert!(collect(tmp.path(), Some(Instant::now()))
        .unwrap_err()
        .is::<crate::deadline::TotalTimeout>());
}

#[cfg(unix)]
#[test]
fn symlinked_directories_and_replacement_symlinks_are_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("file");
    std::fs::write(&file, b"file").unwrap();
    let entries = collect(tmp.path(), None).unwrap();
    let target = file.clone();
    let hook = with_hook(move |point, path| {
        if point == Point::Opening {
            std::fs::remove_file(path)?;
            std::os::unix::fs::symlink(&target, path)?;
        }
        Ok(())
    });
    assert!(stream(&entries[0], None, |_| {}).is_err());
    drop(hook);
    std::fs::remove_file(&file).unwrap();
    let dir = tmp.path().join("dir");
    std::os::unix::fs::symlink(tmp.path(), &dir).unwrap();
    assert!(collect(tmp.path(), None)
        .unwrap_err()
        .to_string()
        .contains("symlink"));
}

#[test]
fn empty_file_policy_read_and_nonregular_entries_are_classified() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("empty");
    std::fs::write(&path, b"").unwrap();
    assert!(read_policy(&path, None).unwrap().is_empty());
    assert!(ScriptFile::capture(tmp.path().to_path_buf(), "directory".into()).is_err());
    assert!(collect(&path, None).is_err());
    assert!(read_policy(&tmp.path().join("missing"), None).is_err());
}
