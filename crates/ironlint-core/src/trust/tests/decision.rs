use super::*;
use crate::trust::policy_hash::compute_worktree_hash;
use crate::trust::store::canonical_key;
use crate::trust::worktree::WorktreeScope;
use std::fs;
use std::path::{Path, PathBuf};
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

fn cfg_with_script(dir: &Path) -> PathBuf {
    let cfg = dir.join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*\"\n    run: \".ironlint/scripts/g.sh\"\n",
    );
    write(&dir.join(".ironlint/scripts/g.sh"), "#!/bin/sh\nexit 0\n");
    cfg
}

/// Build a real git repo at `root` so `WorktreeScope::discover` succeeds.
fn git_repo(root: &Path) {
    fs::create_dir_all(root).unwrap();
    let _ = Command::new("git").args(["init", "-q"]).arg(root).status();
    // .ironlint.yml is the trust surface; commit is unnecessary for discovery.
}

#[test]
fn bless_then_ensure_succeeds() {
    let proj = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let store_path = store.path().join("trust.json");
    let cfg = cfg_with_script(proj.path());
    bless_in(&cfg, &store_path, "2026-06-24T00:00:00Z").unwrap();
    assert!(ensure_trusted_in(&cfg, &store_path).is_ok());
}

#[test]
fn never_blessed_is_not_trusted() {
    let proj = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let cfg = cfg_with_script(proj.path());
    let err = ensure_trusted_in(&cfg, &store.path().join("trust.json"))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("not trusted"),
        "message must say not trusted: {err}"
    );
    assert!(
        err.contains("ironlint trust"),
        "message must point at `ironlint trust`: {err}"
    );
}

#[test]
fn editing_a_script_after_bless_revokes_trust() {
    let proj = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let store_path = store.path().join("trust.json");
    let cfg = cfg_with_script(proj.path());
    bless_in(&cfg, &store_path, "t").unwrap();
    // Tamper with the script.
    write(
        &proj.path().join(".ironlint/scripts/g.sh"),
        "#!/bin/sh\nexit 2\n",
    );
    assert!(ensure_trusted_in(&cfg, &store_path).is_err());
}

#[test]
fn editing_config_after_bless_revokes_trust() {
    let proj = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let store_path = store.path().join("trust.json");
    let cfg = cfg_with_script(proj.path());
    bless_in(&cfg, &store_path, "t").unwrap();
    write(&cfg, "checks:\n  g:\n    files: \"*\"\n    run: \"true\"\n");
    assert!(ensure_trusted_in(&cfg, &store_path).is_err());
}

#[test]
fn bless_rejects_unparseable_config() {
    let proj = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let cfg = proj.path().join(".ironlint.yml");
    write(&cfg, "schema_version: 2\nrules: {}\n"); // legacy → parser rejects
    assert!(bless_in(&cfg, &store.path().join("trust.json"), "t").is_err());
}

#[test]
fn concurrent_blesses_do_not_lose_entries() {
    // Today, bless_in is an unlocked read-modify-write with a fixed temp
    // filename: N threads blessing distinct configs into the same store
    // race each other and lose entries. Line all N up on a barrier so
    // they hit the RMW at (as close to) the same instant as possible,
    // then assert every entry survived.
    const N: usize = 8;
    let store_dir = tempfile::tempdir().unwrap();
    let store_path = store_dir.path().join("trust.json");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(N));

    let handles: Vec<_> = (0..N)
        .map(|_| {
            let store_path = store_path.clone();
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                let proj = tempfile::tempdir().unwrap();
                let cfg = cfg_with_script(proj.path());
                barrier.wait();
                bless_in(&cfg, &store_path, "t").unwrap();
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let store = read_store(&store_path).unwrap();
    assert_eq!(
        store.entries.len(),
        N,
        "concurrent blesses must not lose entries"
    );
}

#[test]
fn bless_recovers_from_corrupt_store() {
    // A corrupt/half-written store must not brick `trust` — bless_in
    // should treat unparseable existing content as empty, warn, and
    // rewrite with the new entry rather than erroring out.
    let proj = tempfile::tempdir().unwrap();
    let store_dir = tempfile::tempdir().unwrap();
    let store_path = store_dir.path().join("trust.json");
    write(&store_path, "{ not json");
    let cfg = cfg_with_script(proj.path());

    bless_in(&cfg, &store_path, "t").unwrap();

    let store = read_store(&store_path).unwrap();
    let key = canonical_key(&cfg.canonicalize().unwrap());
    assert!(
        store.entries.contains_key(&key),
        "bless must recover from a corrupt store and record the new entry"
    );
}

#[test]
fn ensure_trusted_fails_closed_on_corrupt_store() {
    // Unlike bless_in, ensure_trusted must NOT tolerate corruption — a
    // corrupt store must keep `check` failing closed.
    let proj = tempfile::tempdir().unwrap();
    let store_dir = tempfile::tempdir().unwrap();
    let store_path = store_dir.path().join("trust.json");
    write(&store_path, "{ not json");
    let cfg = cfg_with_script(proj.path());

    assert!(ensure_trusted_in(&cfg, &store_path).is_err());
}

#[cfg(unix)]
#[test]
fn bless_in_fails_closed_when_store_unreadable_for_non_parse_reasons() {
    // read_store_for_bless must only tolerate a PARSE error (corrupt
    // content) as "empty store". A read that fails for any other reason
    // (permission denied, transient I/O) must propagate as Err rather
    // than being treated as an empty store — otherwise bless_in would
    // silently overwrite a real, non-empty store with just the one new
    // entry (rename only needs directory write permission, not read
    // permission on the target file, so the clobber would succeed).
    use std::os::unix::fs::PermissionsExt;

    let store_dir = tempfile::tempdir().unwrap();
    let store_path = store_dir.path().join("trust.json");

    // Seed a real, non-empty store first.
    let seed_proj = tempfile::tempdir().unwrap();
    let seed_cfg = cfg_with_script(seed_proj.path());
    bless_in(&seed_cfg, &store_path, "t").unwrap();
    let before = fs::read(&store_path).unwrap();
    assert!(!before.is_empty());

    // Make the store file unreadable.
    let mut perms = fs::metadata(&store_path).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&store_path, perms).unwrap();

    // Root (and some sandboxed/CI environments) bypass file-mode
    // permission checks entirely — skip rather than pass vacuously.
    if std::fs::read_to_string(&store_path).is_ok() {
        let mut perms = fs::metadata(&store_path).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&store_path, perms).unwrap();
        eprintln!(
            "skipping bless_in_fails_closed_when_store_unreadable_for_non_parse_reasons: \
             running with privileges that bypass file-mode permissions"
        );
        return;
    }

    let proj = tempfile::tempdir().unwrap();
    let cfg = cfg_with_script(proj.path());
    let result = bless_in(&cfg, &store_path, "t2");

    // Restore perms so tempdir cleanup can remove the file regardless of
    // assertion outcome below.
    let mut perms = fs::metadata(&store_path).unwrap().permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&store_path, perms).unwrap();

    assert!(
        result.is_err(),
        "bless_in must fail loudly on a non-parse read error, not silently clobber"
    );
    let after = fs::read(&store_path).unwrap();
    assert_eq!(
        before, after,
        "store bytes must be unchanged after a failed bless"
    );
}

// --- blessing worktree entry (Task 4) --------------------------------
#[test]
fn bless_writes_worktree_entry_for_eligible_git_repo() {
    let root = tempfile::tempdir().unwrap();
    git_repo(root.path());
    let cfg = root.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
    );
    let store = tempfile::tempdir().unwrap();
    let store_path = store.path().join("trust.json");
    bless_in(&cfg, &store_path, "t").unwrap();
    let s = read_store(&store_path).unwrap();
    let scope = WorktreeScope::discover(&cfg).unwrap();
    let inner = s
        .worktree_entries
        .get(&scope.common_dir.to_string_lossy().to_string())
        .and_then(|m| m.get(&scope.config_rel))
        .expect("eligible bless writes a worktree_entries entry");
    let h = compute_worktree_hash(&cfg, &scope).unwrap().unwrap();
    assert_eq!(inner.hash, h);
}

#[test]
fn bless_skips_worktree_entry_when_not_a_git_repo() {
    // No .git -> discover returns None -> only the direct entry is written.
    let root = tempfile::tempdir().unwrap();
    let cfg = root.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
    );
    let store = tempfile::tempdir().unwrap();
    let store_path = store.path().join("trust.json");
    bless_in(&cfg, &store_path, "t").unwrap();
    let s = read_store(&store_path).unwrap();
    assert!(
        s.worktree_entries.is_empty(),
        "non-Git config writes no worktree entry"
    );
    assert_eq!(s.entries.len(), 1, "direct entry still written");
}

// --- inherited lookup --------------------------------------------------
#[test]
fn direct_miss_with_matching_worktree_entry_is_trusted() {
    // Bless primary; the SAME store has no direct entry for a sibling's
    // canonical path, but the worktree_entries entry matches -> Trusted.
    let root = tempfile::tempdir().unwrap();
    git_repo(root.path());
    let cfg = root.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:
  g:
    files: \"*.rs\"
    run: \"true\"
",
    );
    let store = tempfile::tempdir().unwrap();
    let store_path = store.path().join("trust.json");
    bless_in(&cfg, &store_path, "t").unwrap();
    // Synthesize a "sibling" by removing the direct entry but KEEPING the
    // worktree entry — simulates a linked worktree with its own canonical path.
    let mut s = read_store(&store_path).unwrap();
    let key = canonical_key(&cfg.canonicalize().unwrap());
    s.entries.remove(&key);
    write_store(&store_path, &s).unwrap();
    // Direct miss, worktree hit:
    assert!(matches!(
        check_trust_in(&cfg, &store_path),
        TrustOutcome::Trusted(_)
    ));
}

#[test]
fn non_git_repo_falls_through_to_untrusted_when_not_blessed() {
    let root = tempfile::tempdir().unwrap();
    let cfg = root.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:
  g:
    files: \"*.rs\"
    run: \"true\"
",
    );
    let store = tempfile::tempdir().unwrap();
    let store_path = store.path().join("trust.json");
    // Never blessed -> untrusted; no inheritance path available.
    assert!(matches!(
        check_trust_in(&cfg, &store_path),
        TrustOutcome::Untrusted(_)
    ));
}

#[test]
fn check_never_writes_store_on_inherited_trust() {
    let root = tempfile::tempdir().unwrap();
    git_repo(root.path());
    let cfg = root.path().join(".ironlint.yml");
    write(
        &cfg,
        "checks:
  g:
    files: \"*.rs\"
    run: \"true\"
",
    );
    let store = tempfile::tempdir().unwrap();
    let store_path = store.path().join("trust.json");
    bless_in(&cfg, &store_path, "t").unwrap();
    // Force a direct miss + worktree hit (remove direct entry):
    let mut s = read_store(&store_path).unwrap();
    let key = canonical_key(&cfg.canonicalize().unwrap());
    s.entries.remove(&key);
    write_store(&store_path, &s).unwrap();
    // Baseline after setup; check must not mutate the store.
    let baseline = std::fs::read(&store_path).unwrap();
    let _ = check_trust_in(&cfg, &store_path);
    assert_eq!(
        baseline,
        std::fs::read(&store_path).unwrap(),
        "store unchanged by check"
    );
}

/// Consent recorded by `init` must cover the bytes it classified, never a
/// second read of the path: a writer that rewrites the config in between must
/// leave the live file untrusted instead of silently approved.
#[test]
fn bless_bytes_binds_consent_to_the_supplied_bytes_not_a_later_rewrite() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    let payload = "version: 1\nchecks:\n  pwn:\n    files: \"*\"\n    run: \"echo pwned\"\n";
    // The live file holds the racing writer's content; consent covers the
    // baseline bytes the caller classified before that write landed.
    write(&cfg, payload);
    let store = dir.path().join("trust.json");
    let baseline = "version: 1\nchecks:\n  g:\n    files: \"*\"\n    run: \"true\"\n";

    bless_bytes_in(&cfg, baseline.as_bytes(), &store, "2026-01-01T00:00:00Z").unwrap();

    match check_trust_in(&cfg, &store) {
        TrustOutcome::Untrusted(_) => {}
        TrustOutcome::Trusted(_) => panic!("consent must not cover rewritten content"),
        TrustOutcome::Unverifiable(e) => panic!("live policy should still be verifiable: {e:#}"),
    }
}

#[test]
fn bless_bytes_never_approves_a_later_live_policy_through_worktree_inheritance() {
    let dir = tempfile::tempdir().unwrap();
    // Worktree discovery is filesystem-only; no Git process or live config is
    // required to model the primary worktree metadata.
    fs::create_dir(dir.path().join(".git")).unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    let approved = b"version: 1\nchecks:\n  approved: {run: 'true'}\n";
    let replacement = b"version: 1\nchecks:\n  replacement: {run: 'true'}\n";
    fs::write(&cfg, replacement).unwrap();
    let store = dir.path().join("trust.json");
    bless_bytes_in(&cfg, approved, &store, "t").unwrap();
    match check_trust_in(&cfg, &store) {
        TrustOutcome::Untrusted(_) => {}
        TrustOutcome::Trusted(_) => {
            panic!("worktree fallback approved bytes the caller never supplied")
        }
        TrustOutcome::Unverifiable(error) => panic!("replacement should be verifiable: {error:#}"),
    }
    fs::write(&cfg, approved).unwrap();
    assert!(matches!(
        check_trust_in(&cfg, &store),
        TrustOutcome::Trusted(_)
    ));
}

#[test]
fn inherited_lookup_uses_captured_identity_and_evaluation_still_detects_drift() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    let config = dir.path().join(".ironlint.yml");
    let store_path = dir.path().join("trust.json");
    let original = b"version: 1\nchecks:\n  original: {run: 'true'}\n";
    fs::write(&config, original).unwrap();
    bless_in(&config, &store_path, "t").unwrap();
    let captured = PolicySnapshot::load(&config).unwrap();
    let store = read_store(&store_path).unwrap();
    fs::write(&config, "version: 1\nchecks:\n  changed: {run: 'true'}\n").unwrap();
    assert!(inherited_trusted(&captured, &store));
    assert_eq!(captured.policy_bytes(), original);
    assert!(captured.verify_unchanged().is_err());
    let replacement = PolicySnapshot::load(&config).unwrap();
    assert!(!inherited_trusted(&replacement, &store));
}

/// The two bless entry points must record the same digest for identical
/// content, so a policy blessed by `init` verifies under the path-based hash.
#[test]
fn bless_bytes_and_bless_record_the_same_hash() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = cfg_with_script(dir.path());
    let bytes = fs::read(&cfg).unwrap();
    let byte_store = dir.path().join("byte.json");
    let path_store = dir.path().join("path.json");

    bless_bytes_in(&cfg, &bytes, &byte_store, "t").unwrap();
    bless_in(&cfg, &path_store, "t").unwrap();

    let key = canonical_key(&cfg.canonicalize().unwrap());
    let byte_hash = read_store(&byte_store).unwrap().entries[&key].hash.clone();
    let path_hash = read_store(&path_store).unwrap().entries[&key].hash.clone();
    assert_eq!(byte_hash, path_hash);
}
