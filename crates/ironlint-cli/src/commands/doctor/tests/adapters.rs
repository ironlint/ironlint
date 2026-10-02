use super::super::adapters::{adapter_check, check_adapters, hooks_row};
use super::super::{CheckResult, Status};
use super::adapter_env;
use ironlint_adapters::{all_harnesses, install, HarnessStatus, Scope};
use tempfile::tempdir;

#[test]
fn check_adapters_reports_installed_pi_as_pass() {
    let tmp = tempdir().unwrap();
    let env = adapter_env(tmp.path());
    let h = all_harnesses()
        .into_iter()
        .find(|h| h.name == "pi")
        .unwrap();
    install(&h, &env, Scope::Local).unwrap();
    let checks = check_adapters(&env);
    let r = checks.iter().find(|c| c.name == "pi").expect("pi reported");
    assert_eq!(r.status, Status::Pass);
    assert!(r.detail.contains("installed"));
}

#[test]
fn check_adapters_reports_modified_pi_as_warn() {
    let tmp = tempdir().unwrap();
    let env = adapter_env(tmp.path());
    let h = all_harnesses()
        .into_iter()
        .find(|h| h.name == "pi")
        .unwrap();
    install(&h, &env, Scope::Local).unwrap();
    std::fs::write(
        env.project_root.join(".pi/extensions/ironlint.ts"),
        "// modified",
    )
    .unwrap();
    let checks = check_adapters(&env);
    let r = checks.iter().find(|c| c.name == "pi").expect("pi reported");
    assert_eq!(r.status, Status::Warn);
}

fn harness_status(detected: bool, installed: bool, registered: bool) -> HarnessStatus {
    HarnessStatus {
        legacy_registration: None,
        harness: "codex",
        detected,
        installed,
        registered,
        intact: Some(true),
        current: Some(true),
    }
}

#[test]
fn adapter_check_skips_when_neither_detected_nor_installed() {
    let s = harness_status(false, false, false);
    assert!(adapter_check(&s).is_none());
}

#[test]
fn adapter_check_reports_registered_but_absent_as_fail() {
    // registered in settings but artifact gone AND harness dir absent:
    // must still surface as a broken (Fail) row, not be skipped.
    let s = HarnessStatus {
        legacy_registration: None,
        harness: "codex",
        detected: false,
        installed: false,
        registered: true,
        intact: None,
        current: None,
    };
    let c = adapter_check(&s).expect("registered-but-absent must not be skipped");
    assert_eq!(c.status, Status::Fail);
}

#[test]
fn adapter_check_warns_when_detected_but_not_installed() {
    let s = harness_status(true, false, false);
    let r = adapter_check(&s).expect("detected harness reported");
    assert_eq!(r.status, Status::Warn);
    assert!(r.detail.contains("not installed"));
    assert!(r
        .remediation
        .unwrap()
        .contains("ironlint init --harness codex"));
}

#[test]
fn adapter_check_warns_when_installed_but_not_registered() {
    let s = harness_status(true, true, false);
    let r = adapter_check(&s).expect("installed harness reported");
    assert_eq!(r.status, Status::Warn);
    assert!(r.detail.contains("not registered"));
}

#[test]
fn adapter_check_warns_when_artifact_modified() {
    let mut s = harness_status(true, true, true);
    s.intact = Some(false);
    let r = adapter_check(&s).expect("modified harness reported");
    assert_eq!(r.status, Status::Warn);
    assert!(r.detail.contains("modified"));
}

#[test]
fn adapter_check_warns_when_artifact_outdated() {
    let mut s = harness_status(true, true, true);
    s.current = Some(false);
    let r = adapter_check(&s).expect("outdated harness reported");
    assert_eq!(r.status, Status::Warn);
    assert!(r.detail.contains("outdated"));
}

#[test]
fn adapter_check_passes_when_installed_and_registered() {
    let s = harness_status(true, true, true);
    let r = adapter_check(&s).expect("healthy harness reported");
    assert_eq!(r.status, Status::Pass);
    assert!(r.remediation.is_none());
}

fn row(name: &'static str, status: Status) -> CheckResult {
    CheckResult {
        name,
        status,
        detail: String::new(),
        remediation: None,
    }
}

#[test]
fn hooks_row_warns_when_no_adapter_rows() {
    let r = hooks_row(&[]);
    assert_eq!(r.status, Status::Warn);
    assert!(r.detail.contains("no coding-agent hooks"));
    assert!(r.remediation.unwrap().contains("ironlint init"));
}

#[test]
fn hooks_row_warns_when_only_unwired_rows() {
    // A detected-but-not-installed harness surfaces as a Warn adapter row; it
    // is NOT wired, so the summary must warn, not report a healthy install.
    let rows = [row("codex", Status::Warn)];
    let r = hooks_row(&rows);
    assert_eq!(r.status, Status::Warn);
    assert!(r.detail.contains("no coding-agent hooks"));
}

#[test]
fn hooks_row_warns_when_only_broken_rows() {
    // A registered-but-broken harness surfaces as a Fail adapter row; still
    // zero hooks are actually wired, so the summary must warn.
    let rows = [row("codex", Status::Fail)];
    let r = hooks_row(&rows);
    assert_eq!(r.status, Status::Warn);
}

#[test]
fn hooks_row_counts_only_wired_pass_rows() {
    // One wired (Pass) harness + one unwired (Warn) harness → pass, but the
    // count reports only the wired one.
    let rows = [row("codex", Status::Pass), row("pi", Status::Warn)];
    let r = hooks_row(&rows);
    assert_eq!(r.status, Status::Pass);
    assert!(
        r.detail.contains("1 harness(es) wired"),
        "detail must count only wired rows: {}",
        r.detail
    );
}
