use super::*;
use std::cell::RefCell;

thread_local! { static FAULT: RefCell<Option<(PathBuf, Stage)>> = const { RefCell::new(None) }; }
pub(crate) struct FaultGuard;
impl Drop for FaultGuard {
    fn drop(&mut self) {
        FAULT.with(|f| *f.borrow_mut() = None);
    }
}
pub(crate) fn fail_at(path: &Path, stage: Stage) -> FaultGuard {
    FAULT.with(|f| *f.borrow_mut() = Some((path.to_path_buf(), stage)));
    FaultGuard
}
pub(crate) fn check_fault(stage: Stage, path: &Path) -> Result<()> {
    FAULT.with(|f| {
        if f.borrow()
            .as_ref()
            .is_some_and(|(target, point)| target == path && *point == stage)
        {
            bail!("injected publication failure");
        }
        Ok(())
    })
}

#[test]
fn concurrent_replacement_writers_publish_whole_payloads() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("shared");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let mut threads = Vec::new();
    for byte in 0..8 {
        let target = target.clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            replace(&target, &vec![byte; 65536]).unwrap();
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
    let bytes = std::fs::read(&target).unwrap();
    assert_eq!(bytes.len(), 65536);
    assert!(bytes.iter().all(|b| *b == bytes[0]));
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
}

#[test]
fn exclusive_temp_collision_retries_and_preserves_foreign_temp() {
    let tmp = tempfile::tempdir().unwrap();
    let collision = tmp.path().join("collision");
    std::fs::write(&collision, b"foreign").unwrap();
    let mut candidates = [collision.clone(), tmp.path().join("owned")].into_iter();
    let (file, guard) =
        create_temp(&tmp.path().join("result"), |_| candidates.next().unwrap()).unwrap();
    drop(file);
    drop(guard);
    assert_eq!(std::fs::read(&collision).unwrap(), b"foreign");
    assert!(!tmp.path().join("owned").exists());
}

#[test]
fn failures_before_publication_leave_original_and_remove_temporary() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("target");
    std::fs::write(&path, b"original").unwrap();
    for stage in [Stage::Created, Stage::BeforePublish] {
        let _fault = fail_at(&path, stage);
        assert!(replace(&path, b"replacement").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
    }
}

#[test]
fn postpublication_failure_reports_published_bytes_and_allows_retry() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("target");
    let fault = fail_at(&path, Stage::Published);
    let error = replace(&path, b"new").unwrap_err();
    assert!(error.to_string().contains("published"), "{error:#}");
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    drop(fault);
    replace(&path, b"new").unwrap();
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn symlink_targets_and_dangling_symlinks_remain_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target");
    let link = tmp.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(replace(&link, b"no").is_err());
    assert!(!target.exists());
    std::fs::write(&target, b"untouched").unwrap();
    assert!(replace(&link, b"no").is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"untouched");
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
}

#[cfg(unix)]
#[test]
fn executable_creation_and_replacement_preserve_modes() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("hook");
    replace_executable(&path, b"new").unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o755
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o751)).unwrap();
    replace_executable(&path, b"replacement").unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o751
    );
}

#[cfg(unix)]
#[test]
fn d2_explicit_activation_preserves_mode_bits_and_adds_owner_execute() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("hook");
    for mode in [0o644, 0o660, 0o640] {
        std::fs::write(&path, b"old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        let fault = fail_at(&path, Stage::BeforePublish);
        assert!(replace_executable(&path, b"new").is_err());
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            mode
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        drop(fault);
        replace_executable(&path, b"new").unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            mode | 0o100
        );
    }
}

#[test]
fn invalid_parents_and_nonregular_targets_fail() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("directory");
    std::fs::create_dir(&dir).unwrap();
    assert!(replace(&dir, b"no").is_err());
    let file = tmp.path().join("file");
    std::fs::write(&file, b"no").unwrap();
    assert!(replace(&file.join("child"), b"no").is_err());
    assert!(regular_file(&file.join("child")).is_err());
    assert!(create_temp(&file.join("child"), temporary_path).is_err());
}

#[test]
fn exclusive_publication_postpublish_failure_keeps_complete_final_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("backup");
    let permissions = std::fs::metadata(tmp.path()).unwrap().permissions();
    let fault = fail_at(&path, Stage::Published);
    assert!(create_once(&path, b"complete", permissions.clone()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"complete");
    drop(fault);
    assert!(!create_once(&path, b"later", permissions).unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), b"complete");
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
}

#[test]
fn concurrent_first_publications_preserve_one_complete_winner() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("backup");
    let permissions = std::fs::metadata(tmp.path()).unwrap().permissions();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let mut threads = Vec::new();
    for byte in 0..4 {
        let path = path.clone();
        let permissions = permissions.clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            create_once(&path, &vec![byte; 65536], permissions).unwrap()
        }));
    }
    let winners = threads
        .into_iter()
        .map(|t| usize::from(t.join().unwrap()))
        .sum::<usize>();
    assert_eq!(winners, 1);
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 65536);
    assert!(bytes.iter().all(|b| *b == bytes[0]));
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
}
