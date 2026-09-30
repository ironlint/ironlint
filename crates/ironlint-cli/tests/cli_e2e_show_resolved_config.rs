//! End-to-end coverage for `ironlint show-resolved-config` (checks model).
//!
//! Default output format (from show_resolved_config.rs) is TSV — one
//! tab-separated row per check:
//!   check_id<TAB>origin<TAB>files(comma-joined)<TAB>run<TAB>timeout_secs<TAB>effective_timeout_secs

use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn show_resolved_config_default_tsv_row_per_check() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    std::fs::write(
        &cfg,
        "version: 1\nchecks:\n  no-todo:\n    files: [\"*.rs\", \"*.txt\"]\n    run: \"grep -q TODO && exit 2 || exit 0\"\n",
    )
    .unwrap();

    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args(["show-resolved-config", "--config", cfg.to_str().unwrap()])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&out);

    let line = stdout
        .lines()
        .find(|l| l.starts_with("no-todo"))
        .expect("default format must emit a TSV row for no-todo");
    let cols: Vec<&str> = line.split('\t').collect();
    assert_eq!(
        cols.len(),
        6,
        "TSV row must be 6 tab-separated columns: {line:?}"
    );
    assert_eq!(cols[0], "no-todo", "col 1 is the check id");
    assert!(
        cols[1].contains(".ironlint.yml"),
        "col 2 (origin) must reference the config file: {line:?}"
    );
    assert_eq!(
        cols[2], "*.rs,*.txt",
        "col 3 is the comma-joined files glob: {line:?}"
    );
    assert!(
        cols[3].contains("grep -q TODO"),
        "col 4 is the run command: {line:?}"
    );
}

#[test]
fn show_resolved_config_lists_multiple_checks() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    std::fs::write(
        &cfg,
        "version: 1\nchecks:\n  alpha:\n    files: [\"*.rs\"]\n    run: \"true\"\n  beta:\n    files: [\"*.ts\"]\n    run: \"true\"\n",
    )
    .unwrap();

    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args(["show-resolved-config", "--config", cfg.to_str().unwrap()])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&out);

    assert!(stdout.contains("alpha"), "must show alpha check: {stdout}");
    assert!(stdout.contains("beta"), "must show beta check: {stdout}");
}

#[test]
fn show_resolved_config_yaml_serializes_v1_rows() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join(".ironlint.yml");
    std::fs::write(
        &cfg,
        "version: 1\nchecks:\n  acceptance:\n    run: cargo test --locked\n",
    )
    .unwrap();

    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "show-resolved-config",
            "--config",
            cfg.to_str().unwrap(),
            "--format",
            "yaml",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let rows: serde_yaml::Value = serde_yaml::from_slice(&out).unwrap();
    let row = &rows.as_sequence().unwrap()[0];
    assert_eq!(row["check"], "acceptance");
    assert_eq!(row["run"], "cargo test --locked");
    assert_eq!(row["files"], serde_yaml::Value::Sequence(Vec::new()));
    assert!(row["origin"].as_str().unwrap().ends_with(".ironlint.yml"));
}

#[test]
fn show_resolved_config_missing_config_exits_one() {
    let dir = tempdir().unwrap();
    let absent = dir.path().join(".ironlint.yml");
    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .args(["show-resolved-config", "--config", absent.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.starts_with("error: "),
        "stderr must lead with error: prefix: {stderr}"
    );
}
