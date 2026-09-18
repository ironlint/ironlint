use assert_cmd::Command;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn run_init(dir: &Path) {
    let xdg = tempdir().unwrap();
    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["init", "--dir", dir.to_str().unwrap()])
        .assert()
        .success();
}

fn read_cfg(dir: &Path) -> String {
    fs::read_to_string(dir.join(".ironlint.yml")).unwrap()
}

#[test]
fn init_scaffolds_checks_config_not_rules() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"foo\"\n").unwrap();
    run_init(dir.path());
    let cfg = read_cfg(dir.path());
    assert!(
        cfg.starts_with("version: 1\n"),
        "v1 config must start with `version: 1`:\n{cfg}"
    );
    assert!(
        !cfg.contains("schema_version"),
        "checks model must not emit schema_version:\n{cfg}"
    );
    assert!(
        !cfg.contains("rules:"),
        "checks model must not emit rules: key:\n{cfg}"
    );
}

#[test]
fn init_existing_config_is_nonfatal_skipped() {
    // An existing .ironlint.yml must no longer be a hard error — init skips
    // scaffolding and prints a "already present (skipped)" note, but succeeds.
    let xdg = tempdir().unwrap();
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".ironlint.yml"), "existing\n").unwrap();
    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["init", "--dir", dir.path().to_str().unwrap(), "--no-hook"])
        .assert()
        .success()
        .stdout(predicates::str::contains("already present (skipped)"));
    // Original file content must be preserved.
    let content = fs::read_to_string(dir.path().join(".ironlint.yml")).unwrap();
    assert_eq!(content, "existing\n");
}

/// Generated config must validate with `ironlint validate`.
#[test]
fn init_generated_config_validates_ok() {
    for (manifest, name, contents) in [
        ("Cargo.toml", "Cargo.toml", "[package]\nname = \"foo\"\n"),
        ("package.json", "package.json", "{}\n"),
        (
            "pyproject.toml",
            "pyproject.toml",
            "[project]\nname=\"x\"\n",
        ),
        ("", "", ""),
    ] {
        let dir = tempdir().unwrap();
        if !manifest.is_empty() {
            fs::write(dir.path().join(name), contents).unwrap();
        }
        run_init(dir.path());
        let cfg_path = dir.path().join(".ironlint.yml");
        Command::cargo_bin("ironlint")
            .unwrap()
            .args(["validate", "--config", cfg_path.to_str().unwrap()])
            .assert()
            .code(0);
    }
}

/// Unknown stack (no manifest): universal baseline with no-fixme.
#[test]
fn init_unknown_stack_uses_generic_template() {
    let dir = tempdir().unwrap();
    run_init(dir.path());
    let cfg = read_cfg(dir.path());

    assert!(cfg.contains("no-fixme"));
}

#[test]
fn init_scaffolds_universal_baseline_regardless_of_stack() {
    for manifest in ["Cargo.toml", "package.json", "pyproject.toml", "none"] {
        let dir = tempfile::tempdir().unwrap();
        let xdg = tempfile::tempdir().unwrap();
        if manifest != "none" {
            std::fs::write(dir.path().join(manifest), "").unwrap();
        }
        Command::cargo_bin("ironlint")
            .unwrap()
            .env("XDG_CONFIG_HOME", xdg.path())
            .current_dir(dir.path())
            .args(["init", "--no-hook"])
            .assert()
            .success();
        let cfg = std::fs::read_to_string(dir.path().join(".ironlint.yml")).unwrap();
        assert!(
            cfg.contains("no-fixme:"),
            "{manifest}: missing no-fixme:\n{cfg}"
        );
        assert!(
            cfg.contains("no-merge-markers:"),
            "{manifest}: missing no-merge-markers:\n{cfg}"
        );
        assert!(cfg.contains("on: [change, accept]"));
        // No toolchain-specific scaffolding.
        for tool in [
            "biome",
            "eslint",
            "ruff",
            "clippy",
            "no-unwrap",
            "console.log",
        ] {
            assert!(
                !cfg.contains(tool),
                "{manifest}: must not scaffold `{tool}`:\n{cfg}"
            );
        }
    }
}

#[test]
fn scaffolded_baseline_validates() {
    let dir = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .current_dir(dir.path())
        .args(["init", "--no-hook"])
        .assert()
        .success();
    Command::cargo_bin("ironlint")
        .unwrap()
        .current_dir(dir.path())
        .args(["validate", "--config", ".ironlint.yml"])
        .assert()
        .success();
}

#[test]
fn init_dry_run_plans_skill_installs_for_explicit_harnesses() {
    let dir = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let out = assert_cmd::Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args([
            "init",
            "--dir",
            dir.path().to_str().unwrap(),
            "--harness",
            "pi",
            "--dry-run",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("ironlint · onboarding"), "plan header:\n{s}");
    assert!(s.contains("pi"), "must mention the pi harness:\n{s}");
    assert!(
        s.contains("requested"),
        "explicit harness tagged requested:\n{s}"
    );
    assert!(
        s.contains("skill"),
        "plan must include the skill step:\n{s}"
    );
    assert!(
        s.contains("skills/ironlint-config/SKILL.md"),
        "plan must name the skill path:\n{s}"
    );
}

/// `init` auto-blesses, so a `check` against the scaffolded config runs
/// without a separate `ironlint trust` step (it is not rejected as untrusted).
#[test]
fn init_auto_blesses_so_check_is_trusted() {
    let proj = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["init", "--dir"])
        .arg(proj.path())
        .assert()
        .success();

    let cfg = proj.path().join(".ironlint.yml");
    // Should NOT be rejected as untrusted.
    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["check", "--config"])
        .arg(&cfg)
        .arg("--root")
        .arg(proj.path())
        .assert();
    let code = out.get_output().status.code().unwrap();
    assert_eq!(
        code, 0,
        "a freshly scaffolded, otherwise empty project must pass its starter policy"
    );
}

/// Release-blocker regression: a fresh `init` that writes the baseline but
/// fails to store consent must not make a retry skip consent. The retry must
/// bless that exact unmodified baseline and leave the file untouched.
#[test]
fn init_retry_after_consent_failure_records_consent() {
    let dir = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    // A regular file where the trust store's directory must go makes consent
    // storage fail deterministically.
    let blocker = xdg.path().join("ironlint");
    fs::write(&blocker, "blocked").unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["init", "--dir", dir.path().to_str().unwrap(), "--no-hook"])
        .assert()
        .failure();

    let cfg = dir.path().join(".ironlint.yml");
    let scaffolded = fs::read(&cfg).unwrap();
    assert!(scaffolded.starts_with(b"version: 1"));
    let store = xdg.path().join("ironlint/trust.json");
    assert!(
        !store.exists(),
        "consent storage must have failed before the retry"
    );

    fs::remove_file(&blocker).unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["init", "--dir", dir.path().to_str().unwrap(), "--no-hook"])
        .assert()
        .success();

    assert_eq!(
        fs::read(&cfg).unwrap(),
        scaffolded,
        "retry must not rewrite the baseline"
    );
    assert!(store.exists(), "retry must record consent");
    assert!(
        fs::read_to_string(&store).unwrap().contains("sha256:"),
        "store must hold the blessed hash"
    );

    // Consent now actually admits the policy: exit 4 would mean it does not.
    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .code(0);
}

/// `init` must never bless or modify a config it did not scaffold: consent for
/// a user-owned policy is an explicit `ironlint trust` action.
#[test]
fn init_does_not_bless_or_modify_a_user_config() {
    let dir = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    let user = "version: 1\nchecks:\n  mine: {run: 'exit 0'}\n";
    fs::write(&cfg, user).unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["init", "--dir", dir.path().to_str().unwrap(), "--no-hook"])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&cfg).unwrap(), user);
    assert!(
        !xdg.path().join("ironlint/trust.json").exists(),
        "init must not record consent for a config it did not scaffold"
    );

    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .env("XDG_CONFIG_HOME", xdg.path())
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
        ])
        .assert();
    assert_eq!(
        out.get_output().status.code(),
        Some(4),
        "a user-owned config stays untrusted until `ironlint trust`"
    );
}
