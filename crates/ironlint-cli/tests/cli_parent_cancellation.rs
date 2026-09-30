#![cfg(unix)]

use assert_cmd::cargo::CommandCargoExt;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn command(root: &Path, home: &Path) -> Command {
    let mut command = Command::cargo_bin("ironlint").unwrap();
    command
        .current_dir(root)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"));
    command
}

#[test]
fn stdin_close_cancels_active_independent_check_and_remaining_selection() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let config = root.path().join(".ironlint.yml");
    fs::write(&config, "version: 1\nchecks:\n  a:\n    run: 'printf %s $$ > owned.pid; exec sleep 30'\n  b:\n    run: 'touch skipped'\n").unwrap();
    assert!(command(root.path(), home.path())
        .args(["trust", "--config"])
        .arg(&config)
        .output()
        .unwrap()
        .status
        .success());
    let mut child = command(root.path(), home.path())
        .args([
            "check",
            "--format",
            "json",
            "--cancel-on-stdin-close",
            "--config",
        ])
        .arg(&config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let marker = root.path().join("owned.pid");
    let until = Instant::now() + Duration::from_secs(2);
    while !marker.exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    let pid = fs::read_to_string(&marker)
        .ok()
        .and_then(|s| s.parse::<i32>().ok());
    drop(child.stdin.take());
    let until = Instant::now() + Duration::from_secs(2);
    let mut finished = false;
    while Instant::now() < until {
        if child.try_wait().unwrap().is_some() {
            finished = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if !finished {
        let _ = child.kill();
        if let Some(pid) = pid {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        finished,
        "parent closure must cancel well before the 30-second command deadline"
    );
    let pid = pid.expect("check must start before cancellation");
    assert_eq!(
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
        Err(nix::errno::Errno::ESRCH)
    );
    assert_eq!(output.status.code(), Some(3));
    let verdict: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(verdict["schema"], 7);
    assert_eq!(verdict["error"], "execution_cancelled");
    assert_eq!(verdict["results"][0]["reason"], "execution_cancelled");
    assert_eq!(verdict["not_run"][0]["id"], "b");
    assert_eq!(verdict["not_run"][0]["reason"], "execution_cancelled");
    assert!(!root.path().join("skipped").exists());
}

#[test]
fn ordinary_closed_stdin_preserves_opt_out_execution() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let config = root.path().join(".ironlint.yml");
    fs::write(
        &config,
        "version: 1\nchecks:\n  a: {run: 'test -z \"$(cat)\"'}\n",
    )
    .unwrap();
    assert!(command(root.path(), home.path())
        .args(["trust", "--config"])
        .arg(&config)
        .output()
        .unwrap()
        .status
        .success());
    let output = command(root.path(), home.path())
        .args(["check", "--format", "json", "--config"])
        .arg(config)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["status"],
        "pass"
    );
}
