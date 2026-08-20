//! `ironlint check --diff` lifecycle dispatch — the W1-R5 pin
//! (specs/2026-08-17-git-floor-hook-and-self-defense-design.md).
//!
//! The git pre-commit floor runs `ironlint check --diff <file>` with NO
//! `--event`, so bare `--diff` must dispatch each check per its own `on:`
//! lifecycle: write-lifecycle checks once per matching file, pre-commit-
//! lifecycle checks once over the whole set (mirroring the bare-sweep
//! dispatch model). Before this fix, bare `--diff` defaulted the event to
//! `write`, so pre-commit-only checks were "event"-skipped and never fired.

mod common;

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const MARKER_NAME: &str = "ironlint-lifecycle-marker";

/// A write-lifecycle probe + a pre-commit-lifecycle probe, both scoped to
/// `**/*.txt`. Each records one line per invocation to `$HOME/MARKER_NAME`:
/// `write|EVENT|IRONLINT_FILE` per file, and
/// `precommit|EVENT|IRONLINT_FILE(or empty)|IRONLINT_FILES (comma-joined)`
/// for the one over-set invocation.
const TWO_LIFECYCLE_CFG: &str = r#"
checks:
  write-probe:
    files: "**/*.txt"
    on: [write]
    run: |
      printf 'write|%s|%s\n' "$IRONLINT_EVENT" "$IRONLINT_FILE" >> "$HOME/ironlint-lifecycle-marker"
  precommit-probe:
    files: "**/*.txt"
    on: [pre-commit]
    run: |
      printf 'precommit|%s|%s|%s\n' "$IRONLINT_EVENT" "${IRONLINT_FILE:-}" "$(printf '%s' "$IRONLINT_FILES" | tr '\n' ',')" >> "$HOME/ironlint-lifecycle-marker"
"#;

/// One dual-lifecycle check (`on: [write, pre-commit]`). Pins that a dual
/// check is batch-dispatched exactly once over the set, never double-run
/// (the same rule the bare sweep's `classify_checks` applies).
const DUAL_LIFECYCLE_CFG: &str = r#"
checks:
  dual-probe:
    files: "**/*.txt"
    on: [write, pre-commit]
    run: |
      printf 'dual|%s\n' "$IRONLINT_EVENT" >> "$HOME/ironlint-lifecycle-marker"
"#;

struct Run {
    out: assert_cmd::assert::Assert,
    marker: PathBuf,
    /// Keeps the marker's HOME dir (and the relay of any tempdir this Run
    /// dropped) alive for the length of the test.
    _home: tempfile::TempDir,
}

/// Seed two on-disk `.txt` files plus a unified diff naming both as
/// modified, then run `ironlint check --diff <patch>` from `dir` with an
/// isolated blessed trust store and a test-controlled HOME (so checks write
/// their marker where the test can read it).
fn run_diff_check(dir: &Path, cfg_body: &str, extra_args: &[&str]) -> Run {
    fs::write(dir.join(".ironlint.yml"), cfg_body).unwrap();
    fs::write(dir.join("a.txt"), "new a\n").unwrap();
    fs::write(dir.join("b.txt"), "new b\n").unwrap();
    let diff_file = dir.join("changes.patch");
    fs::write(
        &diff_file,
        "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old a\n+new a\n\
         --- a/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-old b\n+new b\n",
    )
    .unwrap();

    let xdg = common::blessed_store(&dir.join(".ironlint.yml"));
    let home = tempfile::tempdir().unwrap();
    let marker = home.path().join(MARKER_NAME);

    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.current_dir(dir)
        .env("XDG_CONFIG_HOME", xdg.path())
        .env("HOME", home.path())
        .args(["check", "--diff"])
        .arg(&diff_file)
        .args(["--config", ".ironlint.yml"]);
    for a in extra_args {
        cmd.arg(a);
    }
    Run {
        out: cmd.assert(),
        marker,
        _home: home,
    }
}

fn marker_lines(run: &Run) -> Vec<String> {
    fs::read_to_string(&run.marker)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn successful(out: assert_cmd::assert::Assert) {
    out.success();
}

/// Bare `--diff` (no `--event`): write probe fires once per matching file
/// (2 files → 2 invocations, each with `IRONLINT_EVENT=write` and its own
/// `IRONLINT_FILE`); pre-commit probe fires exactly once over the whole set
/// with `IRONLINT_EVENT=pre-commit`, empty `IRONLINT_FILE`, and both files in
/// `IRONLINT_FILES`. This is the W1-R5 pin — it failed before the fix
/// (bare `--diff` defaulted to the write event and "event"-skipped the
/// pre-commit check).
#[test]
fn implicit_diff_dispatches_per_check_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_diff_check(dir.path(), TWO_LIFECYCLE_CFG, &[]);
    let lines = marker_lines(&run);
    successful(run.out);
    let writes: Vec<&str> = lines
        .iter()
        .map(String::as_str)
        .filter(|l| l.starts_with("write|"))
        .collect();
    let precommits: Vec<&str> = lines
        .iter()
        .map(String::as_str)
        .filter(|l| l.starts_with("precommit|"))
        .collect();

    assert_eq!(
        writes.len(),
        2,
        "write check must fire once per matching file: {lines:?}"
    );
    for f in ["a.txt", "b.txt"] {
        assert!(
            writes.iter().any(|l| l.contains(f)),
            "write invocation for {f} missing: {lines:?}"
        );
    }
    assert!(
        writes.iter().all(|l| l.starts_with("write|write|")),
        "write invocations must carry event `write`: {lines:?}"
    );

    assert_eq!(
        precommits.len(),
        1,
        "pre-commit check must fire exactly once over the set: {lines:?}"
    );
    assert!(
        precommits[0].starts_with("precommit|pre-commit|"),
        "pre-commit invocation must carry event `pre-commit` (and no IRONLINT_FILE): {:?}",
        precommits[0]
    );
    assert!(
        precommits[0].contains("a.txt") && precommits[0].contains("b.txt"),
        "set invocation must list every matching file: {:?}",
        precommits[0]
    );
}

/// Explicit `--event write` keeps single-lifecycle semantics: only the write
/// probe fires, once per file; the pre-commit probe is skipped.
#[test]
fn explicit_write_event_skips_precommit_checks() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_diff_check(dir.path(), TWO_LIFECYCLE_CFG, &["--event", "write"]);
    let lines = marker_lines(&run);
    successful(run.out);
    let writes: Vec<&str> = lines
        .iter()
        .map(String::as_str)
        .filter(|l| l.starts_with("write|"))
        .collect();
    assert_eq!(
        writes.len(),
        2,
        "write check must fire once per file: {lines:?}"
    );
    assert!(
        lines.iter().all(|l| l.starts_with("write|")),
        "no pre-commit probe may fire under `--event write`: {lines:?}"
    );
}

/// Explicit `--event pre-commit` keeps single-lifecycle semantics: one
/// over-set invocation of the pre-commit probe; the write probe is skipped.
#[test]
fn explicit_precommit_event_runs_set_once() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_diff_check(dir.path(), TWO_LIFECYCLE_CFG, &["--event", "pre-commit"]);
    let lines = marker_lines(&run);
    successful(run.out);
    assert_eq!(
        lines.len(),
        1,
        "only the pre-commit probe fires, exactly once: {lines:?}"
    );
    assert!(lines[0].starts_with("precommit|pre-commit|"));
}

/// A dual-lifecycle check under implicit `--diff` is batch-dispatched once
/// over the set (event `pre-commit`), never per-file AND over-set (no
/// double-run) — the same rule `sweep::classify_checks` applies.
#[test]
fn implicit_diff_dual_lifecycle_check_runs_once_batched() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_diff_check(dir.path(), DUAL_LIFECYCLE_CFG, &[]);
    let lines = marker_lines(&run);
    successful(run.out);
    assert_eq!(
        lines,
        vec!["dual|pre-commit".to_string()],
        "dual-lifecycle check must run exactly once, batched: {lines:?}"
    );
}
