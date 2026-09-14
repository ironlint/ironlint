mod common;

use assert_cmd::Command;
use std::fs;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
use std::time::Instant;
use tempfile::tempdir;

#[test]
fn bare_check_reports_untrusted_v1_as_schema_seven() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(&cfg, "version: 1\nchecks:\n  all: {run: 'exit 0'}\n").unwrap();
    let xdg = tempdir().unwrap();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(4),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 7);
    assert_eq!(value["event"], "accept");
    assert_eq!(value["status"], "error");
}

#[test]
fn trust_then_check_runs_a_v1_policy() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(&cfg, "version: 1\nchecks:\n  all: {run: 'exit 0'}\n").unwrap();
    let xdg = tempdir().unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .args(["trust", "--config", cfg.to_str().unwrap()])
        .env("XDG_CONFIG_HOME", xdg.path())
        .assert()
        .success();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 7);
    assert_eq!(value["status"], "pass");
}

#[test]
fn change_without_files_treats_paths_as_unknown_but_files_filter() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(
        &cfg,
        "version: 1\nchecks:\n  scoped: {files: src/**, on: [change, accept], run: 'exit 0'}\n",
    )
    .unwrap();
    let xdg = tempdir().unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .args(["trust", "--config", cfg.to_str().unwrap()])
        .env("XDG_CONFIG_HOME", xdg.path())
        .assert()
        .success();

    let unknown = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--event",
            "change",
            "--format",
            "json",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .output()
        .unwrap();
    assert_eq!(unknown.status.code(), Some(0));
    let unknown: serde_json::Value = serde_json::from_slice(&unknown.stdout).unwrap();
    assert_eq!(unknown["status"], "pass");
    assert_eq!(unknown["results"][0]["id"], "scoped");

    let filtered = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--event",
            "change",
            "--file",
            "README.md",
            "--format",
            "json",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .output()
        .unwrap();
    assert_eq!(filtered.status.code(), Some(0));
    let filtered: serde_json::Value = serde_json::from_slice(&filtered.stdout).unwrap();
    assert_eq!(filtered["status"], "not_run");
    assert!(filtered["results"].as_array().unwrap().is_empty());
}

#[test]
fn human_v1_output_includes_diagnostics_and_truncation() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(
        &cfg,
        "version: 1\nchecks:\n  noisy: {run: \"printf stdout-diagnostic; head -c 100000 /dev/zero; printf stderr-diagnostic >&2; exit 1\"}\n",
    )
    .unwrap();
    let xdg = tempdir().unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .args(["trust", "--config", cfg.to_str().unwrap()])
        .env("XDG_CONFIG_HOME", xdg.path())
        .assert()
        .success();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("noisy: violation"), "stderr: {stderr}");
    assert!(stderr.contains("stdout-diagnostic"), "stderr: {stderr}");
    assert!(stderr.contains("stderr-diagnostic"), "stderr: {stderr}");
    assert!(stderr.contains("stdout: truncated"), "stderr: {stderr}");
}

#[test]
fn legacy_config_rejects_v1_events() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("legacy.yml");
    fs::write(&cfg, "checks:\n  all:\n    run: 'exit 0'\n").unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--event",
            "accept",
        ])
        .assert()
        .code(1);
}

#[test]
fn malformed_v1_config_keeps_schema_seven_errors() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(&cfg, "\"version\": 1\nchecks: [\n").unwrap();
    let xdg = tempdir().unwrap();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 7);
    assert_eq!(value["status"], "error");
}

#[test]
fn indented_malformed_v1_config_keeps_schema_seven_errors() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(&cfg, "  version: 1\n  checks: [\n").unwrap();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--event",
            "accept",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 7);
    assert_eq!(value["event"], "accept");
    assert_eq!(value["status"], "error");
}

#[test]
fn invalid_utf8_v1_marker_keeps_schema_seven_error() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(
        &cfg,
        b"version: 1 # \xff\nchecks:\n  all: {run: 'exit 0'}\n",
    )
    .unwrap();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 7);
    assert_eq!(value["event"], "accept");
    assert_eq!(value["status"], "error");
}

#[test]
fn invalid_utf8_v1_crlf_marker_keeps_schema_seven_error() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(
        &cfg,
        b"version: 1\r\nchecks:\r\n  all: {run: 'exit 0'}\r\n\xff",
    )
    .unwrap();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 7);
    assert_eq!(value["event"], "accept");
    assert_eq!(value["status"], "error");
}

#[test]
#[cfg(unix)]
fn v1_missing_shell_is_an_execution_error() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(&cfg, "version: 1\nchecks:\n  all: {run: 'exit 0'}\n").unwrap();
    let xdg = common::blessed_store(&cfg);
    let empty_path = tempdir().unwrap();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .env("PATH", empty_path.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(3));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 7);
    assert_eq!(value["status"], "error");
    assert_eq!(value["results"][0]["outcome"], "error");
    assert_eq!(value["results"][0]["reason"], "spawn");
}

#[test]
fn v1_rejects_unsupported_event_before_trust() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(&cfg, "version: 1\nchecks:\n  all: {run: 'exit 0'}\n").unwrap();
    let xdg = tempdir().unwrap();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--event",
            "write",
            "--format",
            "json",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 7);
    assert_eq!(value["event"], "write");
    assert_eq!(value["status"], "error");
}

#[test]
#[cfg(unix)]
fn v1_shell_preflight_is_bounded_and_filters_marker() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(
        &cfg,
        "version: 1\nexecution:\n  timeout_secs: 1\n  total_timeout_secs: 1\nchecks:\n  env: {run: 'test -z \"$IRONLINT_V1_PREFLIGHT_MARKER\"'}\n",
    )
    .unwrap();
    let xdg = common::blessed_store(&cfg);
    let shell_dir = tempdir().unwrap();
    let log = dir.path().join("shell.log");
    let shell = shell_dir.path().join("sh");
    fs::write(
        &shell,
        format!(
            "#!/bin/sh\nif [ -n \"$IRONLINT_V1_PREFLIGHT_MARKER\" ]; then printf 'marker\\n' >> '{}'; /bin/sleep 2; else printf 'clean\\n' >> '{}'; fi\nexec /bin/sh \"$@\"\n",
            log.display(),
            log.display(),
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&shell).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&shell, permissions).unwrap();

    let started = Instant::now();
    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .env("PATH", shell_dir.path())
        .env(
            "IRONLINT_V1_PREFLIGHT_MARKER",
            "inherited-only-by-preflight",
        )
        .output()
        .unwrap();
    let elapsed = started.elapsed();

    assert_eq!(output.status.code(), Some(0));
    assert!(
        elapsed < std::time::Duration::from_millis(1500),
        "elapsed: {elapsed:?}"
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "pass");
    assert_eq!(fs::read_to_string(log).unwrap(), "clean\n");
}

#[test]
#[cfg(unix)]
fn v1_validates_missing_path_ancestors_before_accepting_changes() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let outside_nested = outside.path().join("nested");
    fs::create_dir(&outside_nested).unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(
        &cfg,
        "version: 1\nchecks:\n  all: {on: [change, accept], run: 'exit 0'}\n",
    )
    .unwrap();
    symlink(&outside_nested, dir.path().join("link")).unwrap();
    symlink(
        outside.path().join("does-not-exist"),
        dir.path().join("broken"),
    )
    .unwrap();
    let xdg = common::blessed_store(&cfg);

    let check = |file: &str| {
        Command::cargo_bin("ironlint")
            .unwrap()
            .args([
                "check",
                "--config",
                cfg.to_str().unwrap(),
                "--root",
                dir.path().to_str().unwrap(),
                "--event",
                "change",
                "--file",
                file,
                "--format",
                "json",
            ])
            .env("XDG_CONFIG_HOME", xdg.path())
            .output()
            .unwrap()
    };

    let in_root = check("missing/deleted.rs");
    assert_eq!(in_root.status.code(), Some(0));
    let in_root: serde_json::Value = serde_json::from_slice(&in_root.stdout).unwrap();
    assert_eq!(in_root["status"], "pass");

    for file in ["link/deleted.rs", "link/../deleted.rs", "broken/deleted.rs"] {
        let output = check(file);
        assert_eq!(output.status.code(), Some(1), "file: {file}");
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["schema"], 7, "file: {file}");
        assert_eq!(value["status"], "error", "file: {file}");
        assert!(
            value["error"]
                .as_str()
                .is_some_and(|reason| reason.contains("file path escapes --root")),
            "file: {file}, value: {value}"
        );
    }
}

#[test]
fn explicit_v1_discovery_and_usage_errors_have_schema_seven_json() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("missing-policy.yml");

    let discovery = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            missing.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--event",
            "accept",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(discovery.status.code(), Some(1));
    let discovery: serde_json::Value = serde_json::from_slice(&discovery.stdout).unwrap();
    assert_eq!(discovery["schema"], 7);
    assert_eq!(discovery["event"], "accept");
    assert_eq!(discovery["status"], "error");

    let usage = Command::cargo_bin("ironlint")
        .unwrap()
        .args(["check", "--event", "accept", "--format", "json", "--root"])
        .output()
        .unwrap();
    assert_eq!(usage.status.code(), Some(1));
    let usage: serde_json::Value = serde_json::from_slice(&usage.stdout).unwrap();
    assert_eq!(usage["schema"], 7);
    assert_eq!(usage["event"], "accept");
    assert_eq!(usage["status"], "error");

    let help = Command::cargo_bin("ironlint")
        .unwrap()
        .args(["check", "--event", "accept", "--format", "json", "--help"])
        .output()
        .unwrap();
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage:"));

    let duplicate_format = Command::cargo_bin("ironlint")
        .unwrap()
        .args(["check", "--event", "accept", "--format", "json", "--format"])
        .output()
        .unwrap();
    assert_eq!(duplicate_format.status.code(), Some(1));
    let duplicate_format: serde_json::Value =
        serde_json::from_slice(&duplicate_format.stdout).unwrap();
    assert_eq!(duplicate_format["schema"], 7);
    assert_eq!(duplicate_format["event"], "accept");
    assert_eq!(duplicate_format["status"], "error");

    #[cfg(unix)]
    {
        let non_unicode = Command::cargo_bin("ironlint")
            .unwrap()
            .args(["check", "--event", "accept", "--format", "json"])
            .arg(std::ffi::OsString::from_vec(vec![0xFF]))
            .output()
            .unwrap();
        assert_eq!(non_unicode.status.code(), Some(1));
        let non_unicode: serde_json::Value = serde_json::from_slice(&non_unicode.stdout).unwrap();
        assert_eq!(non_unicode["schema"], 7);
        assert_eq!(non_unicode["event"], "accept");
        assert_eq!(non_unicode["status"], "error");
    }
}

#[test]
fn default_v1_usage_errors_have_schema_seven_json() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(&cfg, "version: 1\nchecks:\n  all: {run: 'exit 0'}\n").unwrap();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
            "--bogus",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 7);
    assert_eq!(value["event"], "accept");
    assert_eq!(value["status"], "error");
}

#[test]
fn legacy_usage_errors_do_not_emit_schema_seven() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("legacy.yml");
    fs::write(
        &cfg,
        "checks:\n  all:\n    files: '**/*'\n    run: 'exit 0'\n",
    )
    .unwrap();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--event",
            "accept",
            "--format",
            "json",
            "--bogus",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

#[test]
fn invalid_v1_event_usage_errors_have_schema_seven_json() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(&cfg, "version: 1\nchecks:\n  all: {run: 'exit 0'}\n").unwrap();

    for event_args in [vec!["--event", "nonsense"], vec!["--event"]] {
        let mut args = vec![
            "check".to_string(),
            "--config".to_string(),
            cfg.to_str().unwrap().to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        args.extend(event_args.iter().map(|arg| (*arg).to_string()));
        let output = Command::cargo_bin("ironlint")
            .unwrap()
            .args(args)
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(1));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["schema"], 7);
        assert_eq!(value["event"], "invalid");
        assert_eq!(value["status"], "error");
    }
}

#[test]
fn malformed_v1_event_flags_keep_schema_seven_context() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(&cfg, "version: 1\nchecks:\n  all: {run: 'exit 0'}\n").unwrap();

    for args in [
        vec![
            "check".to_string(),
            "--event".to_string(),
            "--format".to_string(),
            "json".to_string(),
            "--config".to_string(),
            cfg.to_str().unwrap().to_string(),
        ],
        vec![
            "check".to_string(),
            "--config".to_string(),
            cfg.to_str().unwrap().to_string(),
            "--format".to_string(),
            "json".to_string(),
            "--event".to_string(),
            "accept".to_string(),
            "--event=nonsense".to_string(),
        ],
    ] {
        let output = Command::cargo_bin("ironlint")
            .unwrap()
            .args(args)
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(1));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["schema"], 7);
        assert_eq!(value["event"], "invalid");
        assert_eq!(value["status"], "error");
    }
}

#[test]
fn v1_usage_event_scanner_stops_at_option_terminator() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(&cfg, "version: 1\nchecks:\n  all: {run: 'exit 0'}\n").unwrap();

    for args in [
        vec![
            "check".to_string(),
            "--config".to_string(),
            cfg.to_str().unwrap().to_string(),
            "--format".to_string(),
            "json".to_string(),
            "--".to_string(),
            "--event=change".to_string(),
        ],
        vec![
            "check".to_string(),
            "--config".to_string(),
            cfg.to_str().unwrap().to_string(),
            "--format".to_string(),
            "json".to_string(),
            "--event".to_string(),
            "accept".to_string(),
            "--".to_string(),
            "--event".to_string(),
            "change".to_string(),
        ],
    ] {
        let output = Command::cargo_bin("ironlint")
            .unwrap()
            .args(args)
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(1));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["schema"], 7);
        assert_eq!(value["event"], "accept");
        assert_eq!(value["status"], "error");
    }
}

#[test]
#[cfg(unix)]
fn non_unicode_v1_event_usage_error_is_not_default_accept() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(&cfg, "version: 1\nchecks:\n  all: {run: 'exit 0'}\n").unwrap();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--format",
            "json",
        ])
        .arg(OsString::from_vec(b"--event=\xff".to_vec()))
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema"], 7);
    assert_eq!(value["event"], "invalid");
    assert_eq!(value["status"], "error");
}

#[test]
fn validate_accepts_v1_policy() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(&cfg, "version: 1\nchecks:\n  all: {run: 'exit 0'}\n").unwrap();

    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "validate",
            "--config",
            cfg.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "ok");
    assert_eq!(value["checks"], 1);
}
