use super::*;
use crate::filesystem::tests::fail_at;
use crate::filesystem::{ResourceLocks, Stage};

#[test]
fn sidecar_failure_after_artifact_publication_repairs_on_identical_retry() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("plugin");
    let fault = fail_at(&sidecar_path(&dir), Stage::BeforePublish);
    assert!(install_files(&dir, &[("file", b"owned")], false).is_err());
    assert_eq!(std::fs::read(dir.join("file")).unwrap(), b"owned");
    assert!(pending_sidecar_path(&dir).exists());
    assert!(read_sidecar(&dir).unwrap().is_none());
    drop(fault);
    install_files(&dir, &[("file", b"owned")], false).unwrap();
    assert!(read_sidecar(&dir).unwrap().is_some());
    assert!(!pending_sidecar_path(&dir).exists());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
}

#[test]
fn postpublication_failure_leaves_intent_and_retries_without_adopting_edits() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("plugin");
    let fault = fail_at(&dir.join("file"), Stage::Published);
    assert!(install_files(&dir, &[("file", b"owned")], false).is_err());
    drop(fault);
    std::fs::write(dir.join("file"), b"user edit").unwrap();
    assert!(matches!(
        install_files(&dir, &[("file", b"owned")], false).unwrap(),
        InstallResult::Skipped(_)
    ));
    assert!(matches!(
        remove_owned_files(&dir, &["file"]).unwrap(),
        InstallResult::Skipped(_)
    ));
    assert_eq!(std::fs::read(dir.join("file")).unwrap(), b"user edit");
    assert!(pending_sidecar_path(&dir).exists());
    assert!(read_sidecar(&dir).unwrap().is_none());
}

#[test]
fn intent_publication_failure_before_and_after_publication_is_recoverable() {
    for stage in [Stage::BeforePublish, Stage::Published] {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("plugin");
        let fault = fail_at(&pending_sidecar_path(&dir), stage);
        assert!(install_files(&dir, &[("file", b"owned")], false).is_err());
        assert!(!dir.join("file").exists());
        drop(fault);
        install_files(&dir, &[("file", b"owned")], false).unwrap();
        assert!(!pending_sidecar_path(&dir).exists());
    }
}

#[test]
fn sidecar_publication_failure_after_publication_is_repairable() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("plugin");
    let fault = fail_at(&sidecar_path(&dir), Stage::Published);
    assert!(install_files(&dir, &[("file", b"owned")], false).is_err());
    drop(fault);
    install_files(&dir, &[("file", b"owned")], false).unwrap();
    assert!(!pending_sidecar_path(&dir).exists());
    assert!(matches!(
        remove_owned_files(&dir, &["file"]).unwrap(),
        InstallResult::Installed
    ));
}

#[test]
fn pending_install_can_be_uninstalled_and_missing_artifacts_repaired() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("plugin");
    let fault = fail_at(&dir.join("file"), Stage::BeforePublish);
    assert!(install_files(&dir, &[("file", b"owned")], false).is_err());
    drop(fault);
    assert!(matches!(
        remove_owned_files(&dir, &["file"]).unwrap(),
        InstallResult::Installed
    ));
    install_files(&dir, &[("file", b"owned")], false).unwrap();
    std::fs::remove_file(dir.join("file")).unwrap();
    install_files(&dir, &[("file", b"owned")], false).unwrap();
    assert_eq!(std::fs::read(dir.join("file")).unwrap(), b"owned");
}

#[test]
fn uninstall_preflights_all_files_and_preserves_additional_content() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("plugin");
    install_files(&dir, &[("first", b"owned"), ("second", b"owned")], false).unwrap();
    std::fs::write(dir.join("second"), b"edited").unwrap();
    assert!(matches!(
        remove_owned_files(&dir, &["first", "second"]).unwrap(),
        InstallResult::Skipped(_)
    ));
    assert!(dir.join("first").exists());
    std::fs::write(dir.join("second"), b"owned").unwrap();
    std::fs::write(dir.join("foreign"), b"foreign").unwrap();
    assert!(matches!(
        remove_owned_files(&dir, &["first", "second"]).unwrap(),
        InstallResult::Skipped(_)
    ));
    assert_eq!(std::fs::read(dir.join("foreign")).unwrap(), b"foreign");
}

#[test]
fn malformed_ownership_record_and_foreign_files_are_never_adopted() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("plugin");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(pending_sidecar_path(&dir), b"broken").unwrap();
    assert!(install_files(&dir, &[("file", b"owned")], false).is_err());
    std::fs::remove_file(pending_sidecar_path(&dir)).unwrap();
    std::fs::write(dir.join("file"), b"owned").unwrap();
    assert!(matches!(
        install_files(&dir, &[("file", b"owned")], false).unwrap(),
        InstallResult::Skipped(_)
    ));
    assert!(matches!(
        remove_owned_files(&dir, &["file"]).unwrap(),
        InstallResult::Skipped(_)
    ));
}

#[cfg(unix)]
#[test]
fn symlinked_group_is_preserved_on_install_and_uninstall() {
    let tmp = tempfile::tempdir().unwrap();
    let foreign = tmp.path().join("foreign");
    std::fs::create_dir(&foreign).unwrap();
    let dir = tmp.path().join("plugin");
    std::os::unix::fs::symlink(&foreign, &dir).unwrap();
    assert!(matches!(
        install_files(&dir, &[("file", b"owned")], false).unwrap(),
        InstallResult::Skipped(_)
    ));
    assert!(matches!(
        remove_owned_files(&dir, &["file"]).unwrap(),
        InstallResult::Skipped(_)
    ));
    assert_eq!(std::fs::read_dir(&foreign).unwrap().count(), 0);
}

#[test]
fn concurrent_owned_upgrades_share_one_resource_group() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("plugin");
    let mut threads = Vec::new();
    for bytes in [b"first".as_slice(), b"second".as_slice()] {
        let dir = dir.clone();
        threads.push(std::thread::spawn(move || {
            let _locks = ResourceLocks::acquire(&[&dir]).unwrap();
            install_files(&dir, &[("file", bytes)], false).unwrap();
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
    let current = std::fs::read(dir.join("file")).unwrap();
    assert_eq!(
        read_sidecar(&dir).unwrap().unwrap().files.get("file"),
        Some(&sha256_hex(&current))
    );
    assert!(!pending_sidecar_path(&dir).exists());
}
