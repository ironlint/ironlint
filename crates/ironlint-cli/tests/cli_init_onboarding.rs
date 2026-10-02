use assert_cmd::Command;
use std::path::Path;

fn ironlint(home: &Path, project: &Path) -> Command {
    let mut command = Command::cargo_bin("ironlint").unwrap();
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .current_dir(project);
    command
}

fn workspace() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let project = tmp.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    (tmp, home, project)
}

#[test]
fn explicit_pi_install_writes_plugin_and_skill() {
    let (_tmp, home, project) = workspace();
    ironlint(&home, &project)
        .args(["init", "--harness", "pi", "--yes"])
        .assert()
        .success();
    assert!(project.join(".pi/extensions/ironlint.ts").exists());
    assert!(project.join(".pi/skills/ironlint-config/SKILL.md").exists());
}

#[test]
fn automatic_nonterminal_install_requires_detection_and_yes() {
    for (detected, yes, installed) in [
        (false, false, false),
        (false, true, false),
        (true, false, false),
        (true, true, true),
    ] {
        let (_tmp, home, project) = workspace();
        if detected {
            std::fs::create_dir_all(home.join(".pi")).unwrap();
        }
        let mut command = ironlint(&home, &project);
        command.args(["init", "--hook-only"]);
        if yes {
            command.arg("--yes");
        }
        command.assert().success();
        assert_eq!(
            project.join(".pi/extensions/ironlint.ts").exists(),
            installed
        );
    }
}

#[test]
fn automatic_uninstall_finds_owned_skill_after_plugin_is_missing() {
    let (_tmp, home, project) = workspace();
    ironlint(&home, &project)
        .args(["init", "--hook-only", "--harness", "pi", "--yes"])
        .assert()
        .success();
    std::fs::remove_dir_all(project.join(".pi/extensions")).unwrap();
    ironlint(&home, &project)
        .args(["init", "--uninstall", "--yes"])
        .assert()
        .success();
    assert!(!project.join(".pi/skills/ironlint-config/SKILL.md").exists());
}

#[test]
fn pi_reinstall_reports_already_present() {
    let (_tmp, home, project) = workspace();
    for _ in 0..2 {
        ironlint(&home, &project)
            .args(["init", "--hook-only", "--harness", "pi", "--yes"])
            .assert()
            .success();
    }
    ironlint(&home, &project)
        .args(["init", "--hook-only", "--harness", "pi", "--yes"])
        .assert()
        .success()
        .stdout(predicates::str::contains("already present"));
}

#[test]
fn pi_dry_run_writes_nothing() {
    let (_tmp, home, project) = workspace();
    ironlint(&home, &project)
        .args(["init", "--hook-only", "--harness", "pi", "--dry-run"])
        .assert()
        .success();
    assert!(!project.join(".pi").exists());
}

#[test]
fn pi_uninstall_removes_owned_plugin_and_skill() {
    let (_tmp, home, project) = workspace();
    ironlint(&home, &project)
        .args(["init", "--hook-only", "--harness", "pi", "--yes"])
        .assert()
        .success();
    ironlint(&home, &project)
        .args(["init", "--uninstall", "--harness", "pi", "--yes"])
        .assert()
        .success();
    assert!(!project.join(".pi/extensions/ironlint.ts").exists());
    assert!(!project.join(".pi/skills/ironlint-config/SKILL.md").exists());
}

#[test]
fn pi_uninstall_preserves_edited_plugin_and_reports_incomplete_cleanup() {
    let (_tmp, home, project) = workspace();
    ironlint(&home, &project)
        .args(["init", "--hook-only", "--harness", "pi", "--yes"])
        .assert()
        .success();
    let plugin = project.join(".pi/extensions/ironlint.ts");
    std::fs::write(&plugin, "// user edit\n").unwrap();

    ironlint(&home, &project)
        .args(["init", "--uninstall", "--harness", "pi", "--yes"])
        .assert()
        .code(3)
        .stdout(predicates::str::contains("preserved edited"));

    assert_eq!(std::fs::read_to_string(plugin).unwrap(), "// user edit\n");
    assert!(!project.join(".pi/skills/ironlint-config/SKILL.md").exists());
}

#[test]
fn opencode_cleanup_only_adapter_cannot_be_newly_installed() {
    let (_tmp, home, project) = workspace();
    ironlint(&home, &project)
        .args(["init", "--hook-only", "--harness", "opencode", "--yes"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "available: claude-code, codex, pi",
        ));
}

#[test]
fn runtime_pi_package_can_be_updated_after_the_binary_is_built() {
    let (tmp, home, project) = workspace();
    let packages = tmp.path().join("packages");
    std::fs::create_dir_all(packages.join("pi/src")).unwrap();
    std::fs::create_dir_all(packages.join("shared/ironlint-config")).unwrap();
    std::fs::write(
        packages.join("shared/ironlint-config/SKILL.md"),
        "test authoring skill",
    )
    .unwrap();
    let plugin = packages.join("pi/src/index.ts");
    for text in ["// adapter version one\n", "// adapter version two\n"] {
        std::fs::write(&plugin, text).unwrap();
        ironlint(&home, &project)
            .env("IRONLINT_ADAPTERS_ROOT", &packages)
            .args(["init", "--hook-only", "--harness", "pi", "--yes"])
            .assert()
            .success();
        assert_eq!(
            std::fs::read_to_string(project.join(".pi/extensions/ironlint.ts")).unwrap(),
            text
        );
    }
}

#[test]
fn native_adapter_install_migrates_only_owned_entries_and_preserves_edits() {
    for (name, settings_name) in [
        ("codex", ".codex/hooks.json"),
        ("claude-code", ".claude/settings.local.json"),
    ] {
        let (_tmp, home, project) = workspace();
        let settings = project.join(settings_name);
        let directory = home.join(".config/ironlint/adapters").join(name);
        let marker = directory.join("hook.sh");
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(
            &settings,
            serde_json::to_vec(&serde_json::json!({"hooks": {"PreToolUse": [
            {"hooks": [{"type": "command", "command": format!("{} pre-tool-use", marker.display())},
                       {"type": "command", "command": "my-existing-hook"}]}]}}))
            .unwrap(),
        )
        .unwrap();
        ironlint(&home, &project)
            .args(["init", "--hook-only", "--harness", name, "--yes"])
            .assert()
            .success();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
        assert_eq!(
            value["hooks"]["PreToolUse"][0]["hooks"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            value["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            "my-existing-hook"
        );
        assert!(value["hooks"]["PostToolUse"].is_array());
        assert!(value["hooks"]["Stop"].is_array());
        let runner = directory.join("hook.py");
        std::fs::write(&runner, "# user edits\n").unwrap();
        let before = std::fs::read(&settings).unwrap();
        ironlint(&home, &project)
            .args(["init", "--hook-only", "--harness", name, "--yes"])
            .assert()
            .success()
            .stdout(predicates::str::contains("preserved edited"));
        assert_eq!(std::fs::read(&settings).unwrap(), before);
        ironlint(&home, &project)
            .args(["init", "--uninstall", "--harness", name, "--yes"])
            .assert()
            .code(3);
        assert_eq!(std::fs::read_to_string(runner).unwrap(), "# user edits\n");
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
        assert_eq!(value["hooks"]["PostToolUse"].as_array().unwrap().len(), 0);
        assert_eq!(value["hooks"]["Stop"].as_array().unwrap().len(), 0);
        assert_eq!(
            value["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            "my-existing-hook"
        );
    }
}

#[test]
fn replacing_shared_hook_files_also_migrates_owned_global_registration() {
    let (_tmp, home, project) = workspace();
    let settings = home.join(".codex/hooks.json");
    let marker = home.join(".config/ironlint/adapters/codex/hook.sh");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(
        &settings,
        serde_json::to_vec(&serde_json::json!({"hooks": {"PreToolUse": [
        {"hooks": [{"type": "command", "command": format!("{} pre-tool-use", marker.display())}]},
        {"hooks": [{"type": "command", "command": "foreign-global-hook"}]}]}}))
        .unwrap(),
    )
    .unwrap();
    ironlint(&home, &project)
        .args(["init", "--hook-only", "--harness", "codex", "--yes"])
        .assert()
        .success();
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
    assert_eq!(value["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    assert_eq!(
        value["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "foreign-global-hook"
    );
    assert!(value["hooks"]["PostToolUse"].is_array());
    assert!(value["hooks"]["Stop"].is_array());
}

#[test]
fn doctor_detects_a_missing_stop_registration_without_requiring_package_sources() {
    let (tmp, home, project) = workspace();
    ironlint(&home, &project)
        .args(["init", "--harness", "codex", "--yes"])
        .assert()
        .success();
    let settings = project.join(".codex/hooks.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
    value["hooks"].as_object_mut().unwrap().remove("Stop");
    std::fs::write(&settings, serde_json::to_vec(&value).unwrap()).unwrap();
    let output = ironlint(&home, &project)
        .env(
            "IRONLINT_ADAPTERS_ROOT",
            tmp.path().join("missing-packages"),
        )
        .args(["doctor", "--format", "json"])
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = report["checks"].as_array().unwrap();
    let native = rows
        .iter()
        .find(|row| row["name"] == "codex" && row["detail"].as_str().unwrap().contains("local"))
        .unwrap();
    assert_eq!(native["status"], "warn", "{report}");
    ironlint(&home, &project)
        .env(
            "IRONLINT_ADAPTERS_ROOT",
            tmp.path().join("missing-packages"),
        )
        .args(["init", "--uninstall", "--harness", "codex", "--yes"])
        .assert()
        .success();
}

#[test]
fn default_uninstall_discovers_and_removes_legacy_registration() {
    let (_tmp, home, project) = workspace();
    let settings = project.join(".codex/hooks.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let marker = home.join(".config/ironlint/adapters/codex/hook.sh");
    std::fs::write(
        &settings,
        format!(
            r#"{{
  "hooks": {{
    "PreToolUse": [
      {{"hooks": [{{"type": "command", "command": "{} pre-tool-use"}}]}},
      {{"hooks": [{{"type": "command", "command": "other-tool"}}]}}
    ]
  }}
}}"#,
            marker.display()
        ),
    )
    .unwrap();

    ironlint(&home, &project)
        .args(["init", "--uninstall", "--yes"])
        .assert()
        .success();

    let updated = std::fs::read_to_string(settings).unwrap();
    assert!(!updated.contains("ironlint/adapters/codex"));
    assert!(updated.contains("other-tool"));
}

#[test]
fn default_uninstall_also_removes_global_legacy_registration() {
    let (_tmp, home, project) = workspace();
    let settings = home.join(".codex/hooks.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let marker = home.join(".config/ironlint/adapters/codex/hook.sh");
    std::fs::write(
        &settings,
        format!(
            r#"{{
  "hooks": {{
    "PreToolUse": [
      {{"hooks": [{{"type": "command", "command": "{} pre-tool-use"}}]}},
      {{"hooks": [{{"type": "command", "command": "other-tool"}}]}}
    ]
  }}
}}"#,
            marker.display()
        ),
    )
    .unwrap();

    ironlint(&home, &project)
        .args(["init", "--uninstall", "--yes"])
        .assert()
        .success();

    let updated = std::fs::read_to_string(settings).unwrap();
    assert!(!updated.contains("ironlint/adapters/codex"));
    assert!(updated.contains("other-tool"));
}

#[test]
fn legacy_uninstall_fails_if_any_registration_cannot_be_cleaned() {
    let (_tmp, home, project) = workspace();
    let settings = home.join(".codex/hooks.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(&settings, "{not valid json").unwrap();

    ironlint(&home, &project)
        .args(["init", "--uninstall", "--yes"])
        .assert()
        .code(3)
        .stdout(predicates::str::contains("codex (global) failed"));

    assert_eq!(
        std::fs::read_to_string(settings).unwrap(),
        "{not valid json"
    );
}
