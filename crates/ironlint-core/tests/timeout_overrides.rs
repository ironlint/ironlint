use ironlint_core::config::{parse_v1_str, V1Event};
use ironlint_core::policy::PolicySnapshot;
use ironlint_core::runner::evaluate_v1_snapshot;
use ironlint_core::verdict::{V1Status, V1Verdict};
use std::fs;

fn evaluate(policy: &str, event: V1Event) -> V1Verdict {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("policy.yml");
    fs::write(&config, policy).unwrap();
    let snapshot = PolicySnapshot::load(&config).unwrap();
    evaluate_v1_snapshot(&snapshot, dir.path(), event, None).unwrap()
}

#[test]
fn parses_optional_positive_timeout_overrides() {
    let policy = parse_v1_str("version: 1\nexecution: {timeout_secs: 30, total_timeout_secs: 300}\nchecks:\n  inherit: {run: 'true'}\n  short: {timeout_secs: 10, run: 'true'}\n  long: {timeout_secs: 180, run: 'true'}\n").unwrap();
    let serialized = serde_json::to_value(&policy).unwrap();
    assert_eq!(policy.checks()["inherit"].timeout_secs(), None);
    assert_eq!(
        policy.checks()["inherit"].effective_timeout_secs(policy.execution()),
        30
    );
    assert_eq!(policy.checks()["short"].timeout_secs(), Some(10));
    assert_eq!(
        policy.checks()["short"].effective_timeout_secs(policy.execution()),
        10
    );
    assert_eq!(
        policy.checks()["long"].effective_timeout_secs(policy.execution()),
        180
    );
    assert!(serialized["checks"]["inherit"]
        .get("timeout_secs")
        .is_none());
    assert_eq!(serialized["checks"]["short"]["timeout_secs"], 10);
    assert_eq!(serialized["checks"]["long"]["timeout_secs"], 180);
    let roundtrip = parse_v1_str(&serde_yaml::to_string(&policy).unwrap()).unwrap();
    assert_eq!(
        roundtrip.selected_ids(V1Event::Accept, None),
        vec!["inherit", "long", "short"]
    );
}

#[test]
fn rejects_invalid_timeout_override_types_values_and_duplicate_fields() {
    for value in [
        "0",
        "-1",
        "1.5",
        "null",
        "'1'",
        "true",
        "[]",
        "{}",
        "18446744073709551616",
    ] {
        let input =
            format!("version: 1\nchecks:\n  check: {{timeout_secs: {value}, run: 'true'}}\n");
        assert!(parse_v1_str(&input).is_err(), "accepted {value}");
    }
    for input in [
        "version: 1\nchecks:\n  check: {timeout_secs: 1, timeout_secs: 2, run: 'true'}\n",
        "version: 1\nchecks:\n  check: {timeout_sec: 1, run: 'true'}\n",
    ] {
        assert!(parse_v1_str(input).is_err());
    }
}

#[test]
fn real_mixed_duration_commands_use_the_same_override_for_both_events() {
    let policy = "version: 1\nexecution: {timeout_secs: 1, total_timeout_secs: 10}\nchecks:\n  a-default: {on: [change, accept], run: 'printf fast'}\n  b-long: {on: [change, accept], timeout_secs: 3, run: 'sleep 1.2; printf long'}\n";
    for event in [V1Event::Accept, V1Event::Change] {
        let verdict = evaluate(policy, event);
        assert_eq!(verdict.status, V1Status::Pass, "{verdict:?}");
        assert_eq!(verdict.schema, 7);
        assert_eq!(verdict.results[0].stdout, b"fast");
        assert_eq!(verdict.results[1].stdout, b"long");
    }
}

#[test]
fn shorter_override_stops_a_command_before_the_global_default() {
    let verdict = evaluate("version: 1\nexecution: {timeout_secs: 3, total_timeout_secs: 10}\nchecks:\n  a-short: {timeout_secs: 1, run: 'sleep 2'}\n  b-later: {run: 'true'}\n", V1Event::Accept);
    assert_eq!(verdict.status, V1Status::Error);
    assert_eq!(verdict.results[0].reason.as_deref(), Some("timeout"));
    assert_eq!(verdict.not_run[0].reason, "execution_error");
}

#[test]
fn missing_override_inherits_the_global_timeout() {
    let verdict = evaluate("version: 1\nexecution: {timeout_secs: 1, total_timeout_secs: 10}\nchecks:\n  slow: {run: 'sleep 2'}\n", V1Event::Accept);
    assert_eq!(verdict.status, V1Status::Error);
    assert_eq!(verdict.results[0].reason.as_deref(), Some("timeout"));
}

#[test]
fn total_deadline_caps_a_longer_check_override() {
    let verdict = evaluate("version: 1\nexecution: {timeout_secs: 30, total_timeout_secs: 1}\nchecks:\n  a-long: {timeout_secs: 180, run: 'sleep 2'}\n  b-later: {run: 'true'}\n", V1Event::Accept);
    assert_eq!(verdict.status, V1Status::Error);
    assert_eq!(verdict.error.as_deref(), Some("total_timeout"));
    assert_eq!(verdict.not_run[0].reason, "total_timeout");
}

#[test]
fn huge_positive_override_is_capped_before_deadline_arithmetic() {
    let policy = format!("version: 1\nexecution: {{total_timeout_secs: 5}}\nchecks:\n  ok: {{timeout_secs: {}, run: 'true'}}\n", u64::MAX);
    assert_eq!(evaluate(&policy, V1Event::Accept).status, V1Status::Pass);
}

#[test]
fn impossible_total_deadline_remains_an_execution_error() {
    let policy = format!("version: 1\nexecution: {{total_timeout_secs: {0}}}\nchecks:\n  ok: {{timeout_secs: {0}, run: 'true'}}\n", u64::MAX);
    let verdict = evaluate(&policy, V1Event::Accept);
    assert_eq!(verdict.status, V1Status::Error);
    assert_eq!(verdict.error.as_deref(), Some("total_timeout"));
    assert!(verdict.results.is_empty());
}
