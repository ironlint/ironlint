//! Shared helpers for CLI integration tests.
//!
//! Rust integration tests under `tests/` each compile as their own crate, so
//! helpers used by only some suites appear unused in the others.
#![allow(dead_code)]

pub mod fixtures;

use assert_cmd::Command;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Bless `config` in a fresh, isolated trust store and return the `TempDir`
/// that backs it. Keep the returned guard alive for the test, and set
/// `XDG_CONFIG_HOME` to `guard.path()` on every `ironlint` invocation that runs
/// `check`, so they all read the same blessed store.
#[must_use]
pub fn blessed_store(config: &Path) -> TempDir {
    let xdg = tempfile::tempdir().unwrap();
    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["trust", "--config"])
        .arg(config)
        .assert()
        .success();
    xdg
}

/// Resolve a repository-relative path from this crate's manifest directory.
#[must_use]
pub fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}
