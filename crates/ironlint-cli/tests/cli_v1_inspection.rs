//! Retained read-only commands understand valid v1 policies without trust.

mod common;

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

const POLICY: &str = r#"version: 1
checks:
  acceptance:
    run: "exit 99"
  feedback:
    files: ["src/**"]
    on: [change, accept]
    run: "exit 98"
  other-feedback:
    files: ["tests/**"]
    on: [change, accept]
    run: "exit 97"
"#;

#[test]
fn retained_inspection_commands_accept_untrusted_v1_policy() {
    let dir = tempdir().unwrap();
    let home = tempdir().unwrap();
    let config = dir.path().join(".ironlint.yml");
    fs::write(&config, POLICY).unwrap();
    let file = dir.path().join("src/main.rs");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "fn main() {}\n").unwrap();

    let explain = Command::cargo_bin("ironlint")
        .unwrap()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .args([
            "explain",
            "--config",
            config.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        explain.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&explain.stderr)
    );
    let explain: Value = serde_json::from_slice(&explain.stdout).unwrap();
    assert_eq!(explain[0]["check"], "acceptance");
    assert_eq!(
        explain[0]["acceptance"], "required",
        "acceptance applies regardless of file scope"
    );
    assert_eq!(explain[0]["change"], "not-enabled");
    assert_eq!(explain[1]["check"], "feedback");
    assert_eq!(explain[1]["acceptance"], "required");
    assert_eq!(explain[1]["change"], "match");
    assert_eq!(explain[2]["acceptance"], "required");
    assert_eq!(explain[2]["change"], "skip");

    let docs_file = dir.path().join("docs/readme.md");
    fs::create_dir_all(docs_file.parent().unwrap()).unwrap();
    fs::write(&docs_file, "readme\n").unwrap();
    let docs_explain = Command::cargo_bin("ironlint")
        .unwrap()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .args([
            "explain",
            "--config",
            config.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
            docs_file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(docs_explain.status.code(), Some(0));
    let docs_explain: Value = serde_json::from_slice(&docs_explain.stdout).unwrap();
    let feedback = &docs_explain[1];
    assert_eq!(feedback["acceptance"], "required");
    assert_eq!(feedback["change"], "skip");

    let resolved = Command::cargo_bin("ironlint")
        .unwrap()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .args([
            "show-resolved-config",
            "--config",
            config.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        resolved.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    let resolved: Value = serde_json::from_slice(&resolved.stdout).unwrap();
    assert_eq!(resolved.as_array().unwrap().len(), 3);
    assert_eq!(resolved[0]["check"], "acceptance");
    assert_eq!(resolved[0]["files"], Value::Array(vec![]));
    assert!(resolved[0]["origin"]
        .as_str()
        .unwrap()
        .contains(".ironlint.yml"));
    assert_eq!(resolved[1]["check"], "feedback");
    assert_eq!(resolved[1]["files"][0], "src/**");

    let doctor = Command::cargo_bin("ironlint")
        .unwrap()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .args([
            "doctor",
            "--dir",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        doctor.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    let doctor: Value = serde_json::from_slice(&doctor.stdout).unwrap();
    let parses = doctor["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == "parses")
        .unwrap();
    assert_eq!(parses["status"], "pass");
}

#[test]
fn v1_explain_uses_explicit_evaluation_root_for_change_scope() {
    let policy_dir = tempdir().unwrap();
    let root = tempdir().unwrap();
    let home = tempdir().unwrap();
    let config = policy_dir.path().join(".ironlint.yml");
    fs::write(
        &config,
        r#"version: 1
checks:
  feedback:
    files: ["src/**"]
    on: [change, accept]
    run: "exit 0"
"#,
    )
    .unwrap();
    let file = root.path().join("src/a.rs");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "fn main() {}\n").unwrap();

    let explain = Command::cargo_bin("ironlint")
        .unwrap()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .args([
            "explain",
            "--config",
            config.to_str().unwrap(),
            "--root",
            root.path().to_str().unwrap(),
            "--format",
            "json",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(explain.status.code(), Some(0));
    let explain: Value = serde_json::from_slice(&explain.stdout).unwrap();
    assert_eq!(explain[0]["change"], "match");
}

#[test]
fn v1_explain_defaults_to_the_same_root_as_check() {
    let policy_dir = tempdir().unwrap();
    let root = tempdir().unwrap();
    let config = policy_dir.path().join("policy.yml");
    fs::write(
        &config,
        "version: 1\nchecks:\n  feedback:\n    files: [\"src/**\"]\n    on: [change, accept]\n    run: \"exit 0\"\n",
    )
    .unwrap();
    let file = root.path().join("src/a.rs");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "fn main() {}\n").unwrap();
    let xdg = common::blessed_store(&config);

    let check = Command::cargo_bin("ironlint")
        .unwrap()
        .current_dir(root.path())
        .env("XDG_CONFIG_HOME", xdg.path())
        .args([
            "check",
            "--config",
            config.to_str().unwrap(),
            "--event",
            "change",
            "--file",
            file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(check.status.code(), Some(0));
    let verdict: Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(verdict["results"][0]["id"], "feedback");

    let explain = Command::cargo_bin("ironlint")
        .unwrap()
        .current_dir(root.path())
        .args([
            "explain",
            "--config",
            config.to_str().unwrap(),
            "--format",
            "json",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        explain.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&explain.stderr)
    );
    let rows: Value = serde_json::from_slice(&explain.stdout).unwrap();
    assert_eq!(rows[0]["change"], "match");
}

#[test]
fn v1_change_accepts_deleted_path_under_replaced_directory() {
    let root = tempdir().unwrap();
    let config = root.path().join("policy.yml");
    fs::write(
        &config,
        "version: 1\nchecks:\n  feedback:\n    files: [\"old/**\"]\n    on: [change, accept]\n    run: \"exit 0\"\n",
    )
    .unwrap();
    fs::write(root.path().join("old"), "replacement file\n").unwrap();
    let xdg = common::blessed_store(&config);

    let check = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            config.to_str().unwrap(),
            "--root",
            root.path().to_str().unwrap(),
            "--event",
            "change",
            "--file",
            "old/deleted.rs",
            "--format",
            "json",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .output()
        .unwrap();
    assert_eq!(
        check.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&check.stdout)
    );
    let value: Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(value["status"], "pass");
    assert_eq!(value["results"][0]["id"], "feedback");

    let explain = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "explain",
            "--config",
            config.to_str().unwrap(),
            "--root",
            root.path().to_str().unwrap(),
            "--format",
            "json",
            "old/deleted.rs",
        ])
        .output()
        .unwrap();
    assert_eq!(
        explain.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&explain.stderr)
    );
    let value: Value = serde_json::from_slice(&explain.stdout).unwrap();
    assert_eq!(value[0]["change"], "match");
}

#[cfg(unix)]
#[test]
fn show_resolved_config_reports_canonical_v1_origin_for_symlink_alias() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let home = tempdir().unwrap();
    let real = dir.path().join("real.yml");
    let alias = dir.path().join("alias.yml");
    fs::write(&real, "version: 1\nchecks:\n  check:\n    run: 'exit 0'\n").unwrap();
    symlink(&real, &alias).unwrap();

    let resolved = Command::cargo_bin("ironlint")
        .unwrap()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .args([
            "show-resolved-config",
            "--config",
            alias.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(resolved.status.code(), Some(0));
    let resolved: Value = serde_json::from_slice(&resolved.stdout).unwrap();
    let canonical_real = real.canonicalize().unwrap();
    assert_eq!(resolved[0]["origin"].as_str(), canonical_real.to_str());
}

#[cfg(unix)]
#[test]
fn v1_check_accepts_absolute_file_through_symlink_root_alias() {
    use std::os::unix::fs::symlink;

    let root = tempdir().unwrap();
    let alias_dir = tempdir().unwrap();
    let alias = alias_dir.path().join("root");
    symlink(root.path(), &alias).unwrap();
    let config = root.path().join("policy.yml");
    fs::write(
        &config,
        "version: 1\nchecks:\n  feedback: {on: [change, accept], run: 'exit 0'}\n",
    )
    .unwrap();
    let file = root.path().join("src/a.rs");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "fn main() {}\n").unwrap();
    let xdg = common::blessed_store(&config);

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            config.to_str().unwrap(),
            "--root",
            alias.to_str().unwrap(),
            "--event",
            "change",
            "--file",
            alias.join("src/a.rs").to_str().unwrap(),
            "--format",
            "json",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "pass");
}

#[test]
fn v1_explain_rejects_invalid_root_and_escaping_path() {
    let root = tempdir().unwrap();
    let home = tempdir().unwrap();
    let config = root.path().join("policy.yml");
    fs::write(
        &config,
        "version: 1\nchecks:\n  feedback: {files: src/**, on: [change, accept], run: 'exit 0'}\n",
    )
    .unwrap();
    let file = root.path().join("src/a.rs");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "fn main() {}\n").unwrap();

    let invalid_root = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "explain",
            "--config",
            config.to_str().unwrap(),
            "--root",
            root.path().join("missing").to_str().unwrap(),
            file.to_str().unwrap(),
        ])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .output()
        .unwrap();
    assert_eq!(invalid_root.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&invalid_root.stderr).contains("--root"));

    let escaping_file = root.path().parent().unwrap().join("outside.rs");
    let escaping = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "explain",
            "--config",
            config.to_str().unwrap(),
            "--root",
            root.path().to_str().unwrap(),
            escaping_file.to_str().unwrap(),
        ])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .output()
        .unwrap();
    assert_eq!(escaping.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&escaping.stderr).contains("file path escapes --root"));
}

#[test]
fn v1_check_rejects_invalid_root_and_path_before_trust() {
    let root = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    let config = root.path().join("policy.yml");
    fs::write(
        &config,
        "version: 1\nchecks:\n  feedback: {on: [change, accept], run: 'exit 0'}\n",
    )
    .unwrap();
    let file = root.path().join("src/a.rs");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "fn main() {}\n").unwrap();

    let invalid_root = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            config.to_str().unwrap(),
            "--root",
            root.path().join("missing").to_str().unwrap(),
            "--event",
            "change",
            "--file",
            file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .output()
        .unwrap();
    assert_eq!(invalid_root.status.code(), Some(1));
    let invalid_root: Value = serde_json::from_slice(&invalid_root.stdout).unwrap();
    assert_eq!(invalid_root["schema"], 7);
    assert!(invalid_root["error"].as_str().unwrap().contains("--root"));

    let escaping_file = root.path().parent().unwrap().join("outside.rs");
    let escaping = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            config.to_str().unwrap(),
            "--root",
            root.path().to_str().unwrap(),
            "--event",
            "change",
            "--file",
            escaping_file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .output()
        .unwrap();
    assert_eq!(escaping.status.code(), Some(1));
    let escaping: Value = serde_json::from_slice(&escaping.stdout).unwrap();
    assert_eq!(escaping["schema"], 7);
    assert!(escaping["error"]
        .as_str()
        .unwrap()
        .contains("file path escapes --root"));
}

#[test]
fn legacy_check_and_explain_reject_explicit_root() {
    let root = tempdir().unwrap();
    let config = root.path().join("policy.yml");
    fs::write(
        &config,
        "checks:\n  all: {files: '**/*.rs', run: 'exit 0'}\n",
    )
    .unwrap();
    let xdg = common::blessed_store(&config);
    let file = root.path().join("src/a.rs");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "fn main() {}\n").unwrap();

    let check = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            config.to_str().unwrap(),
            "--root",
            root.path().to_str().unwrap(),
            "--file",
            file.to_str().unwrap(),
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .output()
        .unwrap();
    assert_eq!(check.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&check.stderr).contains("--root"));

    let explain = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "explain",
            "--config",
            config.to_str().unwrap(),
            "--root",
            root.path().to_str().unwrap(),
            file.to_str().unwrap(),
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .output()
        .unwrap();
    assert_eq!(explain.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&explain.stderr).contains("--root"));
}
