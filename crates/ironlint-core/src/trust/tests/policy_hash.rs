use super::*;
use std::fs;
use std::path::Path;
use std::process::Command;

fn write(p: &Path, body: &str) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    if matches!(
        p.extension().and_then(|ext| ext.to_str()),
        Some("yml" | "yaml")
    ) && !body.starts_with("version:")
    {
        fs::write(p, format!("version: 1\n{body}")).unwrap();
    } else {
        fs::write(p, body).unwrap();
    }
}

/// Build a real git repo at `root` so `WorktreeScope::discover` succeeds.
fn git_repo(root: &Path) {
    fs::create_dir_all(root).unwrap();
    let _ = Command::new("git").args(["init", "-q"]).arg(root).status();
    // .ironlint.yml is the trust surface; commit is unnecessary for discovery.
}

#[test]
fn hash_is_deterministic_and_prefixed() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
    );
    let a = compute_hash(&cfg).unwrap();
    let b = compute_hash(&cfg).unwrap();
    assert_eq!(a, b, "same inputs must hash identically");
    assert!(
        a.starts_with("sha256:"),
        "hash must be sha256-prefixed: {a}"
    );
}

#[test]
fn editing_config_changes_hash() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
    );
    let before = compute_hash(&cfg).unwrap();
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \"false\"\n",
    );
    let after = compute_hash(&cfg).unwrap();
    assert_ne!(before, after, "a config edit must invalidate the hash");
}

#[test]
fn editing_a_script_changes_hash() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \".ironlint/scripts/g.sh\"\n",
    );
    let script = dir.path().join(".ironlint/scripts/g.sh");
    write(&script, "#!/bin/sh\nexit 0\n");
    let before = compute_hash(&cfg).unwrap();
    write(&script, "#!/bin/sh\nexit 2\n");
    let after = compute_hash(&cfg).unwrap();
    assert_ne!(before, after, "a script edit must invalidate the hash");
}

#[test]
fn hash_folds_scripts_in_sorted_order() {
    // compute_hash must fold the config plus its script files in a
    // deterministic, identity-bound frame: each config keyed by its
    // canonical path, each script file keyed by its scripts dir + sorted
    // relative path (independent of OS enumeration order). We pin the exact
    // scheme by recomputing the digest the same way using the impl's own
    // framing helper. This fails if the impl stops sorting script files (the
    // `out.sort_by` in collect_gate_files) — on a filesystem whose read_dir
    // yields b before a — or if the label binding / length prefixes change,
    // which doubles as a regression lock on the stored-hash encoding.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    let cfg_body = "version: 1\nchecks:\n  g:\n    files: \"*\"\n    run: \"true\"\n";
    write(&cfg, cfg_body);
    write(&dir.path().join(".ironlint/scripts/a.sh"), "a\n");
    write(&dir.path().join(".ironlint/scripts/b.sh"), "b\n");

    // Labels carry canonical paths, so recompute against the canonical form.
    let canon = cfg.canonicalize().unwrap();
    let scripts_dir = canon.parent().unwrap().join(".ironlint").join("scripts");

    let mut expected = Sha256::new();
    hash_entry(
        &mut expected,
        &format!("config\0{}", canon.display()),
        cfg_body.as_bytes(),
    );
    hash_entry(
        &mut expected,
        &format!("scripts\0{}\0a.sh", scripts_dir.display()),
        b"a\n",
    );
    hash_entry(
        &mut expected,
        &format!("scripts\0{}\0b.sh", scripts_dir.display()),
        b"b\n",
    );
    let want = sha256_digest_hex(&expected.finalize());

    assert_eq!(compute_hash(&cfg).unwrap(), want);
}

#[test]
fn editing_a_referenced_outside_script_does_not_change_hash() {
    // After the gates→scripts rename: a script referenced by `run:` but
    // located OUTSIDE .ironlint/scripts/ is no longer folded into the
    // trust hash. It may still be run by a check, but changing it does
    // not revoke trust — the spec's deliberate simplification.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*\"\n    run: \"bash scripts/lint.sh\"\n",
    );
    let script = dir.path().join("scripts/lint.sh");
    write(&script, "#!/bin/sh\nexit 0\n");
    let before = compute_hash(&cfg).unwrap();
    write(&script, "#!/bin/sh\nexit 2\n");
    let after = compute_hash(&cfg).unwrap();
    assert_eq!(
        before, after,
        "editing an in-repo script OUTSIDE .ironlint/scripts/ no longer revokes trust"
    );
}

#[test]
fn missing_scripts_dir_hashes_only_the_config() {
    // No .ironlint/scripts/ at all — must succeed (not error), hashing config alone.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
    );
    assert!(compute_hash(&cfg).unwrap().starts_with("sha256:"));
}

#[cfg(unix)]
#[test]
fn scripts_dir_symlink_loop_is_a_clear_error_not_a_hang() {
    // A self-referencing symlink inside the scripts dir must not be
    // followed. Before the fix, `is_dir()` follows the symlink and the
    // walk recurses through it; the OS eventually caps total symlink
    // resolutions and returns a raw ELOOP, so this terminates promptly
    // either way — but the *message* must call out the symlink, not
    // leak a raw OS error, and (after the fix) the walk must refuse the
    // symlink on first sight rather than ever recursing into it.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(&cfg, "checks:\n  g:\n    files: \"*\"\n    run: \"true\"\n");
    let scripts = dir.path().join(".ironlint/scripts");
    fs::create_dir_all(&scripts).unwrap();
    write(&scripts.join("g.sh"), "#!/bin/sh\nexit 0\n");
    std::os::unix::fs::symlink(&scripts, scripts.join("loop")).unwrap();

    let err = compute_hash(&cfg).unwrap_err().to_string();
    assert!(
        err.contains("symlink"),
        "error should call out the symlink, not a raw OS ELOOP: {err}"
    );
}

#[cfg(unix)]
#[test]
fn classify_entry_refuses_a_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.txt");
    fs::write(&target, "hi").unwrap();
    let link = dir.path().join("link.txt");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let err = classify_entry(&link).unwrap_err().to_string();
    assert!(err.contains("symlink"), "error: {err}");
}

#[cfg(unix)]
#[test]
fn classify_entry_refuses_a_fifo() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo must be available for this test");
    assert!(status.success(), "mkfifo failed");
    let err = classify_entry(&fifo).unwrap_err().to_string();
    assert!(err.contains("non-regular"), "error: {err}");
}

#[test]
fn classify_entry_missing_path_is_missing_kind() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope");
    assert!(matches!(
        classify_entry(&missing).unwrap(),
        EntryKind::Missing
    ));
}

#[test]
fn classify_entry_dir_and_file_kinds() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir_all(&sub).unwrap();
    let file = dir.path().join("f.txt");
    fs::write(&file, "x").unwrap();
    assert!(matches!(classify_entry(&sub).unwrap(), EntryKind::Dir));
    assert!(matches!(classify_entry(&file).unwrap(), EntryKind::File));
}

#[test]
fn scripts_recurse_into_subdirectories() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(&cfg, "checks:\n  g:\n    files: \"*\"\n    run: \"true\"\n");
    write(&dir.path().join(".ironlint/scripts/top.sh"), "top\n");
    write(
        &dir.path().join(".ironlint/scripts/sub/nested.sh"),
        "nested\n",
    );
    let before = compute_hash(&cfg).unwrap();
    write(
        &dir.path().join(".ironlint/scripts/sub/nested.sh"),
        "nested changed\n",
    );
    let after = compute_hash(&cfg).unwrap();
    assert_ne!(
        before, after,
        "editing a nested script file must change the hash"
    );
}

#[cfg(unix)]
#[test]
fn scripts_dir_itself_as_symlink_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(&cfg, "checks:\n  g:\n    files: \"*\"\n    run: \"true\"\n");
    let real_scripts = dir.path().join("real_scripts");
    fs::create_dir_all(&real_scripts).unwrap();
    write(&real_scripts.join("g.sh"), "#!/bin/sh\nexit 0\n");
    fs::create_dir_all(dir.path().join(".ironlint")).unwrap();
    std::os::unix::fs::symlink(&real_scripts, dir.path().join(".ironlint/scripts")).unwrap();

    let err = compute_hash(&cfg).unwrap_err().to_string();
    assert!(err.contains("symlink"), "error: {err}");
}

#[test]
fn scripts_dir_path_is_a_plain_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(&cfg, "checks:\n  g:\n    files: \"*\"\n    run: \"true\"\n");
    fs::create_dir_all(dir.path().join(".ironlint")).unwrap();
    fs::write(dir.path().join(".ironlint/scripts"), "not a dir").unwrap();
    assert!(compute_hash(&cfg).is_err());
}

#[test]
fn worktree_hash_matches_for_equivalent_roots() {
    // Two tempdirs with identical policy + identical .git dir layout must
    // produce the SAME worktree hash (labels are root-relative).
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    for root in [a.path(), b.path()] {
        git_repo(root);
        let cfg = root.join(".ironlint.yml");
        write(
            &cfg,
            "checks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
        );
    }
    let sa = WorktreeScope::discover(&a.path().join(".ironlint.yml")).unwrap();
    let sb = WorktreeScope::discover(&b.path().join(".ironlint.yml")).unwrap();
    let ha = compute_worktree_hash(&a.path().join(".ironlint.yml"), &sa)
        .unwrap()
        .unwrap();
    let hb = compute_worktree_hash(&b.path().join(".ironlint.yml"), &sb)
        .unwrap()
        .unwrap();
    assert_eq!(ha, hb, "identical policy in equivalent roots hashes alike");
}

#[test]
fn worktree_hash_changes_when_config_changes() {
    let root = tempfile::tempdir().unwrap();
    git_repo(root.path());
    let cfg = root.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
    );
    let scope = WorktreeScope::discover(&cfg).unwrap();
    let h1 = compute_worktree_hash(&cfg, &scope).unwrap().unwrap();
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \"false\"\n",
    );
    let h2 = compute_worktree_hash(&cfg, &scope).unwrap().unwrap();
    assert_ne!(h1, h2, "editing the config changes the worktree hash");
}

#[test]
fn worktree_hash_changes_when_script_changes() {
    let root = tempfile::tempdir().unwrap();
    git_repo(root.path());
    let cfg = root.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \".ironlint/scripts/s.sh\"\n",
    );
    let scripts = root.path().join(".ironlint/scripts");
    fs::create_dir_all(&scripts).unwrap();
    write(&scripts.join("s.sh"), "#!/bin/sh\nexit 0\n");
    let scope = WorktreeScope::discover(&cfg).unwrap();
    let h1 = compute_worktree_hash(&cfg, &scope).unwrap().unwrap();
    write(&scripts.join("s.sh"), "#!/bin/sh\nexit 1\n");
    let h2 = compute_worktree_hash(&cfg, &scope).unwrap().unwrap();
    assert_ne!(h1, h2, "editing a covered script changes the worktree hash");
}

/// The verified snapshot must expose the exact policy bytes the hash covered.
#[test]
fn approved_policy_exposes_approved_bytes_and_verifies_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    let body = "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n";
    write(&cfg, body);
    write(
        &dir.path().join(".ironlint/scripts/g.sh"),
        "#!/bin/sh\nexit 0\n",
    );
    let approved = read_and_hash(&cfg).unwrap();
    assert_eq!(approved.policy_bytes(), body.as_bytes());
    approved.verify_unchanged().unwrap();
}

/// Drift in any covered blob fails closed and names the drifted entry.
#[test]
fn verify_unchanged_detects_modified_added_and_removed_scripts() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
    );
    let scripts = dir.path().join(".ironlint/scripts");
    write(&scripts.join("g.sh"), "#!/bin/sh\nexit 0\n");
    let approved = read_and_hash(&cfg).unwrap();

    write(&scripts.join("g.sh"), "#!/bin/sh\nexit 2\n");
    let modified = approved.verify_unchanged().unwrap_err().to_string();
    assert!(modified.contains("g.sh"), "{modified}");
    assert!(modified.contains("modified or added"), "{modified}");

    fs::remove_file(scripts.join("g.sh")).unwrap();
    let removed = approved.verify_unchanged().unwrap_err().to_string();
    assert!(removed.contains("g.sh"), "{removed}");
    assert!(removed.contains("removed"), "{removed}");

    write(&scripts.join("new.sh"), "#!/bin/sh\nexit 0\n");
    let added = approved.verify_unchanged().unwrap_err().to_string();
    assert!(added.contains("new.sh"), "{added}");
    assert!(added.contains("modified or added"), "{added}");
}

#[test]
fn verify_unchanged_detects_policy_edits() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
    );
    let approved = read_and_hash(&cfg).unwrap();
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \"false\"\n",
    );
    let error = approved.verify_unchanged().unwrap_err().to_string();
    assert!(error.contains("policy"), "{error}");
    assert!(error.contains(".ironlint.yml"), "{error}");
}

/// Unreadable or unparseable live bytes can never be confirmed approved, so
/// verification must fail closed rather than assume no drift.
#[test]
fn verify_unchanged_fails_closed_when_live_policy_no_longer_parses() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
    );
    let approved = read_and_hash(&cfg).unwrap();
    write(&cfg, "checks: {broken");
    let error = approved.verify_unchanged().unwrap_err().to_string();
    assert!(error.contains("could not verify"), "{error}");
}

/// The approved snapshot must not retain managed-script bytes: it is held for
/// the whole run and a fresh live copy is built before every check, so keeping
/// blob content resident would double the footprint of a large scripts tree
/// (and `verify_unchanged` only ever needs the digest comparison).
#[test]
fn approved_snapshot_retains_digests_not_script_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(&cfg, "checks:\n  g:\n    files: \"*\"\n    run: \"true\"\n");
    let big = vec![b'x'; 1 << 20];
    fs::create_dir_all(dir.path().join(".ironlint/scripts")).unwrap();
    fs::write(dir.path().join(".ironlint/scripts/big.sh"), &big).unwrap();

    let approved = read_and_hash(&cfg).unwrap();
    assert!(
        approved.retained_blob_bytes() < big.len(),
        "snapshot retained {} blob bytes for a {} byte script",
        approved.retained_blob_bytes(),
        big.len()
    );
}

/// The byte-bound hash used by `init` must fold identically to the path-based
/// hash, or blessing classified bytes would record a digest `check` can never
/// reproduce.
#[test]
fn byte_bound_hash_matches_path_hash_for_the_same_content() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*\"\n    run: \".ironlint/scripts/g.sh\"\n",
    );
    write(
        &dir.path().join(".ironlint/scripts/g.sh"),
        "#!/bin/sh\nexit 0\n",
    );
    let bytes = fs::read(&cfg).unwrap();
    assert_eq!(
        compute_hash(&cfg).unwrap(),
        hash_policy_bytes(&cfg, &bytes).unwrap(),
        "path-based and byte-bound hashing must agree for identical content"
    );
}
