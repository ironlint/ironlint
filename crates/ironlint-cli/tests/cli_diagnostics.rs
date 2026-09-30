//! Diagnostics inspect isolated installations without executing their contents.
use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;
use tempfile::TempDir;

struct Workspace {
    dir: TempDir,
    home: TempDir,
}

impl Workspace {
    fn new() -> Self {
        let workspace = Self {
            dir: tempfile::tempdir().unwrap(),
            home: tempfile::tempdir().unwrap(),
        };
        workspace.policy("version: 1\nchecks:\n  ok: {run: 'true'}\n");
        workspace
    }

    fn policy(&self, text: &str) {
        fs::write(self.dir.path().join(".ironlint.yml"), text).unwrap();
    }

    fn command(&self) -> Command {
        let mut command = Command::cargo_bin("ironlint").unwrap();
        command
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join(".config"))
            .env_remove("GIT_DIR")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_WORK_TREE");
        command
    }

    fn doctor(&self) -> Output {
        self.command()
            .args(["doctor", "--dir"])
            .arg(self.dir.path())
            .args(["--format", "json"])
            .output()
            .unwrap()
    }

    fn check(&self, format: &str) -> Output {
        self.command()
            .args(["check", "--root"])
            .arg(self.dir.path())
            .arg("--config")
            .arg(self.dir.path().join(".ironlint.yml"))
            .args(["--format", format])
            .output()
            .unwrap()
    }

    fn trust(&self) {
        self.command()
            .args(["trust", "--config"])
            .arg(self.dir.path().join(".ironlint.yml"))
            .assert()
            .success();
    }

    fn init_pi(&self, global: bool) {
        let mut command = self.command();
        command.args(["init", "--dir"]).arg(self.dir.path()).args([
            "--hook-only",
            "--harness",
            "pi",
            "--yes",
        ]);
        if global {
            command.arg("--global");
        }
        command.assert().success();
    }

    fn git_hook(&self) -> PathBuf {
        let output = std::process::Command::new("git")
            .args(["init", "-q"])
            .arg(self.dir.path())
            .env("HOME", self.home.path())
            .env_remove("GIT_DIR")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .unwrap();
        assert!(output.status.success());
        self.dir.path().join(".git/hooks/pre-commit")
    }
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn report(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("invalid JSON: {error}; output: {output:?}"))
}

fn named<'a>(report: &'a Value, name: &str) -> Vec<&'a Value> {
    report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["name"] == name)
        .collect()
}

#[test]
fn doctor_reports_global_only_pi_with_scope_and_artifact_path() {
    let workspace = Workspace::new();
    workspace.init_pi(true);
    let output = workspace.doctor();
    assert_eq!(output.status.code(), Some(0));
    let report = report(&output);
    let rows = named(&report, "pi");
    let row = rows.iter().find(|row| row["status"] == "pass").unwrap();
    let detail = row["detail"].as_str().unwrap();
    assert!(detail.contains("global"), "{row}");
    assert!(detail.contains(".pi/agent/extensions/ironlint.ts"), "{row}");
    assert_eq!(
        named(&report, "hooks")[0]["detail"],
        "1 harness(es) wired (healthy)"
    );
}

#[test]
fn doctor_reports_local_and_global_legacy_registrations_with_valid_cleanup() {
    let workspace = Workspace::new();
    let artifact = workspace
        .home
        .path()
        .join(".config/ironlint/adapters/codex/hook.sh");
    write(&artifact, "#!/bin/sh\nexit 1\n");
    let settings = serde_json::json!({"hooks": {"PreToolUse": [{"hooks": [{
        "type": "command", "command": format!("{} pre-tool-use", artifact.display())
    }]}]}});
    for path in [
        workspace.dir.path().join(".codex/hooks.json"),
        workspace.home.path().join(".codex/hooks.json"),
    ] {
        write(&path, &settings.to_string());
    }
    let output = workspace.doctor();
    let report = report(&output);
    let rows = named(&report, "codex");
    assert_eq!(rows.len(), 2, "independent registration paths: {report}");
    for (row, scope) in rows.iter().zip(["local", "global"]) {
        assert_ne!(row["status"], "pass", "{row}");
        assert!(row["detail"].as_str().unwrap().contains(scope), "{row}");
        assert!(
            row["detail"].as_str().unwrap().contains("unsupported"),
            "{row}"
        );
        let remediation = row["remediation"].as_str().unwrap();
        assert!(remediation.contains("--uninstall --harness codex"), "{row}");
        assert_eq!(remediation.contains("--global"), scope == "global", "{row}");
    }
}

#[test]
fn doctor_deduplicates_fallback_scopes_but_counts_independent_pi_installs() {
    let workspace = Workspace::new();
    workspace.init_pi(false);
    workspace.init_pi(true);
    write(
        &workspace.dir.path().join(".opencode/plugins/ironlint.ts"),
        "// foreign legacy plugin; preserve me\n",
    );
    let report = report(&workspace.doctor());
    let pi = named(&report, "pi");
    assert_eq!(pi.iter().filter(|row| row["status"] == "pass").count(), 2);
    assert_eq!(
        named(&report, "hooks")[0]["detail"],
        "2 harness(es) wired (healthy)"
    );
    let legacy = named(&report, "opencode");
    assert_eq!(legacy.len(), 1, "one physical fallback artifact: {report}");
    assert!(legacy[0]["detail"]
        .as_str()
        .unwrap()
        .contains("local/global"));
}

#[cfg(unix)]
#[test]
fn doctor_counts_a_hard_linked_pi_artifact_once_across_scopes() {
    let workspace = Workspace::new();
    workspace.init_pi(false);
    workspace.init_pi(true);
    let local = workspace.dir.path().join(".pi/extensions/ironlint.ts");
    let global = workspace
        .home
        .path()
        .join(".pi/agent/extensions/ironlint.ts");
    fs::remove_file(&global).unwrap();
    fs::hard_link(&local, &global).unwrap();
    let report = report(&workspace.doctor());
    let pi = named(&report, "pi");
    assert_eq!(pi.len(), 2, "independent ownership records: {report}");
    assert!(pi.iter().all(|row| row["status"] == "pass"));
    assert_eq!(
        named(&report, "hooks")[0]["detail"],
        "1 harness(es) wired (healthy)"
    );
}

#[test]
fn doctor_reports_registered_missing_primary_even_when_adapter_directory_exists() {
    let workspace = Workspace::new();
    let artifact = workspace
        .home
        .path()
        .join(".config/ironlint/adapters/codex/hook.sh");
    fs::create_dir_all(artifact.parent().unwrap()).unwrap();
    let settings = serde_json::json!({"hooks": {"PreToolUse": [{"hooks": [{
        "command": format!("{} pre-tool-use", artifact.display())
    }]}]}});
    write(
        &workspace.dir.path().join(".codex/hooks.json"),
        &settings.to_string(),
    );
    let output = workspace.doctor();
    assert_eq!(output.status.code(), Some(1));
    let report = report(&output);
    let rows = named(&report, "codex");
    let row = rows.iter().find(|row| row["status"] == "fail").unwrap();
    assert!(row["detail"].as_str().unwrap().contains("missing"), "{row}");
    assert!(row["detail"].as_str().unwrap().contains("hook.sh"), "{row}");
}

#[test]
fn doctor_reports_missing_pi_artifact_from_retained_ownership_metadata() {
    let workspace = Workspace::new();
    workspace.init_pi(false);
    let artifact = workspace.dir.path().join(".pi/extensions/ironlint.ts");
    fs::remove_file(&artifact).unwrap();
    let output = workspace.doctor();
    assert_eq!(output.status.code(), Some(1));
    let report = report(&output);
    let rows = named(&report, "pi");
    let row = rows.iter().find(|row| row["status"] == "fail").unwrap();
    assert!(row["detail"].as_str().unwrap().contains("missing"), "{row}");
    assert!(
        row["detail"]
            .as_str()
            .unwrap()
            .contains(artifact.to_str().unwrap()),
        "{row}"
    );
}

#[test]
fn doctor_preserves_malformed_and_unreadable_settings_errors() {
    for malformed in [true, false] {
        let workspace = Workspace::new();
        let settings = workspace.dir.path().join(".codex/hooks.json");
        if malformed {
            write(&settings, "{broken JSON");
        } else {
            fs::create_dir_all(&settings).unwrap();
        }
        let output = workspace.doctor();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let report = report(&output);
        let rows = named(&report, "codex");
        let row = rows.iter().find(|row| row["status"] == "fail").unwrap();
        let detail = row["detail"].as_str().unwrap();
        assert!(detail.contains(settings.to_str().unwrap()), "{row}");
        assert!(
            detail.contains(if malformed { "parsing" } else { "reading" }),
            "{row}"
        );
        assert!(row["remediation"].is_string(), "{row}");
    }
}

#[test]
fn doctor_does_not_report_pending_adapter_recovery_as_healthy() {
    let workspace = Workspace::new();
    workspace.init_pi(false);
    let pending = workspace
        .dir
        .path()
        .join(".pi/extensions/.ironlint-adapter.pending.json");
    write(&pending, "{}\n");
    let report = report(&workspace.doctor());
    let rows = named(&report, "pi");
    let row = rows
        .iter()
        .find(|row| row["detail"].as_str().unwrap().contains("local"))
        .unwrap();
    assert_ne!(row["status"], "pass", "{row}");
    assert!(
        row["detail"]
            .as_str()
            .unwrap()
            .contains("incomplete installation"),
        "{row}"
    );
    assert!(
        row["detail"]
            .as_str()
            .unwrap()
            .contains(pending.to_str().unwrap()),
        "{row}"
    );
    assert!(
        row["remediation"]
            .as_str()
            .unwrap()
            .contains("--harness pi"),
        "{row}"
    );
    assert_eq!(fs::read_to_string(pending).unwrap(), "{}\n");
}

#[test]
fn doctor_reports_artifact_read_and_sidecar_parse_errors_with_paths() {
    for invalid_sidecar in [true, false] {
        let workspace = Workspace::new();
        workspace.init_pi(false);
        let path = workspace.dir.path().join(if invalid_sidecar {
            ".pi/extensions/.ironlint-adapter.json"
        } else {
            ".pi/extensions/ironlint.ts"
        });
        if invalid_sidecar {
            fs::write(&path, "{malformed sidecar").unwrap();
        } else {
            fs::remove_file(&path).unwrap();
            fs::create_dir(&path).unwrap();
        }
        let output = workspace.doctor();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let report = report(&output);
        let rows = named(&report, "pi");
        let row = rows.iter().find(|row| row["status"] == "fail").unwrap();
        assert!(
            row["detail"]
                .as_str()
                .unwrap()
                .contains(path.to_str().unwrap()),
            "{row}"
        );
        assert!(row["remediation"].is_string(), "{row}");
    }
}

#[test]
fn doctor_inspects_legacy_modified_and_foreign_git_hooks_without_running_them() {
    for (body, expected) in [
        (
            "# >>> ironlint pre-commit floor >>>\nironlint diff --event tool-edit\n# <<< ironlint pre-commit floor <<<\n",
            "obsolete",
        ),
        (
            "# >>> ironlint pre-commit floor >>>\necho altered managed block\n# <<< ironlint pre-commit floor <<<\n",
            "modified",
        ),
        ("echo user-owned hook\n", "not managed"),
    ] {
        let workspace = Workspace::new();
        let hook = workspace.git_hook();
        let marker = workspace.dir.path().join("hook-was-executed");
        let contents = format!("#!/bin/sh\ntouch '{}'\n{body}", marker.display());
        write(&hook, &contents);
        let output = workspace.doctor();
        let report = report(&output);
        let rows = named(&report, "git_hook");
        assert_eq!(rows.len(), 1, "{report}");
        assert_ne!(rows[0]["status"], "pass");
        assert!(rows[0]["detail"].as_str().unwrap().contains(expected), "{}", rows[0]);
        assert!(!marker.exists(), "doctor executed installed hook content");
        assert_eq!(fs::read_to_string(hook).unwrap(), contents);
    }
}

#[test]
fn doctor_reports_current_git_hook_and_rejects_ambiguous_markers() {
    let workspace = Workspace::new();
    let hook = workspace.git_hook();
    workspace
        .command()
        .args(["init", "--dir"])
        .arg(workspace.dir.path())
        .args(["--hook-only", "--git-hook", "--yes"])
        .assert()
        .success();
    let healthy = report(&workspace.doctor());
    assert_eq!(named(&healthy, "git_hook")[0]["status"], "pass");
    let mut body = fs::read_to_string(&hook).unwrap();
    body.push_str("# >>> ironlint pre-commit floor >>>\n");
    fs::write(&hook, &body).unwrap();
    let output = workspace.doctor();
    assert_eq!(output.status.code(), Some(1));
    let report = report(&output);
    let row = named(&report, "git_hook")[0];
    assert_eq!(row["status"], "fail");
    assert!(row["detail"].as_str().unwrap().contains("markers"), "{row}");
    assert_eq!(fs::read_to_string(hook).unwrap(), body);
}

#[cfg(unix)]
#[test]
fn doctor_reports_a_nonexecutable_managed_git_hook_with_permission_remediation() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = Workspace::new();
    let hook = workspace.git_hook();
    workspace
        .command()
        .args(["init", "--dir"])
        .arg(workspace.dir.path())
        .args(["--hook-only", "--git-hook", "--yes"])
        .assert()
        .success();
    let contents = fs::read(&hook).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o644)).unwrap();
    let output = workspace.doctor();
    assert_eq!(output.status.code(), Some(1));
    let report = report(&output);
    let row = named(&report, "git_hook")[0];
    assert_eq!(row["status"], "fail");
    assert!(
        row["detail"].as_str().unwrap().contains("not executable"),
        "{row}"
    );
    assert!(
        row["detail"]
            .as_str()
            .unwrap()
            .contains(hook.to_str().unwrap()),
        "{row}"
    );
    let remediation = row["remediation"].as_str().unwrap();
    assert!(remediation.contains("chmod u+x"), "{row}");
    assert_eq!(fs::read(&hook).unwrap(), contents);
    assert_eq!(
        fs::metadata(hook).unwrap().permissions().mode() & 0o777,
        0o644
    );
}

#[test]
fn human_output_reports_top_level_drift_after_a_passing_check() {
    let workspace = Workspace::new();
    let policy =
        "version: 1\nchecks:\n  mutate: {run: \"printf '\\n# drift\\n' >> .ironlint.yml\"}\n";
    for format in ["human", "json"] {
        workspace.policy(policy);
        workspace.trust();
        let output = workspace.check(format);
        assert_eq!(output.status.code(), Some(3));
        if format == "json" {
            let verdict = report(&output);
            assert_eq!(verdict["schema"], 7);
            assert_eq!(verdict["status"], "error");
            assert_eq!(verdict["results"][0]["outcome"], "pass");
            assert!(verdict["error"]
                .as_str()
                .unwrap()
                .contains("changed during evaluation"));
        } else {
            assert_eq!(String::from_utf8_lossy(&output.stdout), "error\n");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("mutate: pass"), "{stderr}");
            assert!(stderr.contains("changed during evaluation"), "{stderr}");
            assert!(stderr.contains("re-run `ironlint trust`"), "{stderr}");
        }
    }
}

#[test]
fn human_output_reports_every_unexecuted_id_and_total_timeout_reason() {
    let workspace = Workspace::new();
    workspace.policy(
        "version: 1\nexecution:\n  timeout_secs: 5\n  total_timeout_secs: 1\nchecks:\n  a-slow: {run: 'sleep 2'}\n  b-later: {run: 'true'}\n  c-later: {run: 'true'}\n",
    );
    workspace.trust();
    let json_output = workspace.check("json");
    assert_eq!(json_output.status.code(), Some(3));
    let verdict = report(&json_output);
    assert_eq!(verdict["schema"], 7);
    assert_eq!(verdict["status"], "error");
    assert!(verdict["error"].as_str().unwrap().contains("total_timeout"));
    let output = workspace.check("human");
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "error\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    for skipped in verdict["not_run"].as_array().unwrap() {
        let id = skipped["id"].as_str().unwrap();
        let reason = skipped["reason"].as_str().unwrap();
        assert!(stderr.contains(id), "missing {id}: {stderr}");
        assert!(stderr.contains(reason), "missing reason {reason}: {stderr}");
    }
    assert_eq!(verdict["not_run"].as_array().unwrap().len(), 2);
}
