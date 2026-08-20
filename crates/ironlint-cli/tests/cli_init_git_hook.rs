//! E2E for the git pre-commit floor hook (specs/2026-08-17-git-floor-hook-and-self-defense-design.md,
//! W1): `ironlint init` installs a marker-bracketed, chaining pre-commit
//! hook that runs `ironlint check --diff` over the staged set; exit mapping
//! per W1-R4; `init --uninstall` removes it.

mod common;

use assert_cmd::Command;
use std::fs;
use std::path::Path;

const MARKER_START: &str = "# >>> ironlint pre-commit floor >>>";
const MARKER_END: &str = "# <<< ironlint pre-commit floor <<<";

/// A pre-commit-only check scoped to `**/*.rs`: blocks when any matched
/// staged file contains TODO. Fires once over the staged set via the floor's
/// bare `check --diff` — the W1-R5 dispatch fix exercised end-to-end.
const PRECOMMIT_TODO_CFG: &str = r#"
checks:
  no-todo:
    files: "**/*.rs"
    on: [pre-commit]
    run: "! grep -q TODO \"$IRONLINT_FILES\""
"#;

/// Fresh git repo: `git init`, identity configured, no commits yet.
fn git_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git_ok(&dir, &["init", "-q"]);
    git_ok(&dir, &["config", "user.email", "test@example.com"]);
    git_ok(&dir, &["config", "user.name", "Test"]);
    dir
}

fn git(dir: &tempfile::TempDir, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir.path())
        .output()
        .expect("git spawn failed")
}

fn git_ok(dir: &tempfile::TempDir, args: &[&str]) {
    let out = git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn hook_file(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let common = git(dir, &["rev-parse", "--git-common-dir"]);
    let common = String::from_utf8(common.stdout).unwrap().trim().to_string();
    let base = if Path::new(&common).is_absolute() {
        std::path::PathBuf::from(common)
    } else {
        dir.path().join(common)
    };
    base.join("hooks").join("pre-commit")
}

/// `ironlint init --yes` from `dir`, with isolated HOME (so no harness is
/// detected) and the given XDG trust store.
fn run_init(
    dir: &tempfile::TempDir,
    xdg: &Path,
    home: &Path,
    extra: &[&str],
) -> assert_cmd::assert::Assert {
    let mut cmd = Command::cargo_bin("ironlint").unwrap();
    cmd.current_dir(dir.path())
        .env("XDG_CONFIG_HOME", xdg)
        .env("HOME", home)
        .args(["init", "--yes"]);
    for a in extra {
        cmd.arg(a);
    }
    cmd.assert()
}

/// `git commit -m t` from `dir` with the given XDG store, HOME, and any
/// extra env vars (e.g. IRONLINT_BIN shim, FAIL_CLOSED). The hook inherits
/// this env, which is how the shim override and trust store reach it.
struct CommitOutcome {
    status: std::process::ExitStatus,
    text: String,
}

impl CommitOutcome {
    fn success(&self) {
        assert!(
            self.status.success(),
            "commit should succeed:\n{}",
            self.text
        );
    }
    fn failure(&self) {
        assert!(!self.status.success(), "commit should fail:\n{}", self.text);
    }
    fn contains(&self, needle: &str) {
        assert!(
            self.text.contains(needle),
            "missing {needle:?} in:\n{}",
            self.text
        );
    }
    fn not_contains(&self, needle: &str) {
        assert!(
            !self.text.contains(needle),
            "unexpected {needle:?} in:\n{}",
            self.text
        );
    }
}

fn commit(
    dir: &tempfile::TempDir,
    xdg: &Path,
    home: &Path,
    envs: &[(&str, &str)],
) -> CommitOutcome {
    let mut c = std::process::Command::new("git");
    c.current_dir(dir.path())
        .args(["commit", "-m", "t"])
        .env("XDG_CONFIG_HOME", xdg)
        .env("HOME", home);
    for (k, v) in envs {
        c.env(k, v);
    }
    let out = c.output().expect("git commit spawn failed");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    CommitOutcome {
        status: out.status,
        text,
    }
}

/// Write an executable shim script that records each invocation to `record`
/// (and exits `code`). Returns the shim path for IRONLINT_BIN.
fn shim(dir: &Path, name: &str, record: &Path, code: i32) -> std::path::PathBuf {
    let p = dir.join(name);
    fs::write(
        &p,
        format!(
            "#!/bin/sh\necho invoked >> \"{}\"\nexit {code}\n",
            record.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    p
}

// ---------------------------------------------------------------------------
// Install / idempotency / uninstall surface
// ---------------------------------------------------------------------------

#[test]
fn init_installs_executable_marker_hook() {
    let dir = git_repo();
    let xdg = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();

    let hook = hook_file(&dir);
    let body = fs::read_to_string(&hook).unwrap();
    assert!(body.starts_with("#!/bin/sh\n"));
    assert!(body.contains(MARKER_START) && body.contains(MARKER_END));
    assert!(body.contains("check --diff \"$diff_file\""));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&hook).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "hook must be executable");
    }
}

#[test]
fn second_init_is_byte_identical() {
    let dir = git_repo();
    let xdg = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();
    let hook = hook_file(&dir);
    let first = fs::read(&hook).unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();
    assert_eq!(
        first,
        fs::read(&hook).unwrap(),
        "reinstall must be idempotent"
    );
}

#[test]
fn init_skips_cleanly_outside_git_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();
}

#[test]
fn no_git_hook_flag_skips_install() {
    let dir = git_repo();
    let xdg = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    run_init(&dir, xdg.path(), home.path(), &["--no-git-hook"]).success();
    assert!(
        !hook_file(&dir).exists(),
        "--no-git-hook must skip the floor"
    );
}

#[test]
fn uninstall_removes_marker_and_keeps_custom_hook() {
    let dir = git_repo();
    let xdg = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    // Pre-existing custom hook; init chains; uninstall must keep it intact.
    let hook = hook_file(&dir);
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    fs::write(&hook, "#!/bin/sh\necho \"custom\"\n").unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();
    assert!(fs::read_to_string(&hook).unwrap().contains(MARKER_START));

    run_init(&dir, xdg.path(), home.path(), &["--uninstall"]).success();
    let body = fs::read_to_string(&hook).unwrap();
    assert!(
        !body.contains(MARKER_START) && !body.contains(MARKER_END),
        "marker must be gone: {body}"
    );
    assert!(body.starts_with("#!/bin/sh\necho \"custom\"\n"));
}

#[test]
fn uninstall_deletes_ironlint_only_hook() {
    let dir = git_repo();
    let xdg = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();
    let hook = hook_file(&dir);
    assert!(hook.exists());
    run_init(&dir, xdg.path(), home.path(), &["--uninstall"]).success();
    assert!(
        !hook.exists(),
        "ironlint-only hook must be deleted on uninstall"
    );
}

// ---------------------------------------------------------------------------
// Behavior at the commit boundary
// ---------------------------------------------------------------------------

fn custom_config_repo(cfg_body: &str) -> (tempfile::TempDir, tempfile::TempDir) {
    let dir = git_repo();
    fs::write(dir.path().join(".ironlint.yml"), cfg_body).unwrap();
    // Bless the custom config into an isolated store shared by init + commits.
    let xdg = common::blessed_store(&dir.path().join(".ironlint.yml"));
    (dir, xdg)
}

#[test]
fn floor_blocks_commit_when_precommit_check_fails() {
    let (dir, xdg) = custom_config_repo(PRECOMMIT_TODO_CFG);
    let home = tempfile::tempdir().unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();

    fs::write(dir.path().join("a.rs"), "// TODO fix this\n").unwrap();
    git_ok(&dir, &["add", "a.rs"]);
    let out = commit(&dir, xdg.path(), home.path(), &[]);
    out.failure();
    out.contains("block: [no-todo]");
}

#[test]
fn floor_allows_commit_when_no_check_matches() {
    let (dir, xdg) = custom_config_repo(PRECOMMIT_TODO_CFG);
    let home = tempfile::tempdir().unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();

    // .txt is out of scope for the .rs-scoped check → nothing fires → pass.
    fs::write(dir.path().join("a.txt"), "fine\n").unwrap();
    git_ok(&dir, &["add", "a.txt"]);
    commit(&dir, xdg.path(), home.path(), &[]).success();
}

#[test]
fn floor_allows_deletions_only_without_invoking_ironlint() {
    let (dir, xdg) = custom_config_repo(PRECOMMIT_TODO_CFG);
    let home = tempfile::tempdir().unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();

    fs::write(dir.path().join("a.txt"), "fine\n").unwrap();
    git_ok(&dir, &["add", "a.txt"]);
    commit(&dir, xdg.path(), home.path(), &[]).success();

    // Record invocations via an IRONLINT_BIN shim; a deletions-only staged
    // set must produce an empty diff (--diff-filter=ACMR) and never spawn.
    let record = home.path().join("invocations.txt");
    let bin = shim(dir.path(), "record-bin", &record, 0);
    git_ok(&dir, &["rm", "a.txt"]);
    let out = commit(
        &dir,
        xdg.path(),
        home.path(),
        &[("IRONLINT_BIN", bin.to_str().unwrap())],
    );
    out.success();
    assert!(
        !record.exists(),
        "deletions-only commit must not invoke ironlint"
    );
}

#[test]
fn chaining_runs_custom_hook_then_floor() {
    let dir = git_repo();
    let xdg = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let sentinel = home.path().join("sentinel.txt");
    let hook = hook_file(&dir);
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    fs::write(
        &hook,
        format!("#!/bin/sh\necho custom-ran >> \"{}\"\n", sentinel.display()),
    )
    .unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();

    fs::write(dir.path().join("a.txt"), "fine\n").unwrap();
    git_ok(&dir, &["add", "a.txt"]);
    let out = commit(&dir, xdg.path(), home.path(), &[]);
    out.success();
    assert!(
        fs::read_to_string(&sentinel)
            .unwrap()
            .contains("custom-ran"),
        "custom hook must run before the floor"
    );
}

#[test]
fn failing_custom_hook_prevents_floor_from_running() {
    let dir = git_repo();
    let xdg = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let record = home.path().join("invocations.txt");
    let hook = hook_file(&dir);
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();

    fs::write(dir.path().join("a.txt"), "fine\n").unwrap();
    git_ok(&dir, &["add", "a.txt"]);
    let out = commit(
        &dir,
        xdg.path(),
        home.path(),
        &[(
            "IRONLINT_BIN",
            shim(dir.path(), "b", &record, 0).to_str().unwrap(),
        )],
    );
    out.failure();
    assert!(
        !record.exists(),
        "git aborts on the failing custom hook before the floor runs"
    );
}

// ---------------------------------------------------------------------------
// Exit mapping (W1-R4): shim binaries standing in for `ironlint check`
// ---------------------------------------------------------------------------

#[test]
fn exit_3_allows_commit_with_warning() {
    let (dir, xdg) = custom_config_repo(PRECOMMIT_TODO_CFG);
    let home = tempfile::tempdir().unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();
    fs::write(dir.path().join("a.rs"), "// TODO\n").unwrap();
    git_ok(&dir, &["add", "a.rs"]);

    let record = home.path().join("invocations.txt");
    let bin = shim(dir.path(), "shim3", &record, 3);
    let out = commit(
        &dir,
        xdg.path(),
        home.path(),
        &[("IRONLINT_BIN", bin.to_str().unwrap())],
    );
    out.success();
    out.contains("internal error — allowing commit");
    assert!(record.exists());
}

#[test]
fn exit_3_with_fail_closed_blocks_commit() {
    let (dir, xdg) = custom_config_repo(PRECOMMIT_TODO_CFG);
    let home = tempfile::tempdir().unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();
    fs::write(dir.path().join("a.rs"), "// TODO\n").unwrap();
    git_ok(&dir, &["add", "a.rs"]);

    let record = home.path().join("invocations.txt");
    let bin = shim(dir.path(), "shim3f", &record, 3);
    let out = commit(
        &dir,
        xdg.path(),
        home.path(),
        &[
            ("IRONLINT_BIN", bin.to_str().unwrap()),
            ("IRONLINT_FAIL_CLOSED_ON_INTERNAL", "1"),
        ],
    );
    out.failure();
    out.contains("blocking (IRONLINT_FAIL_CLOSED_ON_INTERNAL=1)");
}

#[test]
fn exit_4_blocks_commit_with_trust_remediation() {
    let (dir, xdg) = custom_config_repo(PRECOMMIT_TODO_CFG);
    let home = tempfile::tempdir().unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();
    fs::write(dir.path().join("a.rs"), "// TODO\n").unwrap();
    git_ok(&dir, &["add", "a.rs"]);

    let record = home.path().join("invocations.txt");
    let bin = shim(dir.path(), "shim4", &record, 4);
    let out = commit(
        &dir,
        xdg.path(),
        home.path(),
        &[("IRONLINT_BIN", bin.to_str().unwrap())],
    );
    out.failure();
    out.contains("untrusted — run: ironlint trust");
}

#[test]
fn exit_1_blocks_commit() {
    let (dir, xdg) = custom_config_repo(PRECOMMIT_TODO_CFG);
    let home = tempfile::tempdir().unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();
    fs::write(dir.path().join("a.rs"), "// TODO\n").unwrap();
    git_ok(&dir, &["add", "a.rs"]);

    let record = home.path().join("invocations.txt");
    let bin = shim(dir.path(), "shim1", &record, 1);
    let out = commit(
        &dir,
        xdg.path(),
        home.path(),
        &[("IRONLINT_BIN", bin.to_str().unwrap())],
    );
    out.failure();
    out.not_contains("allowing");
}

#[test]
fn missing_binary_allows_commit_with_warning() {
    let (dir, xdg) = custom_config_repo(PRECOMMIT_TODO_CFG);
    let home = tempfile::tempdir().unwrap();
    run_init(&dir, xdg.path(), home.path(), &[]).success();
    fs::write(dir.path().join("a.rs"), "// TODO\n").unwrap();
    git_ok(&dir, &["add", "a.rs"]);

    // IRONLINT_BIN points at a nonexistent path; PATH is stripped of any
    // ironlint, so `command -v ironlint` also fails → hook allows + warns.
    let missing = dir.path().join("no-such-bin");
    let git_dir = std::process::Command::new("which")
        .arg("git")
        .output()
        .unwrap();
    let git_dir = String::from_utf8(git_dir.stdout)
        .unwrap()
        .trim()
        .to_string();
    let git_parent = Path::new(&git_dir)
        .parent()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let out = commit(
        &dir,
        xdg.path(),
        home.path(),
        &[
            ("IRONLINT_BIN", missing.to_str().unwrap()),
            ("PATH", &git_parent),
        ],
    );
    out.success();
    out.contains("binary not found; allowing commit");
}
