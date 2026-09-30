//! Exercise setup through a real terminal, with isolated homes and projects.
#![cfg(unix)]

use expectrl::{ControlCode, Eof, Session, WaitStatus};
use ironlint_core::adapter::{sha256_hex, sidecar_path, AdapterSidecar};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

struct Workspace {
    _directory: tempfile::TempDir,
    home: PathBuf,
    project: PathBuf,
}

impl Workspace {
    fn new(detected: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let project = directory.path().join("project");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&project).unwrap();
        if detected {
            fs::create_dir(home.join(".pi")).unwrap();
        }
        Self {
            _directory: directory,
            home,
            project,
        }
    }

    fn session(&self, arguments: &[&str]) -> Session {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ironlint"));
        command
            .args(arguments)
            .current_dir(&self.project)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("TERM", "dumb")
            .env_remove("GIT_DIR")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_WORK_TREE");
        let mut session = Session::spawn(command).unwrap();
        session.set_expect_timeout(Some(Duration::from_secs(3)));
        session
    }

    fn plugin(&self) -> PathBuf {
        self.project.join(".pi/extensions/ironlint.ts")
    }

    fn git_hook(&self) -> PathBuf {
        let output = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&self.project)
            .env("HOME", &self.home)
            .env_remove("GIT_DIR")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .unwrap();
        assert!(output.status.success());
        self.project.join(".git/hooks/pre-commit")
    }

    fn legacy_settings(&self) -> Vec<PathBuf> {
        let artifact = self.home.join(".config/ironlint/adapters/codex/hook.sh");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        let bytes = b"#!/bin/sh\n# owned legacy artifact\nexit 1\n";
        fs::write(&artifact, bytes).unwrap();
        let record = AdapterSidecar {
            files: BTreeMap::from([("hook.sh".into(), sha256_hex(bytes))]),
        };
        fs::write(
            sidecar_path(artifact.parent().unwrap()),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        let settings = serde_json::json!({"hooks": {"PreToolUse": [
            {"hooks": [{"type": "command", "command": format!("{} pre-tool-use", artifact.display())}]},
            {"hooks": [{"type": "command", "command": "other-tool"}]}
        ]}});
        let paths = vec![
            self.project.join(".codex/hooks.json"),
            self.home.join(".codex/hooks.json"),
        ];
        for path in &paths {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, settings.to_string()).unwrap();
        }
        paths
    }
}

fn complete(session: &mut Session) -> Vec<u8> {
    let output = session.expect(Eof).unwrap().as_bytes().to_vec();
    let process = session.get_process_mut();
    assert_eq!(
        process.wait().unwrap(),
        WaitStatus::Exited(process.pid(), 0)
    );
    output
}

fn assert_plain(output: &[u8]) {
    let output = String::from_utf8_lossy(output);
    for sequence in ["\x1b[?1049h", "\x1b[?1049l", "\x1b[?25l", "\x1b[?25h"] {
        assert!(
            !output.contains(sequence),
            "terminal state changed: {output:?}"
        );
    }
}

#[test]
fn explicit_terminal_eof_cancels_without_writing_adapter_files() {
    let workspace = Workspace::new(false);
    let mut session = workspace.session(&["init", "--hook-only", "--harness", "pi"]);
    let plan = session.expect("Proceed? [Y/n] ").unwrap();
    assert_plain(plan.as_bytes());
    session.send(ControlCode::EndOfTransmission).unwrap();
    assert_plain(&complete(&mut session));
    assert!(
        !workspace.plugin().exists(),
        "EOF must not authorize installation"
    );
    assert!(!workspace.project.join(".pi/skills").exists());
    assert!(!workspace.home.join(".config").exists());
}

#[test]
fn detected_pi_prints_the_plan_and_accepts_the_plain_default() {
    let workspace = Workspace::new(true);
    let mut session = workspace.session(&["init", "--hook-only"]);
    let plan = session.expect("Proceed? [Y/n] ").unwrap();
    let output = String::from_utf8_lossy(plan.as_bytes());
    assert!(output.contains("detected"), "{output}");
    assert!(output.contains(".pi/extensions/ironlint.ts"), "{output}");
    assert!(
        output.contains(".pi/skills/ironlint-config/SKILL.md"),
        "{output}"
    );
    assert_plain(plan.as_bytes());
    session.send_line("").unwrap();
    assert_plain(&complete(&mut session));
    assert!(workspace.plugin().exists());
    assert!(workspace
        .project
        .join(".pi/skills/ironlint-config/SKILL.md")
        .exists());
}

#[test]
fn undetected_pi_requires_an_affirmative_answer_to_the_printed_plan() {
    for answer in ["", "no", "cancel", "yes"] {
        let workspace = Workspace::new(false);
        let mut session = workspace.session(&["init", "--hook-only"]);
        let plan = session.expect("Proceed? [y/N] ").unwrap();
        let output = String::from_utf8_lossy(plan.as_bytes());
        assert!(output.contains(".pi/extensions/ironlint.ts"), "{output}");
        assert_plain(plan.as_bytes());
        session.send_line(answer).unwrap();
        assert_plain(&complete(&mut session));
        assert_eq!(
            workspace.plugin().exists(),
            answer == "yes",
            "answer {answer:?}"
        );
    }
}

#[test]
fn cancellation_declines_the_printed_git_hook_and_adapter_plan() {
    let workspace = Workspace::new(false);
    let hook = workspace.git_hook();
    let mut session = workspace.session(&["init", "--hook-only", "--harness", "pi", "--git-hook"]);
    let plan = session.expect("Proceed? [Y/n] ").unwrap();
    let output = String::from_utf8_lossy(plan.as_bytes());
    assert!(output.contains("git hook:"), "{output}");
    assert!(output.contains("would install"), "{output}");
    assert!(output.contains("pre-commit"), "{output}");
    session.send_line("no").unwrap();
    complete(&mut session);
    assert!(!hook.exists());
    assert!(!workspace.plugin().exists());
}

#[test]
fn terminal_dry_run_finishes_without_a_prompt_or_writes() {
    let workspace = Workspace::new(false);
    let hook = workspace.git_hook();
    let mut session = workspace.session(&["init", "--dry-run", "--git-hook"]);
    let output = complete(&mut session);
    assert_plain(&output);
    let output = String::from_utf8_lossy(&output);
    assert!(output.contains("would scaffold and trust"), "{output}");
    assert!(output.contains(".pi/extensions/ironlint.ts"), "{output}");
    assert!(!output.contains("Proceed?"), "{output}");
    assert!(!workspace.project.join(".ironlint.yml").exists());
    assert!(!workspace.home.join(".config").exists());
    assert!(!workspace.plugin().exists());
    assert!(!hook.exists());
}

#[test]
fn interactive_uninstall_confirms_owned_local_and_global_legacy_cleanup() {
    let workspace = Workspace::new(false);
    let settings = workspace.legacy_settings();
    for answer in ["no", "yes"] {
        let mut session = workspace.session(&["init", "--uninstall"]);
        let plan = session.expect("Proceed? [Y/n] ").unwrap();
        let output = String::from_utf8_lossy(plan.as_bytes());
        for path in ["./.codex/hooks.json", "~/.codex/hooks.json"] {
            assert!(output.contains(path), "{output}");
        }
        assert_plain(plan.as_bytes());
        session.send_line(answer).unwrap();
        complete(&mut session);
        for path in &settings {
            let contents = fs::read_to_string(path).unwrap();
            assert_eq!(contents.contains("ironlint/adapters/codex"), answer == "no");
            assert!(contents.contains("other-tool"));
        }
    }
}

#[test]
fn automatic_uninstall_does_not_select_an_unowned_harness_directory() {
    let workspace = Workspace::new(false);
    fs::create_dir(workspace.home.join(".codex")).unwrap();
    let mut session = workspace.session(&["init", "--uninstall"]);
    let output = complete(&mut session);
    assert!(!String::from_utf8_lossy(&output).contains("Proceed?"));
    assert!(workspace.home.join(".codex").is_dir());
    assert_eq!(fs::read_dir(&workspace.home).unwrap().count(), 1);
}
