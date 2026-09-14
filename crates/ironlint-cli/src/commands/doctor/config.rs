use std::path::Path;

use super::{CheckResult, DoctorContext, Status};

pub(super) enum ConfigSnapshot {
    Missing,
    Loaded(crate::commands::config::ReadOnlyConfig),
    Failed(anyhow::Error),
}

pub(super) fn load_config_snapshot(config_path: &Path) -> ConfigSnapshot {
    snapshot_from_load(
        config_path,
        crate::commands::config::load_read_only(config_path),
    )
}

fn snapshot_from_load(
    config_path: &Path,
    result: anyhow::Result<crate::commands::config::ReadOnlyConfig>,
) -> ConfigSnapshot {
    match result {
        Ok(config) => ConfigSnapshot::Loaded(config),
        Err(error)
            if error.downcast_ref::<std::io::Error>().is_some_and(|error| {
                matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                )
            }) && matches!(
                std::fs::symlink_metadata(config_path),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    )
            ) =>
        {
            ConfigSnapshot::Missing
        }
        Err(error) => ConfigSnapshot::Failed(error),
    }
}

#[allow(dead_code)]
pub(super) fn check_config_present(ctx: &DoctorContext) -> CheckResult {
    let snapshot = load_config_snapshot(&ctx.config_path);
    check_config_present_snapshot(ctx, &snapshot)
}

pub(super) fn check_config_present_snapshot(
    ctx: &DoctorContext,
    snapshot: &ConfigSnapshot,
) -> CheckResult {
    match snapshot {
        ConfigSnapshot::Missing => CheckResult {
            name: "config",
            status: Status::Fail,
            detail: format!("{} not found", ctx.config_path.display()),
            remediation: Some("run `ironlint init` to scaffold a starter config".into()),
        },
        ConfigSnapshot::Loaded(_) | ConfigSnapshot::Failed(_) => CheckResult {
            name: "config",
            status: Status::Pass,
            detail: format!("{} exists", ctx.config_path.display()),
            remediation: None,
        },
    }
}

#[allow(dead_code)]
pub(super) fn check_config_parses(ctx: &DoctorContext) -> CheckResult {
    let snapshot = load_config_snapshot(&ctx.config_path);
    check_config_parses_snapshot(ctx, &snapshot)
}

pub(super) fn check_config_parses_snapshot(
    _ctx: &DoctorContext,
    snapshot: &ConfigSnapshot,
) -> CheckResult {
    match snapshot {
        ConfigSnapshot::Missing => CheckResult {
            name: "parses",
            status: Status::Fail,
            detail: "config missing; nothing to parse".into(),
            remediation: Some("run `ironlint init` first".into()),
        },
        ConfigSnapshot::Loaded(crate::commands::config::ReadOnlyConfig::Legacy {
            config, ..
        }) => CheckResult {
            name: "parses",
            status: Status::Pass,
            detail: format!("config parses ({} check(s))", config.checks.len()),
            remediation: None,
        },
        ConfigSnapshot::Loaded(crate::commands::config::ReadOnlyConfig::V1(config)) => {
            CheckResult {
                name: "parses",
                status: Status::Pass,
                detail: format!("config parses ({} check(s))", config.checks.len()),
                remediation: None,
            }
        }
        ConfigSnapshot::Failed(e) => CheckResult {
            name: "parses",
            status: Status::Fail,
            detail: format!("{e:#}"),
            remediation: Some("fix the YAML error above and re-run".into()),
        },
    }
}

/// For each check whose `run` is a single token (no spaces) that starts with
/// `.ironlint/scripts/`, check that the path exists and is executable. Inline commands
/// (e.g. `grep -q TODO && exit 2`) are skipped — detection: `run` contains a
/// space or doesn't look like a file path.
#[allow(dead_code)]
pub(super) fn check_script_paths(ctx: &DoctorContext) -> CheckResult {
    let snapshot = load_config_snapshot(&ctx.config_path);
    check_script_paths_snapshot(ctx, &snapshot)
}

pub(super) fn check_script_paths_snapshot(
    ctx: &DoctorContext,
    snapshot: &ConfigSnapshot,
) -> CheckResult {
    let (check_count, bad) = match snapshot {
        ConfigSnapshot::Loaded(crate::commands::config::ReadOnlyConfig::Legacy {
            config, ..
        }) => {
            let mut bad = Vec::new();
            for (id, check) in &config.checks {
                for step in check.effective_steps() {
                    if let Some(issue) = check_run_path(&ctx.dir, id, &step.run) {
                        bad.push(issue);
                    }
                }
            }
            (config.checks.len(), bad)
        }
        ConfigSnapshot::Loaded(crate::commands::config::ReadOnlyConfig::V1(config)) => {
            let mut bad = Vec::new();
            for (id, check) in &config.checks {
                if let Some(issue) = check_run_path(&ctx.dir, id, &check.run) {
                    bad.push(issue);
                }
            }
            (config.checks.len(), bad)
        }
        ConfigSnapshot::Missing | ConfigSnapshot::Failed(_) => {
            return CheckResult {
                name: "check_scripts",
                status: Status::Warn,
                detail: "skipped (config does not parse)".into(),
                remediation: None,
            };
        }
    };
    if bad.is_empty() {
        CheckResult {
            name: "check_scripts",
            status: Status::Pass,
            detail: format!("{} check(s) checked", check_count),
            remediation: None,
        }
    } else {
        CheckResult {
            name: "check_scripts",
            status: Status::Fail,
            detail: format!("missing/non-executable check script(s): {}", bad.join("; ")),
            remediation: Some(
                "ensure check scripts exist under .ironlint/scripts/ and are executable (chmod +x)"
                    .into(),
            ),
        }
    }
}

/// Returns `Some(problem description)` if `run` looks like a script path that
/// is missing or not executable; `None` if the command is inline or the script
/// is fine.
pub(super) fn check_run_path(dir: &Path, check_id: &str, run: &str) -> Option<String> {
    // Inline command: contains a space → skip.
    if run.contains(' ') {
        return None;
    }
    // Only check paths that look like they're under .ironlint/scripts/
    if !run.starts_with(".ironlint/scripts/") {
        return None;
    }
    let script = dir.join(run);
    if !script.exists() {
        return Some(format!("{check_id}: {run} not found"));
    }
    // Check executable bit (Unix only; on Windows always passes).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&script) {
            if meta.permissions().mode() & 0o111 == 0 {
                return Some(format!("{check_id}: {run} not executable"));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn doctor_rows_use_one_snapshot_after_policy_replacement() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".ironlint.yml");
        fs::write(
            &config_path,
            "version: 1\nchecks:\n  check:\n    run: exit 0\n",
        )
        .unwrap();
        let snapshot = load_config_snapshot(&config_path);
        fs::write(
            &config_path,
            "version: 1\nchecks:\n  broken:\n    on: [change]\n    run: exit 0\n",
        )
        .unwrap();
        let ctx = DoctorContext {
            dir: dir.path().to_path_buf(),
            config_path,
        };

        assert_eq!(
            check_config_parses_snapshot(&ctx, &snapshot).status,
            Status::Pass
        );
        assert_eq!(
            check_script_paths_snapshot(&ctx, &snapshot).status,
            Status::Pass
        );
    }

    #[test]
    fn doctor_rows_use_one_snapshot_after_config_deletion() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".ironlint.yml");
        fs::write(
            &config_path,
            "version: 1\nchecks:\n  check:\n    run: exit 0\n",
        )
        .unwrap();
        let snapshot = load_config_snapshot(&config_path);
        fs::remove_file(&config_path).unwrap();
        let ctx = DoctorContext {
            dir: dir.path().to_path_buf(),
            config_path,
        };

        assert_eq!(
            check_config_present_snapshot(&ctx, &snapshot).status,
            Status::Pass
        );
        assert_eq!(
            check_config_parses_snapshot(&ctx, &snapshot).status,
            Status::Pass
        );
        assert_eq!(
            check_script_paths_snapshot(&ctx, &snapshot).status,
            Status::Pass
        );
    }

    #[test]
    fn doctor_rows_treat_not_found_during_load_as_missing() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".ironlint.yml");
        fs::write(
            &config_path,
            "version: 1\nchecks:\n  check:\n    run: exit 0\n",
        )
        .unwrap();
        fs::remove_file(&config_path).unwrap();
        let snapshot = snapshot_from_load(
            &config_path,
            crate::commands::config::load_read_only(&config_path),
        );
        let ctx = DoctorContext {
            dir: dir.path().to_path_buf(),
            config_path,
        };

        assert_eq!(
            check_config_present_snapshot(&ctx, &snapshot).status,
            Status::Fail
        );
        assert_eq!(
            check_config_parses_snapshot(&ctx, &snapshot).status,
            Status::Fail
        );
        assert_eq!(
            check_script_paths_snapshot(&ctx, &snapshot).status,
            Status::Warn
        );
    }

    #[test]
    fn doctor_reports_missing_extended_policy_without_claiming_root_absent() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".ironlint.yml");
        fs::write(&config_path, "extends: [missing.yml]\nchecks: {}\n").unwrap();
        let snapshot = load_config_snapshot(&config_path);
        let ctx = DoctorContext {
            dir: dir.path().to_path_buf(),
            config_path,
        };

        let present = check_config_present_snapshot(&ctx, &snapshot);
        assert_eq!(present.status, Status::Pass);
        assert!(present.remediation.is_none());
        let parses = check_config_parses_snapshot(&ctx, &snapshot);
        assert_eq!(parses.status, Status::Fail);
        assert!(parses.detail.contains("missing.yml"), "{}", parses.detail);
    }

    #[test]
    fn doctor_rows_treat_not_a_directory_during_load_as_missing() {
        let dir = tempdir().unwrap();
        let parent = dir.path().join("not-a-directory");
        fs::write(&parent, "not a directory").unwrap();
        let config_path = parent.join(".ironlint.yml");
        let snapshot = load_config_snapshot(&config_path);
        let ctx = DoctorContext {
            dir: dir.path().to_path_buf(),
            config_path,
        };

        assert_eq!(
            check_config_present_snapshot(&ctx, &snapshot).status,
            Status::Fail
        );
        assert_eq!(
            check_config_parses_snapshot(&ctx, &snapshot).status,
            Status::Fail
        );
        assert_eq!(
            check_script_paths_snapshot(&ctx, &snapshot).status,
            Status::Warn
        );
    }
}
