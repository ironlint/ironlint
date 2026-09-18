//! Linked-worktree trust inheritance, exercised through REAL `git worktree add`
//! metadata — not a hand-crafted directory fixture. These are the acceptance
//! tests for docs/security/trust.md.

use ironlint_core::trust::{bless_in, check_trust_in, TrustOutcome};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use tempfile::tempdir;

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Set up a primary git repo with a config + a blocking check script, plus a
/// linked worktree sibling. Returns (primary_cfg, linked_cfg, store_path, script).
fn worktree_pair() -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    // Convert the tempdirs to plain paths so the directories survive the
    // helper's return. Use a non-hidden final path component for the worktree
    // itself, because `git worktree add` derives a branch name from it and
    // branch names may not start with `.` (tempfile names do).
    let primary = tempdir().unwrap().keep();
    let linked_root = tempdir().unwrap().keep();
    // git init the primary
    assert!(Command::new("git")
        .args(["init", "-q"])
        .arg(&primary)
        .status()
        .unwrap()
        .success());
    // write a config + a script that BLOCKS (exits 1) so we can prove check ran
    let cfg = primary.join(".ironlint.yml");
    std::fs::write(
        &cfg,
        "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \".ironlint/scripts/g.sh\"\n",
    )
    .unwrap();
    let scripts = primary.join(".ironlint/scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    let script = scripts.join("g.sh");
    std::fs::write(&script, "#!/bin/sh\nexit 1\n").unwrap();
    // commit so `git worktree add` has a HEAD
    Command::new("git")
        .args(["add", "."])
        .current_dir(&primary)
        .status()
        .unwrap();
    Command::new("git")
        .args([
            "-c",
            "user.email=t@t.co",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ])
        .current_dir(&primary)
        .status()
        .unwrap();
    // add a linked worktree
    let linked_wt = linked_root.join("wt");
    assert!(Command::new("git")
        .args(["worktree", "add", "-q"])
        .arg(&linked_wt)
        .current_dir(&primary)
        .status()
        .unwrap()
        .success());
    let linked_cfg = linked_wt.join(".ironlint.yml");
    let store = tempdir().unwrap();
    let store_path = store.path().join("trust.json");
    (cfg, linked_cfg, store_path, script)
}

/// Primary git repo whose config lives at a nested path (e.g.
/// `pkg/sub/.ironlint.yml`) and a linked worktree sibling with the same nested
/// layout. Returns (primary_cfg, linked_cfg, store_path, script).
fn nested_worktree_pair() -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let primary = tempdir().unwrap().keep();
    let linked_root = tempdir().unwrap().keep();

    assert!(Command::new("git")
        .args(["init", "-q"])
        .arg(&primary)
        .status()
        .unwrap()
        .success());

    let cfg = primary.join("pkg/sub/.ironlint.yml");
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    std::fs::write(
        &cfg,
        "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \".ironlint/scripts/g.sh\"\n",
    )
    .unwrap();

    let scripts = cfg.parent().unwrap().join(".ironlint/scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    let script = scripts.join("g.sh");
    std::fs::write(&script, "#!/bin/sh\nexit 1\n").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(&primary)
        .status()
        .unwrap();
    Command::new("git")
        .args([
            "-c",
            "user.email=t@t.co",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ])
        .current_dir(&primary)
        .status()
        .unwrap();

    let linked_wt = linked_root.join("wt");
    assert!(Command::new("git")
        .args(["worktree", "add", "-q"])
        .arg(&linked_wt)
        .current_dir(&primary)
        .status()
        .unwrap()
        .success());

    let linked_cfg = linked_wt.join("pkg/sub/.ironlint.yml");
    let store = tempdir().unwrap();
    let store_path = store.path().join("trust.json");
    (cfg, linked_cfg, store_path, script)
}

/// Non-Git project with a config + script. Returns (cfg, store_path, script).
fn non_git_setup() -> (PathBuf, PathBuf, PathBuf) {
    let dir = tempdir().unwrap().keep();
    let cfg = dir.join(".ironlint.yml");
    std::fs::write(
        &cfg,
        "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \".ironlint/scripts/g.sh\"\n",
    )
    .unwrap();
    let scripts = dir.join(".ironlint/scripts");
    std::fs::create_dir_all(&scripts).unwrap();
    let script = scripts.join("g.sh");
    std::fs::write(&script, "#!/bin/sh\nexit 1\n").unwrap();
    let store = tempdir().unwrap();
    let store_path = store.path().join("trust.json");
    (cfg, store_path, script)
}

#[test]
fn blessed_primary_makes_unchanged_sibling_trusted() {
    if !git_available() {
        eprintln!("skip: git not on PATH");
        return;
    }
    let (primary_cfg, linked_cfg, store_path, _script) = worktree_pair();
    bless_in(&primary_cfg, &store_path, "t").unwrap();
    // The sibling has a different canonical path -> direct miss -> worktree hit.
    assert!(
        matches!(
            check_trust_in(&linked_cfg, &store_path),
            TrustOutcome::Trusted(_)
        ),
        "unchanged sibling of a blessed primary must be trusted via inheritance"
    );
}

#[test]
fn editing_sibling_config_revokes_trust() {
    if !git_available() {
        eprintln!("skip: git not on PATH");
        return;
    }
    let (primary_cfg, linked_cfg, store_path, _script) = worktree_pair();
    bless_in(&primary_cfg, &store_path, "t").unwrap();
    std::fs::write(
        &linked_cfg,
        "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
    )
    .unwrap();
    assert!(
        matches!(
            check_trust_in(&linked_cfg, &store_path),
            TrustOutcome::Untrusted(_)
        ),
        "a changed sibling config must not inherit trust"
    );
}

#[test]
fn editing_sibling_script_revokes_trust() {
    if !git_available() {
        eprintln!("skip: git not on PATH");
        return;
    }
    let (primary_cfg, linked_cfg, store_path, _script) = worktree_pair();
    bless_in(&primary_cfg, &store_path, "t").unwrap();
    // The sibling's script lives under the linked worktree's own
    // .ironlint/scripts/ (git worktree adds a working tree copy).
    let sibling_script = linked_cfg.parent().unwrap().join(".ironlint/scripts/g.sh");
    std::fs::write(&sibling_script, "#!/bin/sh\nexit 0\n").unwrap();
    assert!(
        matches!(
            check_trust_in(&linked_cfg, &store_path),
            TrustOutcome::Untrusted(_)
        ),
        "a changed sibling script must not inherit trust"
    );
}

#[test]
fn separate_clone_with_identical_policy_does_not_inherit() {
    if !git_available() {
        eprintln!("skip: git not on PATH");
        return;
    }
    let (primary_cfg, _linked_cfg, store_path, _script) = worktree_pair();
    bless_in(&primary_cfg, &store_path, "t").unwrap();
    // A completely separate git repo with byte-identical policy -> different
    // common dir -> no inheritance.
    let other = tempdir().unwrap();
    Command::new("git")
        .args(["init", "-q"])
        .arg(other.path())
        .status()
        .unwrap();
    let other_cfg = other.path().join(".ironlint.yml");
    std::fs::write(
        &other_cfg,
        "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \".ironlint/scripts/g.sh\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(other.path().join(".ironlint/scripts")).unwrap();
    std::fs::write(
        other.path().join(".ironlint/scripts/g.sh"),
        "#!/bin/sh\nexit 1\n",
    )
    .unwrap();
    assert!(
        matches!(
            check_trust_in(&other_cfg, &store_path),
            TrustOutcome::Untrusted(_)
        ),
        "a separate clone must not inherit trust"
    );
}

#[test]
fn non_git_project_uses_direct_trust() {
    // Spec item 5: without Git metadata the policy is ineligible for inheritance
    // but exact-path direct trust still works.
    let (cfg, store_path, _script) = non_git_setup();
    bless_in(&cfg, &store_path, "t").unwrap();
    assert!(
        matches!(check_trust_in(&cfg, &store_path), TrustOutcome::Trusted(_)),
        "a non-Git config must be trusted via its direct entry"
    );

    // Editing the config revokes the direct entry.
    std::fs::write(
        &cfg,
        "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
    )
    .unwrap();
    assert!(
        matches!(
            check_trust_in(&cfg, &store_path),
            TrustOutcome::Untrusted(_)
        ),
        "a changed non-Git config must lose direct trust"
    );
}

/// RAII guard that restores the original working directory on drop.
struct CdGuard(PathBuf);

impl Drop for CdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

/// Serialize tests that mutate the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn nested_config_sibling_trusted_and_scope_not_from_cwd() {
    // Spec item 7: scope identity comes from the config's Git worktree (and
    // the nested config_rel), never from the caller's current directory.
    if !git_available() {
        eprintln!("skip: git not on PATH");
        return;
    }
    let (primary_cfg, linked_cfg, store_path, _script) = nested_worktree_pair();
    bless_in(&primary_cfg, &store_path, "t").unwrap();

    assert!(
        matches!(
            check_trust_in(&linked_cfg, &store_path),
            TrustOutcome::Trusted(_)
        ),
        "a sibling with the same nested config path must inherit trust"
    );

    // Repeat from a completely unrelated cwd to prove scope is not cwd-derived.
    let _guard = CWD_LOCK.lock().unwrap();
    let original = std::env::current_dir().unwrap();
    let cd_guard = CdGuard(original);
    let unrelated = tempdir().unwrap();
    std::env::set_current_dir(unrelated.path()).unwrap();

    assert!(
        matches!(
            check_trust_in(&linked_cfg, &store_path),
            TrustOutcome::Trusted(_)
        ),
        "trust scope must derive from the config path, not the current directory"
    );

    // `cd_guard` restores the original cwd when it drops.
    drop(cd_guard);
}
