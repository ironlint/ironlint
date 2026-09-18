use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

/// `ironlint trust` writes a blessed entry into the XDG-redirected store.
#[test]
fn trust_writes_a_store_entry() {
    let proj = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let cfg = proj.path().join(".ironlint.yml");
    fs::write(
        &cfg,
        "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
    )
    .unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["trust", "--config"])
        .arg(&cfg)
        .assert()
        .success();

    let store = xdg.path().join("ironlint/trust.json");
    assert!(store.exists(), "trust must create the store file");
    let body = fs::read_to_string(&store).unwrap();
    assert!(body.contains("sha256:"), "store must hold a hash: {body}");
}

/// Task 5.31: `ironlint trust` prints a summary of exactly what it blessed —
/// the config hash (first 16 hex chars) and every script file under
/// `.ironlint/scripts/` — so the operator can eyeball trust coverage instead
/// of taking it on faith. Commands that reference files outside
/// `.ironlint/scripts/` do not add those files to the trust surface;
/// see `editing_a_referenced_outside_script_does_not_change_hash`.
#[test]
fn trust_prints_blessed_summary() {
    let proj = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let cfg = proj.path().join(".ironlint.yml");
    fs::write(
        &cfg,
        "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \".ironlint/scripts/lint.sh\"\n",
    )
    .unwrap();
    let scripts = proj.path().join(".ironlint/scripts");
    fs::create_dir_all(&scripts).unwrap();
    fs::write(scripts.join("lint.sh"), "#!/bin/sh\nexit 0\n").unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["trust", "--config"])
        .arg(&cfg)
        .assert()
        .success()
        .stdout(
            predicates::str::contains("config sha256:")
                .and(predicates::str::contains("checks: 1"))
                .and(predicates::str::contains("scripts: 1"))
                .and(predicates::str::contains("lint.sh")),
        );
}

/// Sibling guard: with no scripts dir and no referenced scripts, the summary
/// still prints `scripts: 0` (the scripts block is always shown).
#[test]
fn trust_summary_prints_zero_scripts_when_empty() {
    let proj = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let cfg = proj.path().join(".ironlint.yml");
    fs::write(
        &cfg,
        "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \"true\"\n",
    )
    .unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["trust", "--config"])
        .arg(&cfg)
        .assert()
        .success()
        .stdout(
            predicates::str::contains("checks: 1").and(predicates::str::contains("scripts: 0")),
        );
}

/// Blessing a config that does not parse fails (exit 1), writes nothing, and
/// speaks the one error voice (T1): a lowercase `error:` line on stderr, not a
/// raw `Error: <debug>` anyhow chain leaked through `?` — matching
/// explain/show-resolved-config/check.
#[test]
fn trust_rejects_unparseable_config() {
    let proj = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let cfg = proj.path().join(".ironlint.yml");
    fs::write(&cfg, "schema_version: 2\nrules: {}\n").unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["trust", "--config"])
        .arg(&cfg)
        .assert()
        .failure()
        .code(1)
        .stderr(predicates::str::starts_with("error: "));

    // The spec's other half: a rejected config must write nothing to the store.
    let store = xdg.path().join("ironlint/trust.json");
    assert!(
        !store.exists(),
        "bless must not write the store on parse failure: {store:?}"
    );
}

/// An unblessed (but well-formed, parseable) config makes `check` fail
/// closed with exit **4** — its own code, distinct from exit 1 (config/parse
/// error). Before Task 3.2 this was exit 1, the same code a parse error
/// uses, so an adapter mapping exit 1 -> allow would silently un-gate every
/// edit for a config nobody ever blessed. See `parse_error_config_check_exits_1`
/// below for the sibling guard that a genuine parse error keeps exit 1.
#[test]
fn unblessed_config_check_exits_4() {
    let proj = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let cfg = proj.path().join(".ironlint.yml");
    fs::write(
        &cfg,
        "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \"exit 0\"\n",
    )
    .unwrap();
    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["check", "--config"])
        .arg(&cfg)
        .assert()
        .failure()
        .code(4)
        .stderr(predicates::str::contains("not trusted"));
}

/// Sibling guard for `unblessed_config_check_exits_4`: a config that fails to
/// **parse** must keep exit **1**, not collapse into
/// the untrusted-config exit 4. It can never even be blessed (`ironlint
/// trust` itself refuses to bless anything that doesn't parse — see
/// `trust_rejects_unparseable_config` above), so this hits `check` directly:
/// the trust layer can't compute a hash over content that doesn't parse, and
/// that failure is a structural config problem, not a "this config was never
/// reviewed" problem — it must surface through the same exit code a load
/// failure would use.
#[test]
fn parse_error_config_check_exits_1() {
    let proj = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let cfg = proj.path().join(".ironlint.yml");
    fs::write(&cfg, "schema_version: 2\nrules: {}\n").unwrap();
    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["check", "--config"])
        .arg(&cfg)
        .assert()
        .failure()
        .code(1);
}

/// After `trust`, `check` admits the config and actually runs its checks — not
/// a vacuous exit 0. A blocking check yields exit 2, which is only reachable if
/// trust passed AND the check executed to its verdict (an untrusted config would
/// exit 1; a config whose check never ran would exit 0).
#[test]
fn blessed_config_check_runs() {
    let proj = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let cfg = proj.path().join(".ironlint.yml");
    fs::write(
        &cfg,
        "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \"exit 2\"\n",
    )
    .unwrap();
    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["trust", "--config"])
        .arg(&cfg)
        .assert()
        .success();

    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["check", "--config"])
        .arg(&cfg)
        .assert()
        .failure()
        .code(2);
}

/// Editing a check script after blessing revokes trust → check exits 4 (the
/// blessed hash no longer matches, so this is the untrusted/mismatch case,
/// not a parse error).
#[test]
fn editing_check_after_bless_blocks_check() {
    let proj = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let cfg = proj.path().join(".ironlint.yml");
    fs::write(
        &cfg,
        "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \".ironlint/scripts/g.sh\"\n",
    )
    .unwrap();
    let scripts = proj.path().join(".ironlint/scripts");
    fs::create_dir_all(&scripts).unwrap();
    fs::write(scripts.join("g.sh"), "#!/bin/sh\nexit 0\n").unwrap();
    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["trust", "--config"])
        .arg(&cfg)
        .assert()
        .success();

    fs::write(scripts.join("g.sh"), "#!/bin/sh\nexit 2\n").unwrap(); // tamper

    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["check", "--config"])
        .arg(&cfg)
        .assert()
        .failure()
        .code(4)
        .stderr(predicates::str::contains("not trusted"));
}

/// A script referenced by `run:` but located
/// OUTSIDE `.ironlint/scripts/` is no longer part of the trust surface, so it
/// must NOT appear in the blessed summary. (The hash-level guarantee — that
/// editing such a script does not revoke trust — is pinned at the unit level
/// by `editing_a_referenced_outside_script_does_not_change_hash` in
/// ironlint-core; this test pins the user-visible CLI summary, which that
/// unit test cannot reach.) This keeps the hash surface equal to the
/// summary must not imply the hash covers it either.
#[test]
fn out_of_dir_referenced_script_is_absent_from_summary() {
    let proj = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let cfg = proj.path().join(".ironlint.yml");
    // The check references a script at the repo root — OUTSIDE .ironlint/scripts/.
    fs::write(
        &cfg,
        "version: 1\nchecks:\n  g:\n    files: \"*.rs\"\n    run: \"./lint.sh\"\n",
    )
    .unwrap();
    fs::write(proj.path().join("lint.sh"), "#!/bin/sh\nexit 0\n").unwrap();

    // The summary must report scripts: 0 and must NOT list the out-of-dir
    // script — only files under .ironlint/scripts/ are summarized.
    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["trust", "--config"])
        .arg(&cfg)
        .assert()
        .success()
        .stdout(
            predicates::str::contains("checks: 1")
                .and(predicates::str::contains("scripts: 0"))
                .and(predicates::str::contains("lint.sh").not()),
        );
}

/// SECURITY REGRESSION (v1.0.0 release blocker): trust is verified once over
/// the policy plus every file under `.ironlint/scripts/`, but each check runs
/// `sh -c` against the LIVE filesystem. A check executed earlier in the same
/// acceptance pass can therefore rewrite a managed script, and a later check
/// executes bytes that were never approved. `c-restore` puts the original
/// bytes back before the process exits, so a hash-before/hash-after comparison
/// also passes. The only acceptable outcomes are that the approved bytes
/// execute or the run fails closed; unapproved bytes must never execute.
#[test]
fn script_mutated_between_trust_and_execution_never_executes() {
    let proj = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let marker_dir = tempfile::tempdir().unwrap();
    let marker = marker_dir.path().join("mutated-ran");
    let marker_str = marker.to_str().unwrap();
    let root = proj.path();
    let cfg = root.join(".ironlint.yml");

    fs::write(
        &cfg,
        format!(
            r#"version: 1
checks:
  a-rewrite:
    on: [accept]
    run: |
      cat > "$IRONLINT_ROOT/.ironlint/scripts/gate.sh" <<'EOF'
      #!/bin/sh
      echo mutated > '{marker_str}'
      exit 0
      EOF
  b-gate:
    on: [accept]
    run: |
      sh "$IRONLINT_ROOT/.ironlint/scripts/gate.sh"
  c-restore:
    on: [accept]
    run: |
      cat > "$IRONLINT_ROOT/.ironlint/scripts/gate.sh" <<'EOF'
      #!/bin/sh
      exit 0
      EOF
"#
        ),
    )
    .unwrap();
    let scripts = root.join(".ironlint/scripts");
    fs::create_dir_all(&scripts).unwrap();
    fs::write(scripts.join("gate.sh"), "#!/bin/sh\nexit 0\n").unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["trust", "--config"])
        .arg(&cfg)
        .assert()
        .success();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            root.to_str().unwrap(),
            "--event",
            "accept",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    // The fix binds execution fail-closed: the drifted run stops before
    // `b-gate`, reports the incomplete verdict, and never passes.
    assert_eq!(
        output.status.code(),
        Some(3),
        "drifted run must exit 3 (incomplete); stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 7);
    assert_eq!(value["status"], "error");
    assert_eq!(value["not_run"][0]["id"], "b-gate");
    assert_eq!(value["not_run"][0]["reason"], "policy_changed");
    assert!(
        !marker.exists(),
        "a check executed .ironlint/scripts bytes that were never approved; \
         exit={:?} stdout={} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
