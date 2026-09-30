use crate::config::V1Event;
use crate::deadline::{self, TotalTimeout};
use crate::engine::{run_v1_until, V1ExecutionEnv, V1ExecutionError, V1ExecutionOutcome};
use crate::policy::PolicySnapshot;
use crate::trust::ApprovedPolicy;
use crate::verdict::{V1CheckOutcome, V1CheckResult, V1NotRun, V1Verdict};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Evaluate one approved v1 policy against the supplied tree.
///
/// The validated model belongs to the approved snapshot's exact bytes, and
/// the approved policy plus every managed script is
/// re-verified before each check runs. Drift stops the run fail-closed: the
/// remaining checks are reported `not_run` with reason `policy_changed` and no
/// unapproved bytes execute. Selection is performed once, in the config's
/// lexicographic check-id order; each selected command runs at most once.
pub fn evaluate_v1(
    approved: &ApprovedPolicy,
    root: &Path,
    event: &str,
    changed_paths: Option<&[PathBuf]>,
) -> Result<V1Verdict> {
    evaluate_v1_snapshot(
        approved.snapshot(),
        root,
        V1Event::parse(event)?,
        changed_paths,
    )
}

/// Evaluate a captured policy without consulting or writing CLI consent.
///
/// Selection uses the snapshot's validated model and compiled scopes. The
/// policy and managed scripts are verified before each command and afterward.
pub fn evaluate_v1_snapshot(
    snapshot: &PolicySnapshot,
    root: &Path,
    event: V1Event,
    changed_paths: Option<&[PathBuf]>,
) -> Result<V1Verdict> {
    evaluate_with(
        snapshot,
        root,
        event,
        changed_paths,
        &Instant::now,
        &|snapshot, deadline| snapshot.verify_unchanged_until(deadline),
    )
}

fn evaluate_with(
    snapshot: &PolicySnapshot,
    root: &Path,
    event: V1Event,
    changed_paths: Option<&[PathBuf]>,
    now: &dyn Fn() -> Instant,
    verify: &dyn Fn(&PolicySnapshot, Option<Instant>) -> Result<()>,
) -> Result<V1Verdict> {
    let config = snapshot.policy();
    let event_name = event.as_str();
    let start = now();
    let total_deadline = deadline::from_budget(
        start,
        Duration::from_secs(config.execution().total_timeout_secs()),
    );
    let selected = config.selected_ids(event, changed_paths);
    let mut results = Vec::with_capacity(selected.len());
    let mut not_run = Vec::new();
    let mut error = None;
    let Some(total_deadline) = total_deadline.filter(|until| now() < *until) else {
        mark_not_run(&mut not_run, &selected, "total_timeout");
        return Ok(V1Verdict::from_results(
            event_name,
            results,
            not_run,
            Some("total_timeout".into()),
        ));
    };
    let bin = ironlint_bin();

    for (index, id) in selected.iter().enumerate() {
        let check = &config.checks()[*id];
        if let Err(drift) = verify_current(snapshot, total_deadline, now, verify) {
            let (reason, detail) = verification_failure(&drift);
            mark_not_run(&mut not_run, &selected[index..], reason);
            error = Some(detail);
            break;
        }
        let execution = run_v1_until(
            check.run(),
            &V1ExecutionEnv {
                root,
                event: event_name,
                bin: &bin,
            },
            Duration::from_secs(check.effective_timeout_secs(config.execution())),
            total_deadline,
        );
        if execution.outcome == V1ExecutionOutcome::Error(V1ExecutionError::TotalTimeoutBeforeSpawn)
        {
            mark_not_run(&mut not_run, &selected[index..], "total_timeout");
            error = Some("total_timeout".into());
            break;
        }
        let failed = !matches!(
            &execution.outcome,
            V1ExecutionOutcome::Pass | V1ExecutionOutcome::Violation
        );
        let total_timed_out = now() >= total_deadline
            && matches!(
                execution.outcome,
                V1ExecutionOutcome::Error(V1ExecutionError::Timeout)
            );
        results.push(result_for(id, execution));
        if failed {
            if total_timed_out {
                mark_not_run(&mut not_run, &selected[index + 1..], "total_timeout");
                error = Some("total_timeout".to_string());
            } else {
                mark_not_run(&mut not_run, &selected[index + 1..], "execution_error");
                error = results.last().and_then(|result| result.reason.clone());
            }
            break;
        }
    }

    if error.is_none() && !selected.is_empty() {
        // Post-run: a check that edited the approved policy or scripts (or a
        // concurrent writer doing so) must never yield a clean `pass`.
        if let Err(drift) = verify_current(snapshot, total_deadline, now, verify) {
            error = Some(verification_failure(&drift).1);
        }
    }

    Ok(V1Verdict::from_results(event_name, results, not_run, error))
}

fn verify_current(
    snapshot: &PolicySnapshot,
    until: Instant,
    now: &dyn Fn() -> Instant,
    verify: &dyn Fn(&PolicySnapshot, Option<Instant>) -> Result<()>,
) -> Result<()> {
    deadline::check_at(Some(until), now())?;
    verify(snapshot, Some(until))?;
    deadline::check_at(Some(until), now())
}

fn verification_failure(error: &anyhow::Error) -> (&'static str, String) {
    if error.is::<TotalTimeout>() {
        ("total_timeout", "total_timeout".into())
    } else {
        ("policy_changed", format!("{error:#}"))
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
        V1ExecutionError::TotalTimeoutBeforeSpawn => "total_timeout".to_string(),
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
    fn d5_empty_selection_consuming_budget_is_total_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let policy = dir.path().join("policy.yml");
        std::fs::write(
            &policy,
            "version: 1\nchecks:\n  rust: {files: '*.rs', on: [change, accept], run: 'touch marker'}\n",
        )
        .unwrap();
        let snapshot = PolicySnapshot::load(&policy).unwrap();
        let start = Instant::now();
        let first_read = std::cell::Cell::new(true);
        let now = || {
            if first_read.replace(false) {
                start
            } else {
                start + Duration::from_secs(301)
            }
        };
        let verdict = evaluate_with(
            &snapshot,
            dir.path(),
            V1Event::Change,
            Some(&[PathBuf::from("README.md")]),
            &now,
            &|_, _| panic!("empty selection must not verify or execute a check"),
        )
        .unwrap();
        assert_eq!(verdict.status, crate::verdict::V1Status::Error);
        assert_eq!(verdict.error.as_deref(), Some("total_timeout"));
        assert!(verdict.results.is_empty());
        assert!(verdict.not_run.is_empty());
        assert!(!dir.path().join("marker").exists());
    }

    #[test]
    fn d5_verification_consuming_budget_never_starts_a_command() {
        let dir = tempfile::tempdir().unwrap();
        let policy = dir.path().join("policy.yml");
        std::fs::write(
            &policy,
            "version: 1\nchecks:\n  a: {run: 'touch marker'}\n  b: {run: 'exit 0'}\n",
        )
        .unwrap();
        let snapshot = PolicySnapshot::load(&policy).unwrap();
        let start = Instant::now();
        let clock = std::cell::Cell::new(start);
        let verify = |snapshot: &PolicySnapshot, _: Option<Instant>| -> Result<()> {
            snapshot.verify_unchanged()?;
            clock.set(start + Duration::from_secs(301));
            Ok(())
        };
        let verdict = evaluate_with(
            &snapshot,
            dir.path(),
            V1Event::Accept,
            None,
            &|| clock.get(),
            &verify,
        )
        .unwrap();
        assert!(!dir.path().join("marker").exists());
        assert!(verdict.results.is_empty());
        assert_eq!(verdict.error.as_deref(), Some("total_timeout"));
        assert_eq!(
            verdict
                .not_run
                .iter()
                .map(|n| n.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn d5_expired_final_verification_denies_pass_but_retains_results() {
        let dir = tempfile::tempdir().unwrap();
        let policy = dir.path().join("policy.yml");
        std::fs::write(&policy, "version: 1\nchecks:\n  a: {run: 'exit 0'}\n").unwrap();
        let snapshot = PolicySnapshot::load(&policy).unwrap();
        let start = Instant::now();
        let clock = std::cell::Cell::new(start);
        let count = std::cell::Cell::new(0);
        let verify = |snapshot: &PolicySnapshot, _: Option<Instant>| -> Result<()> {
            snapshot.verify_unchanged()?;
            count.set(count.get() + 1);
            if count.get() == 2 {
                clock.set(start + Duration::from_secs(301));
            }
            Ok(())
        };
        let verdict = evaluate_with(
            &snapshot,
            dir.path(),
            V1Event::Accept,
            None,
            &|| clock.get(),
            &verify,
        )
        .unwrap();
        assert_eq!(verdict.error.as_deref(), Some("total_timeout"));
        assert_eq!(verdict.results.len(), 1);
        assert_eq!(verdict.results[0].outcome, V1CheckOutcome::Pass);
        assert!(verdict.not_run.is_empty());
    }

    #[test]
    fn d5_unrepresentable_total_deadline_marks_checks_unexecuted() {
        let dir = tempfile::tempdir().unwrap();
        let policy = dir.path().join("policy.yml");
        std::fs::write(&policy, format!("version: 1\nexecution: {{total_timeout_secs: {}}}\nchecks:\n  a: {{run: 'touch marker'}}\n", u64::MAX)).unwrap();
        let snapshot = PolicySnapshot::load(&policy).unwrap();
        let verdict = evaluate_v1_snapshot(&snapshot, dir.path(), V1Event::Accept, None).unwrap();
        assert_eq!(verdict.error.as_deref(), Some("total_timeout"));
        assert!(verdict.results.is_empty());
        assert_eq!(verdict.not_run.len(), 1);
        assert!(!dir.path().join("marker").exists());
    }
    use crate::trust::{bless_in, check_trust_in, TrustOutcome};

    /// Build the approved snapshot the way the CLI does: bless into an
    /// isolated store, then verify and take the verified bytes.
    fn approve(dir: &Path, policy: &Path) -> ApprovedPolicy {
        let store = dir.join("trust-store.json");
        bless_in(policy, &store, "test").unwrap();
        match check_trust_in(policy, &store) {
            TrustOutcome::Trusted(approved) => approved,
            TrustOutcome::Untrusted(_) | TrustOutcome::Unverifiable(_) => {
                panic!("test policy must be trusted")
            }
        }
    }

    #[test]
    fn no_selected_change_checks_is_not_run_not_pass() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("policy.yml"),
            "version: 1\nchecks:\n  rust: {files: '**/*.rs', on: [change, accept], run: 'exit 0'}\n",
        )
        .unwrap();
        let policy = dir.path().join("policy.yml");
        let approved = approve(dir.path(), &policy);
        let verdict = evaluate_v1(
            &approved,
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
        let policy = dir.path().join("policy.yml");
        let approved = approve(dir.path(), &policy);
        let verdict = evaluate_v1(&approved, dir.path(), "accept", None).unwrap();
        assert_eq!(verdict.status, crate::verdict::V1Status::Error);
        assert_eq!(verdict.not_run[0].id, "b");
        assert!(!dir.path().join("b-ran").exists());
    }

    /// SECURITY REGRESSION: a check that rewrites a managed script must stop
    /// the run before the next check executes the unapproved bytes, even
    /// though the approved hash covered the original script.
    #[test]
    fn script_drift_between_checks_stops_execution() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ironlint/scripts")).unwrap();
        std::fs::write(
            dir.path().join(".ironlint/scripts/gate.sh"),
            "#!/bin/sh\nexit 0\n",
        )
        .unwrap();
        let policy = dir.path().join("policy.yml");
        std::fs::write(
            &policy,
            r#"version: 1
checks:
  a-rewrite:
    run: |
      cat > "$IRONLINT_ROOT/.ironlint/scripts/gate.sh" <<'EOF'
      #!/bin/sh
      touch "$IRONLINT_ROOT/gate-mutated-ran"
      exit 0
      EOF
  b-gate:
    run: sh "$IRONLINT_ROOT/.ironlint/scripts/gate.sh"
"#,
        )
        .unwrap();
        let approved = approve(dir.path(), &policy);
        let verdict = evaluate_v1(&approved, dir.path(), "accept", None).unwrap();
        assert_eq!(verdict.status, crate::verdict::V1Status::Error);
        assert_eq!(verdict.not_run.len(), 1);
        assert_eq!(verdict.not_run[0].id, "b-gate");
        assert_eq!(verdict.not_run[0].reason, "policy_changed");
        assert!(verdict.error.is_some());
        assert!(!dir.path().join("gate-mutated-ran").exists());
    }

    #[test]
    fn invalid_event_is_an_input_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("policy.yml"),
            "version: 1\nchecks:\n  check: {run: 'exit 0'}\n",
        )
        .unwrap();
        let policy = dir.path().join("policy.yml");
        let approved = approve(dir.path(), &policy);
        let error = evaluate_v1(&approved, dir.path(), "other", None).unwrap_err();
        assert!(error.to_string().contains("expected `change` or `accept`"));
    }

    #[test]
    fn execution_error_reasons_are_stable() {
        let cases = [
            (V1ExecutionError::NotFound, "not_found"),
            (V1ExecutionError::NotExecutable, "not_executable"),
            (V1ExecutionError::Timeout, "timeout"),
            (V1ExecutionError::DeadlineOverflow, "deadline_overflow"),
            (V1ExecutionError::TotalTimeoutBeforeSpawn, "total_timeout"),
            (V1ExecutionError::Signal(9), "signal"),
            (V1ExecutionError::HighExit(200), "high_exit"),
            (V1ExecutionError::Spawn("spawn failed".into()), "spawn"),
        ];
        for (error, expected) in cases {
            assert_eq!(execution_error_reason(&error), expected);
        }
    }
}
