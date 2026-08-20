//! E2e for `ironlint gate-bash`: stdin command → exit 0 (allow) / 2 (block).
//! Mirrors `cli_e2e_trust`'s `assert_cmd` harness.
//!
//! Pins the binary exit contract and the "no config / no trust needed"
//! property: the subcommand runs in a bare temp dir with no .ironlint.yml.

use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn allows_ironlint_check_on_stdin() {
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.args(["gate-bash"]).write_stdin("ironlint check");
    cmd.assert()
        .success()
        .code(0)
        .stdout(predicates::str::is_empty());
}

#[test]
fn blocks_ironlint_trust_on_stdin() {
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.args(["gate-bash"]).write_stdin("ironlint trust");
    cmd.assert()
        .failure()
        .code(2)
        .stdout(predicates::str::contains(
            "ironlint trust must be run by a human",
        ));
}

#[test]
fn blocks_redirect_to_ironlint_yml_on_stdin() {
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.args(["gate-bash"])
        .write_stdin("echo x > .ironlint.yml");
    cmd.assert()
        .failure()
        .code(2)
        .stdout(predicates::str::contains(
            "must be edited through the Write/Edit tool",
        ));
}

#[test]
fn allows_empty_stdin() {
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.args(["gate-bash"]).write_stdin("");
    cmd.assert()
        .success()
        .code(0)
        .stdout(predicates::str::is_empty());
}

#[test]
fn runs_with_no_config_present() {
    // The bash-gate is not trust-gated and needs no .ironlint.yml: run it in
    // a bare temp dir with no config, no trust store. It must still decide.
    let dir = tempdir().unwrap();
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.args(["gate-bash"])
        .current_dir(dir.path())
        .write_stdin("ironlint trust");
    cmd.assert()
        .failure()
        .code(2)
        .stdout(predicates::str::contains(
            "ironlint trust must be run by a human",
        ));
}

#[test]
fn allows_indirection_known_gap() {
    // Pinned: variable-substitution indirection MUST allow (documented gap).
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.args(["gate-bash"])
        .write_stdin("iron$(echo lint) trust");
    cmd.assert()
        .success()
        .code(0)
        .stdout(predicates::str::is_empty());
}

#[test]
fn allows_non_utf8_stdin_without_crashing() {
    // Defensive: a genuinely non-UTF8 stdin should not be reachable from the
    // adapters (they send UTF-8 command text), but the subcommand must not crash
    // if it ever happens. It allows (exit 0) and logs to stderr — never panics.
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.args(["gate-bash"])
        .write_stdin(b"\xff\xfe ironlint trust");
    cmd.assert().success().code(0);
}

// ---------------------------------------------------------------------------
// W3 — harness self-defense surface end-to-end
// ---------------------------------------------------------------------------

#[test]
fn blocks_adapter_surface_write_on_stdin() {
    // A Bash write to a harness install artifact must block (exit 2) with
    // the self-defense reason.
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.args(["gate-bash"])
        .write_stdin("rm -rf ~/.claude/settings.json");
    cmd.assert()
        .failure()
        .code(2)
        .stdout(predicates::str::contains(
            "must be edited through the Write/Edit tool",
        ));
}

#[test]
fn blocks_git_floor_bypass_on_stdin() {
    // The lazy-agent escape from the W1 commit-boundary rail.
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.args(["gate-bash"])
        .write_stdin("git commit --no-verify -m x");
    cmd.assert()
        .failure()
        .code(2)
        .stdout(predicates::str::contains("git commit bypass"));
}

#[test]
fn blocks_git_config_env_injection_on_stdin() {
    // `GIT_CONFIG_*` env vars are git's env spelling of `-c`: the KEY segment
    // carries core.hooksPath, so it must block with the same bypass reason.
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.args(["gate-bash"]).write_stdin(
        "env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/tmp/x git commit -m x",
    );
    cmd.assert()
        .failure()
        .code(2)
        .stdout(predicates::str::contains("git commit bypass"));
}

#[test]
fn allows_surface_file_read_on_stdin() {
    // Reading a harness settings file is a legitimate operation.
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.args(["gate-bash"])
        .write_stdin("cat ~/.claude/settings.json");
    cmd.assert()
        .success()
        .code(0)
        .stdout(predicates::str::is_empty());
}

#[test]
fn allows_plain_git_commit_on_stdin() {
    // A normal commit (no bypass flags) is not an escape.
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.args(["gate-bash"]).write_stdin("git commit -m x");
    cmd.assert()
        .success()
        .code(0)
        .stdout(predicates::str::is_empty());
}

// ---------------------------------------------------------------------------
// W3-R2 parity: the gate-bash blocklist must equal the registry-derived
// adapter installation surface. Adding a fifth adapter or moving an install
// path in `adapter/registry.rs` fails THIS test first — before the gate can
// silently under-cover the new surface.
// ---------------------------------------------------------------------------

#[test]
fn bash_gate_surface_matches_registry_install_targets() {
    let (files, dirs) = ironlint_core::adapter::adapter_install_surface();
    let mut gate_files: Vec<&str> = ironlint_bash_gate::ADAPTER_SURFACE_FILES.to_vec();
    gate_files.sort_unstable();
    let mut gate_dirs: Vec<&str> = ironlint_bash_gate::ADAPTER_SURFACE_DIRS.to_vec();
    gate_dirs.sort_unstable();
    assert_eq!(
        files, gate_files,
        "gate-bash file blocklist drifted from the registry install targets"
    );
    assert_eq!(
        dirs, gate_dirs,
        "gate-bash dir blocklist drifted from the registry install targets"
    );
}
