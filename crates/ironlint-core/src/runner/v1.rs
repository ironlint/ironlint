use crate::config::v1::{parse_v1_file, V1Event};
use crate::engine::{run_v1, V1ExecutionEnv, V1ExecutionError, V1ExecutionOutcome};
use crate::verdict::{V1CheckOutcome, V1CheckResult, V1NotRun, V1Verdict};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Evaluate one v1 policy against the supplied tree. Selection is performed
/// once, in the config's lexicographic check-id order; each selected command
/// runs at most once.
pub fn evaluate_v1(
    policy: &Path,
    root: &Path,
    event: &str,
    changed_paths: Option<&[PathBuf]>,
) -> Result<V1Verdict> {
    let config = parse_v1_file(policy)?;
    let event = parse_event(event)?;
    let event_name = event_name(event);
    let selected = config.selected_ids(event, changed_paths);
    let start = Instant::now();
    let total_deadline =
        start.checked_add(Duration::from_secs(config.execution.total_timeout_secs));
    let mut results = Vec::with_capacity(selected.len());
    let mut not_run = Vec::new();
    let mut error = None;
    let bin = ironlint_bin();

    for (index, id) in selected.iter().enumerate() {
        let Some(total_deadline) = total_deadline else {
            mark_not_run(&mut not_run, &selected[index..], "total_timeout");
            error = Some("total_timeout".to_string());
            break;
        };
        let remaining = total_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            mark_not_run(&mut not_run, &selected[index..], "total_timeout");
            error = Some("total_timeout".to_string());
            break;
        }

        let check = &config.checks[*id];
        let execution = run_v1(
            &check.run,
            &V1ExecutionEnv {
                root,
                event: event_name,
                bin: &bin,
            },
            Duration::from_secs(config.execution.timeout_secs),
            remaining,
        );
        let failed = !matches!(
            &execution.outcome,
            V1ExecutionOutcome::Pass | V1ExecutionOutcome::Violation
        );
        results.push(result_for(id, execution));
        if failed {
            mark_not_run(&mut not_run, &selected[index + 1..], "execution_error");
            error = results.last().and_then(|result| result.reason.clone());
            break;
        }
    }

    Ok(V1Verdict::from_results(event_name, results, not_run, error))
}

fn event_name(event: V1Event) -> &'static str {
    match event {
        V1Event::Change => "change",
        V1Event::Accept => "accept",
    }
}

fn parse_event(event: &str) -> Result<V1Event> {
    match event {
        "change" => Ok(V1Event::Change),
        "accept" => Ok(V1Event::Accept),
        _ => bail!("invalid v1 event `{event}`; expected `change` or `accept`"),
    }
}

fn ironlint_bin() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ironlint"))
}

fn result_for(id: &str, execution: crate::engine::V1ExecutionResult) -> V1CheckResult {
    let (outcome, reason) = match execution.outcome {
        V1ExecutionOutcome::Pass => (V1CheckOutcome::Pass, None),
        V1ExecutionOutcome::Violation => (V1CheckOutcome::Violation, None),
        V1ExecutionOutcome::Error(error) => {
            let reason = execution_error_reason(&error);
            (V1CheckOutcome::Error, Some(reason))
        }
    };
    V1CheckResult {
        id: id.to_string(),
        outcome,
        exit_status: execution.exit_code,
        stdout: execution.stdout,
        stderr: execution.stderr,
        stdout_truncated: execution.stdout_truncated,
        stderr_truncated: execution.stderr_truncated,
        reason,
    }
}

fn execution_error_reason(error: &V1ExecutionError) -> String {
    match error {
        V1ExecutionError::NotFound => "not_found".to_string(),
        V1ExecutionError::NotExecutable => "not_executable".to_string(),
        V1ExecutionError::Timeout => "timeout".to_string(),
        V1ExecutionError::DeadlineOverflow => "deadline_overflow".to_string(),
        V1ExecutionError::Signal(_) => "signal".to_string(),
        V1ExecutionError::HighExit(_) => "high_exit".to_string(),
        V1ExecutionError::Spawn(_) => "spawn".to_string(),
    }
}

fn mark_not_run(not_run: &mut Vec<V1NotRun>, ids: &[&str], reason: &str) {
    not_run.extend(ids.iter().map(|id| V1NotRun {
        id: (*id).to_string(),
        reason: reason.to_string(),
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_selected_change_checks_is_not_run_not_pass() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("policy.yml"),
            "version: 1\nchecks:\n  rust: {files: '**/*.rs', on: [change, accept], run: 'exit 0'}\n",
        )
        .unwrap();
        let verdict = evaluate_v1(
            &dir.path().join("policy.yml"),
            dir.path(),
            "change",
            Some(&[PathBuf::from("README.md")]),
        )
        .unwrap();
        assert_eq!(verdict.status, crate::verdict::V1Status::NotRun);
        assert!(verdict.results.is_empty());
    }

    #[test]
    fn execution_error_stops_and_marks_remaining_checks() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("policy.yml"),
            "version: 1\nchecks:\n  a: {run: 'definitely-not-a-real-command-ironlint-v1'}\n  b: {run: 'touch b-ran'}\n",
        )
        .unwrap();
        let verdict =
            evaluate_v1(&dir.path().join("policy.yml"), dir.path(), "accept", None).unwrap();
        assert_eq!(verdict.status, crate::verdict::V1Status::Error);
        assert_eq!(verdict.not_run[0].id, "b");
        assert!(!dir.path().join("b-ran").exists());
    }

    #[test]
    fn invalid_event_is_an_input_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("policy.yml"),
            "version: 1\nchecks:\n  check: {run: 'exit 0'}\n",
        )
        .unwrap();
        let error =
            evaluate_v1(&dir.path().join("policy.yml"), dir.path(), "other", None).unwrap_err();
        assert!(error.to_string().contains("expected `change` or `accept`"));
    }

    #[test]
    fn execution_error_reasons_are_stable() {
        let cases = [
            (V1ExecutionError::NotFound, "not_found"),
            (V1ExecutionError::NotExecutable, "not_executable"),
            (V1ExecutionError::Timeout, "timeout"),
            (V1ExecutionError::DeadlineOverflow, "deadline_overflow"),
            (V1ExecutionError::Signal(9), "signal"),
            (V1ExecutionError::HighExit(200), "high_exit"),
            (V1ExecutionError::Spawn("spawn failed".into()), "spawn"),
        ];
        for (error, expected) in cases {
            assert_eq!(execution_error_reason(&error), expected);
        }
    }
}
