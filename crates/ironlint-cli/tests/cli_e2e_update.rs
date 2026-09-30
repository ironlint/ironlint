//! End-to-end coverage for `ironlint update`'s no-receipt path.
//!
//! A binary with no install receipt (Homebrew / `cargo install` / source build)
//! must defer with a friendly message and exit 1 — never attempt a network
//! update. Receipt-managed updates use local fake downloaders and installers;
//! they never contact GitHub or replace a live binary.

use assert_cmd::Command;

#[test]
fn update_without_receipt_defers_and_exits_one() {
    // Point every receipt-lookup root at an empty dir so the receipt load fails
    // with NoReceipt regardless of how the test host installed ironlint. This
    // short-circuits before any network call.
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("ironlint")
        .unwrap()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path())
        .env("LOCALAPPDATA", home.path())
        // axoupdater consults this before any home-directory logic, so it makes
        // NoReceipt deterministic regardless of the host's homedir behavior or a
        // stray real receipt on a dev machine.
        .env("AXOUPDATER_CONFIG_PATH", home.path())
        .env_remove("AXOUPDATER_CONFIG_WORKING_DIR")
        .arg("update")
        .assert()
        .failure()
        .code(1)
        .stderr(predicates::str::contains("can't self-update"))
        .stderr(predicates::str::contains("ironlint-cli-installer.sh"));
}

#[cfg(unix)]
fn fake_update(downloader: &str) -> (tempfile::TempDir, assert_cmd::assert::Assert) {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(home.path().join("ironlint-cli-receipt.json"), "{}").unwrap();
    let curl = bin.join("curl");
    std::fs::write(&curl, format!("#!/bin/sh\n{downloader}\n")).unwrap();
    std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o755)).unwrap();
    let assertion = Command::cargo_bin("ironlint")
        .unwrap()
        .current_dir(home.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path())
        .env("TMPDIR", home.path())
        .env("AXOUPDATER_CONFIG_PATH", home.path())
        .env_remove("AXOUPDATER_CONFIG_WORKING_DIR")
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .arg("update")
        .assert();
    (home, assertion)
}

#[cfg(unix)]
#[test]
fn failed_download_is_not_masked_by_a_successful_shell() {
    let (home, assertion) = fake_update("exit 7");
    assertion
        .failure()
        .code(1)
        .stderr(predicates::str::contains("download"));
    assert_no_temporary_installer(home.path());
}

#[cfg(unix)]
#[test]
fn partial_download_is_never_executed() {
    let (home, assertion) = fake_update(
        "payload='touch \"$HOME/installer-ran\"'
         while [ \"$#\" -gt 0 ]; do
           if [ \"$1\" = -o ]; then shift; printf '%s\\n' \"$payload\" > \"$1\"; exit 7; fi
           shift
         done
         printf '%s\\n' \"$payload\"
         exit 7",
    );
    assert!(
        !home.path().join("installer-ran").exists(),
        "partial installer must not execute"
    );
    assertion.failure().code(1);
    assert_no_temporary_installer(home.path());
}

#[cfg(unix)]
fn assert_no_temporary_installer(home: &std::path::Path) {
    assert!(!std::fs::read_dir(home).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("ironlint-update-")
    }));
}

#[cfg(unix)]
#[test]
fn receipt_managed_success_uses_the_fully_downloaded_local_installer() {
    let (home, assertion) = fake_update(
        "while [ \"$#\" -gt 0 ]; do
           if [ \"$1\" = -o ]; then shift; printf '%s\\n' 'printf success > \"$HOME/installer-ran\"' > \"$1\"; exit 0; fi
           shift
         done
         exit 9",
    );
    assertion
        .success()
        .code(0)
        .stdout(predicates::str::contains("updated ironlint"));
    assert_eq!(
        std::fs::read_to_string(home.path().join("installer-ran")).unwrap(),
        "success"
    );
    assert_no_temporary_installer(home.path());
}
