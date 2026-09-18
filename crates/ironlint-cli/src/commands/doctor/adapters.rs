use ironlint_core::adapter::{all_harnesses, status, AdapterEnv, HarnessStatus, Scope};
use std::path::Path;

use super::{CheckResult, Status};

/// Decide the (status, detail, remediation) triple for a harness that is
/// detected or installed. Split out of `adapter_check` so the if/else-if ladder
/// stays under the cognitive-complexity cap.
fn adapter_verdict(s: &HarnessStatus) -> (Status, String, Option<String>) {
    if s.registered && !s.installed {
        (
            Status::Fail,
            "registered in settings but hook artifact is missing (broken)".to_string(),
            Some(format!("re-run `ironlint init --harness {}`", s.harness)),
        )
    } else if !s.installed {
        (
            Status::Warn,
            "harness detected; ironlint hook not installed".to_string(),
            Some(format!("run `ironlint init --harness {}`", s.harness)),
        )
    } else if !s.registered {
        (
            Status::Warn,
            "hook artifact present but not registered in settings".to_string(),
            Some(format!("re-run `ironlint init --harness {}`", s.harness)),
        )
    } else if s.intact == Some(false) {
        (
            Status::Warn,
            "hook artifact modified since install".to_string(),
            Some("re-run `ironlint init` to restore".to_string()),
        )
    } else if s.current == Some(false) {
        (
            Status::Warn,
            "hook artifact outdated".to_string(),
            Some("re-run `ironlint init` to update".to_string()),
        )
    } else {
        (Status::Pass, "installed and registered".to_string(), None)
    }
}

/// Map one harness's status to a doctor CheckResult. Returns None for a
/// harness that is neither present nor installed (no signal worth a line).
pub(super) fn adapter_check(s: &HarnessStatus) -> Option<CheckResult> {
    if !s.detected && !s.installed && !s.registered {
        return None;
    }
    let (status, detail, remediation) = adapter_verdict(s);
    Some(CheckResult {
        name: s.harness,
        status,
        detail,
        remediation,
    })
}

/// Always-present summary row over the per-harness adapter rows. Warns when
/// zero coding-agent hooks are wired — the most common first-run failure mode,
/// since the tool's entire effect happens through hooks. Only a `Pass` adapter
/// row (installed AND registered) counts as wired: a `Warn` row (detected but
/// ironlint not installed) or a `Fail` row (registered but broken) is present
/// but NOT wired, so a machine with, e.g., Claude Code installed and `ironlint
/// init` never run must still warn — not report a healthy install.
pub(super) fn hooks_row(adapter_rows: &[CheckResult]) -> CheckResult {
    let wired = adapter_rows
        .iter()
        .filter(|r| r.status == Status::Pass)
        .count();
    if wired == 0 {
        CheckResult {
            name: "hooks",
            status: Status::Warn,
            detail: "no coding-agent hooks detected".into(),
            remediation: Some("run `ironlint init`".into()),
        }
    } else {
        CheckResult {
            name: "hooks",
            status: Status::Pass,
            detail: format!("{wired} harness(es) wired"),
            remediation: None,
        }
    }
}

/// Per-harness adapter checks. Uses Local scope because `ironlint init` defaults
/// to a project-local install and `doctor` runs in a project; a status() error
/// for a harness is skipped rather than failing the whole report.
pub(super) fn check_adapters(env: &AdapterEnv) -> Vec<CheckResult> {
    all_harnesses()
        .iter()
        .filter(|h| h.installable)
        .filter_map(|h| status(h, env, Scope::Local).ok())
        .filter_map(|s| adapter_check(&s))
        .collect()
}

/// The adapter block of the report: per-harness rows and the always-present
/// hooks summary.
/// When the adapter environment can't be resolved, no adapter is detectable,
/// so only the (warning) hooks summary over an empty set is emitted.
pub(super) fn adapter_section(dir: &Path) -> Vec<CheckResult> {
    let Ok(env) = AdapterEnv::from_process(dir.to_path_buf()) else {
        return vec![hooks_row(&[])];
    };
    let adapter_rows = check_adapters(&env);
    let hooks = hooks_row(&adapter_rows);
    let mut section = adapter_rows;
    section.push(hooks);
    section
}
