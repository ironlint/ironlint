use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::process::Output;
use tempfile::{tempdir, TempDir};

struct Workspace {
    root: TempDir,
    home: TempDir,
}
impl Workspace {
    fn new(policy: &str) -> Self {
        let workspace = Self {
            root: tempdir().unwrap(),
            home: tempdir().unwrap(),
        };
        workspace.policy(policy);
        workspace
    }
    fn policy(&self, policy: &str) {
        fs::write(self.root.path().join(".ironlint.yml"), policy).unwrap();
    }
    fn command(&self) -> Command {
        let mut command = Command::cargo_bin("ironlint").unwrap();
        command
            .current_dir(self.root.path())
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join(".config"));
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
    fn trust(&self) {
        self.command().arg("trust").assert().success();
    }
}

#[test]
fn inspection_reports_override_and_configured_timeout_without_total_capping() {
    let workspace = Workspace::new("version: 1\nexecution: {timeout_secs: 30, total_timeout_secs: 1}\nchecks:\n  a-inherit: {run: 'touch executed'}\n  b-short: {timeout_secs: 10, run: 'touch executed'}\n  c-long: {timeout_secs: 180, run: 'touch executed'}\n");
    for args in [
        vec!["show-resolved-config", "--format", "json"],
        vec!["explain", "src/a.rs", "--format", "json"],
    ] {
        let output = workspace.run(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
        for (row, (timeout, effective)) in
            rows.as_array()
                .unwrap()
                .iter()
                .zip([(None, 30), (Some(10), 10), (Some(180), 180)])
        {
            assert!(row.get("timeout_secs").is_some());
            assert_eq!(
                row["timeout_secs"],
                timeout.map_or(Value::Null, Value::from)
            );
            assert_eq!(row["effective_timeout_secs"], effective);
        }
    }
    let output = workspace.run(&["show-resolved-config", "--format", "yaml"]);
    assert!(output.status.success());
    let rows: serde_yaml::Value = serde_yaml::from_slice(&output.stdout).unwrap();
    assert!(rows[0]["timeout_secs"].is_null());
    assert_eq!(rows[0]["effective_timeout_secs"].as_u64(), Some(30));
    assert_eq!(rows[2]["effective_timeout_secs"].as_u64(), Some(180));
    let tsv = workspace.run(&["show-resolved-config"]);
    assert!(tsv.status.success());
    let tsv = String::from_utf8(tsv.stdout).unwrap();
    let rows: Vec<_> = tsv
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .collect();
    assert!(rows.iter().all(|row| row.len() == 6));
    assert_eq!(&rows[0][4..], &["", "30"]);
    assert_eq!(&rows[2][4..], &["180", "180"]);
    let human = workspace.run(&["explain", "src/a.rs"]);
    assert!(human.status.success());
    let human = String::from_utf8(human.stdout).unwrap();
    assert!(human.contains("timeout_secs=default effective_timeout_secs=30"));
    assert!(human.contains("timeout_secs=180 effective_timeout_secs=180"));
    assert!(!workspace.root.path().join("executed").exists());
    assert_eq!(fs::read_dir(workspace.home.path()).unwrap().count(), 0);
}

#[test]
fn inherited_inspection_timeout_is_present_for_existing_policy() {
    let workspace = Workspace::new("version: 1\nchecks:\n  ok: {run: 'true'}\n");
    let output = workspace.run(&["show-resolved-config", "--format", "json"]);
    assert!(output.status.success());
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(rows[0].get("timeout_secs").is_some());
    assert!(rows[0]["timeout_secs"].is_null());
    assert_eq!(rows[0]["effective_timeout_secs"], 30);
}

#[test]
fn adding_or_changing_an_override_requires_renewed_consent() {
    let workspace = Workspace::new("version: 1\nchecks:\n  ok: {run: 'true'}\n");
    workspace.trust();
    for timeout in [1, 2] {
        workspace.policy(&format!(
            "version: 1\nchecks:\n  ok: {{timeout_secs: {timeout}, run: 'true'}}\n"
        ));
        let denied = workspace.run(&["check", "--format", "json"]);
        assert_eq!(denied.status.code(), Some(4));
        let report: Value = serde_json::from_slice(&denied.stdout).unwrap();
        assert_eq!(report["schema"], 7);
        workspace.trust();
        assert!(workspace
            .run(&["check", "--format", "json"])
            .status
            .success());
    }
}

#[test]
fn validate_rejects_invalid_override_values_with_input_exit() {
    let workspace = Workspace::new("version: 1\nchecks:\n  ok: {run: 'true'}\n");
    for value in ["0", "-1", "1.5", "null", "'1'", "true"] {
        workspace.policy(&format!(
            "version: 1\nchecks:\n  ok: {{timeout_secs: {value}, run: 'true'}}\n"
        ));
        let output = workspace.run(&["validate", "--format", "json"]);
        assert_eq!(output.status.code(), Some(1));
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["status"], "error");
        assert!(report["reason"].as_str().unwrap().contains("timeout_secs"));
    }
    assert_eq!(fs::read_dir(workspace.home.path()).unwrap().count(), 0);
}

#[test]
fn timeout_support_is_identified_as_version_1_1_0() {
    let workspace = Workspace::new("version: 1\nchecks:\n  ok: {run: 'true'}\n");
    let output = workspace.run(&["--version"]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ironlint 1.1.0\n"
    );
}
