use ironlint_core::config::{parse_v1_str, SelectionDecision, V1Event};
use ironlint_core::policy::PolicySnapshot;
use ironlint_core::runner::{evaluate_v1, evaluate_v1_snapshot};
use ironlint_core::trust::{bless_in, check_trust_in, TrustOutcome};
use ironlint_core::verdict::V1Status;
use std::fs;
use std::path::PathBuf;

const POLICY: &str = "version: 1\nchecks:\n  accept_only: {run: 'true'}\n  scoped: {files: '*.rs', on: [change, accept], run: 'true'}\n  unconditional: {on: [change, accept], run: 'true'}\n";

#[test]
fn selection_decisions_and_execution_agree_for_every_path_state() {
    use SelectionDecision::*;
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join(".ironlint.yml");
    fs::write(&config, POLICY).unwrap();
    let snapshot = PolicySnapshot::load(&config).unwrap();
    let cases = [
        (V1Event::Accept, None, [Acceptance, Acceptance, Acceptance]),
        (
            V1Event::Accept,
            Some(vec![]),
            [Acceptance, Acceptance, Acceptance],
        ),
        (
            V1Event::Change,
            None,
            [ChangeDisabled, UnknownPaths, Unscoped],
        ),
        (
            V1Event::Change,
            Some(vec![]),
            [ChangeDisabled, EmptyPaths, Unscoped],
        ),
        (
            V1Event::Change,
            Some(vec![PathBuf::from("deep/src/a.rs")]),
            [ChangeDisabled, Matched, Unscoped],
        ),
        (
            V1Event::Change,
            Some(vec![PathBuf::from("readme.md")]),
            [ChangeDisabled, Unmatched, Unscoped],
        ),
        (
            V1Event::Change,
            Some(vec![PathBuf::from("a.rs"), PathBuf::from("a.rs")]),
            [ChangeDisabled, Matched, Unscoped],
        ),
    ];
    for (event, paths, expected) in cases {
        let rows = snapshot.policy().selection(event, paths.as_deref());
        assert_eq!(
            rows.iter().map(|row| row.decision()).collect::<Vec<_>>(),
            expected
        );
        let selected: Vec<_> = rows
            .iter()
            .filter(|row| row.decision().is_selected())
            .map(|row| row.id())
            .collect();
        let verdict = evaluate_v1_snapshot(&snapshot, dir.path(), event, paths.as_deref()).unwrap();
        assert_eq!(verdict.status, V1Status::Pass);
        assert_eq!(
            verdict
                .results
                .iter()
                .map(|result| result.id.as_str())
                .collect::<Vec<_>>(),
            selected
        );
    }
    assert!(!dir.path().join("trust.json").exists());
    assert_eq!(fs::read_to_string(config).unwrap(), POLICY);
}

#[test]
fn public_validated_model_roundtrips_with_defaults_and_getters() {
    let model = parse_v1_str(POLICY).unwrap();
    assert_eq!(model.version(), 1);
    assert_eq!(model.execution().timeout_secs(), 30);
    assert_eq!(model.execution().total_timeout_secs(), 300);
    assert_eq!(model.checks()["accept_only"].on(), &[V1Event::Accept]);
    assert_eq!(model.checks()["scoped"].files().unwrap(), &["*.rs"]);
    assert_eq!(model.checks()["scoped"].run(), "true");
    let serialized = serde_yaml::to_string(&model).unwrap();
    let parsed = parse_v1_str(&serialized).unwrap();
    assert_eq!(
        parsed.selected_ids(V1Event::Change, None),
        model.selected_ids(V1Event::Change, None)
    );
    assert_eq!(V1Event::parse("accept").unwrap().as_str(), "accept");
    assert!(V1Event::parse("unknown")
        .unwrap_err()
        .to_string()
        .contains("expected `change` or `accept`"));
}

#[test]
fn legacy_approved_evaluator_forwards_and_neutral_snapshot_refuses_drift() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("policy.yml");
    let store = dir.path().join("trust.json");
    fs::write(&config, POLICY).unwrap();
    bless_in(&config, &store, "test").unwrap();
    let approved = match check_trust_in(&config, &store) {
        TrustOutcome::Trusted(approved) => approved,
        _ => panic!("unchanged consent must stay valid"),
    };
    let legacy = evaluate_v1(&approved, dir.path(), "accept", None).unwrap();
    let typed =
        evaluate_v1_snapshot(approved.snapshot(), dir.path(), V1Event::Accept, None).unwrap();
    assert_eq!(legacy, typed);
    assert_eq!(approved.policy_bytes(), POLICY.as_bytes());
    approved.verify_unchanged().unwrap();
    assert!(evaluate_v1(&approved, dir.path(), "unknown", None).is_err());
    fs::write(&config, "version: 1\nchecks:\n  changed: {run: 'true'}\n").unwrap();
    let denied =
        evaluate_v1_snapshot(approved.snapshot(), dir.path(), V1Event::Accept, None).unwrap();
    assert_eq!(denied.status, V1Status::Error);
    assert!(denied.results.is_empty());
    assert!(denied
        .not_run
        .iter()
        .all(|entry| entry.reason == "policy_changed"));
}

#[test]
fn neutral_evaluation_checks_post_run_and_between_check_script_drift() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("policy.yml");
    fs::create_dir_all(dir.path().join(".ironlint/scripts")).unwrap();
    fs::write(dir.path().join(".ironlint/scripts/helper"), "original").unwrap();
    fs::write(&config, "version: 1\nchecks:\n  a: {run: 'echo changed > .ironlint/scripts/helper'}\n  b: {run: 'touch should-not-run'}\n").unwrap();
    let snapshot = PolicySnapshot::load(&config).unwrap();
    let result = evaluate_v1_snapshot(&snapshot, dir.path(), V1Event::Accept, None).unwrap();
    assert_eq!(result.status, V1Status::Error);
    assert_eq!(result.not_run[0].reason, "policy_changed");
    assert!(!dir.path().join("should-not-run").exists());

    fs::write(
        &config,
        "version: 1\nchecks:\n  a: {run: 'echo final-change > .ironlint/scripts/helper'}\n",
    )
    .unwrap();
    let snapshot = PolicySnapshot::load(&config).unwrap();
    let result = evaluate_v1_snapshot(&snapshot, dir.path(), V1Event::Accept, None).unwrap();
    assert_eq!(result.status, V1Status::Error);
    assert!(result.error.unwrap().contains("helper"));
}
