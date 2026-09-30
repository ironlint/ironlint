use ironlint_core::adapter::{
    all_harnesses, sidecar_path, status, status_paths, AdapterEnv, Harness, HarnessStatus, Scope,
    StatusPaths,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

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
    } else if s.intact.is_none() {
        (
            Status::Warn,
            "artifact ownership metadata missing; not managed".to_string(),
            Some(
                "preserve the existing artifact and inspect ownership before reinstalling"
                    .to_string(),
            ),
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
#[cfg(test)]
pub(super) fn hooks_row(adapter_rows: &[CheckResult]) -> CheckResult {
    let wired = adapter_rows
        .iter()
        .filter(|r| r.status == Status::Pass)
        .count();
    hooks_summary(wired, 0)
}

fn hooks_summary(wired: usize, artifacts: usize) -> CheckResult {
    if wired == 0 && artifacts == 0 {
        CheckResult {
            name: "hooks",
            status: Status::Warn,
            detail: "no coding-agent hooks detected".into(),
            remediation: Some("run `ironlint init`".into()),
        }
    } else if wired == 0 {
        CheckResult {
            name: "hooks",
            status: Status::Warn,
            detail: format!("0 harness(es) wired (healthy); {artifacts} artifact(s) detected"),
            remediation: Some("resolve the adapter diagnostics above".into()),
        }
    } else {
        CheckResult {
            name: "hooks",
            status: Status::Pass,
            detail: format!("{wired} harness(es) wired (healthy)"),
            remediation: None,
        }
    }
}

/// Compatibility helper for the unit tests of individual adapter rows.
#[cfg(test)]
pub(super) fn check_adapters(env: &AdapterEnv) -> Vec<CheckResult> {
    inspect_adapters(env)
        .into_iter()
        .map(|inspection| inspection.row)
        .collect()
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum PhysicalArtifact {
    #[cfg(unix)]
    Inode(u64, u64),
    Path(PathBuf),
}

fn canonical_resource(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn physical_artifact(path: &Path) -> PhysicalArtifact {
    #[cfg(unix)]
    if let Ok(metadata) = std::fs::metadata(path) {
        use std::os::unix::fs::MetadataExt;
        return PhysicalArtifact::Inode(metadata.dev(), metadata.ino());
    }
    PhysicalArtifact::Path(canonical_resource(path))
}

struct Inspection {
    row: CheckResult,
    paths: StatusPaths,
}

fn same_resources(a: &Inspection, b: &Inspection) -> bool {
    a.row.name == b.row.name
        && physical_artifact(&a.paths.artifact) == physical_artifact(&b.paths.artifact)
        && a.paths.registration.as_deref().map(canonical_resource)
            == b.paths.registration.as_deref().map(canonical_resource)
        && canonical_resource(&ownership_path(&a.paths))
            == canonical_resource(&ownership_path(&b.paths))
}

fn ownership_path(paths: &StatusPaths) -> PathBuf {
    sidecar_path(paths.artifact.parent().unwrap_or(Path::new(".")))
}

fn scope_name(scope: Scope) -> &'static str {
    match scope {
        Scope::Local => "local",
        Scope::Global => "global",
    }
}

fn remediation(harness: &Harness, scope: Scope, failed: bool) -> String {
    let operation = if harness.installable {
        ""
    } else {
        "--uninstall "
    };
    let global = if scope == Scope::Global {
        " --global"
    } else {
        ""
    };
    let action = format!(
        "ironlint init {operation}--harness {}{global}",
        harness.name
    );
    if failed {
        format!("inspect the path/error above; then run `{action}`")
    } else {
        format!("run `{action}`")
    }
}

fn inspect_scope(harness: &Harness, env: &AdapterEnv, scope: Scope) -> Option<Inspection> {
    let paths = status_paths(harness, env, scope);
    let mut row = match status(harness, env, scope) {
        Ok(state) => adapter_check(&state)?,
        Err(error) => CheckResult {
            name: harness.name,
            status: Status::Fail,
            detail: format!("{error:#}"),
            remediation: None,
        },
    };
    if !harness.installable {
        row.detail
            .push_str("; unsupported adapter retained for cleanup");
        if row.status == Status::Pass {
            row.status = Status::Warn;
        }
    }
    if row.status != Status::Pass {
        row.remediation = Some(remediation(harness, scope, row.status == Status::Fail));
    }
    let registration = paths
        .registration
        .as_ref()
        .map_or_else(String::new, |path| {
            format!("; registration {}", path.display())
        });
    row.detail = format!(
        "{}: {}; artifact {}{registration}",
        scope_name(scope),
        row.detail,
        paths.artifact.display()
    );
    Some(Inspection { row, paths })
}

fn inspect_adapters(env: &AdapterEnv) -> Vec<Inspection> {
    // Resolve and inspect every scope before merging aliases, so a failing
    // registration cannot disappear behind a healthy row for a shared file.
    let all: Vec<_> = all_harnesses()
        .iter()
        .flat_map(|harness| {
            [Scope::Local, Scope::Global]
                .into_iter()
                .filter_map(|scope| inspect_scope(harness, env, scope))
        })
        .collect();
    let mut merged: Vec<Inspection> = Vec::new();
    for next in all {
        if let Some(existing) = merged
            .iter_mut()
            .find(|existing| same_resources(existing, &next))
        {
            existing.row.detail = format!(
                "local/global: {} | {}",
                existing.row.detail, next.row.detail
            );
            if severity(next.row.status) > severity(existing.row.status) {
                existing.row.status = next.row.status;
                existing.row.remediation = next.row.remediation;
            }
        } else {
            merged.push(next);
        }
    }
    merged
}

fn severity(status: Status) -> u8 {
    match status {
        Status::Pass => 0,
        Status::Warn => 1,
        Status::Fail => 2,
    }
}

/// The adapter block of the report: per-harness rows and the always-present
/// hooks summary.
/// When the adapter environment can't be resolved, no adapter is detectable,
/// so only the (warning) hooks summary over an empty set is emitted.
pub(super) fn adapter_section(dir: &Path) -> Vec<CheckResult> {
    let env = match AdapterEnv::from_process(dir.to_path_buf()) {
        Ok(env) => env,
        Err(error) => {
            return vec![
                CheckResult {
                    name: "adapters",
                    status: Status::Fail,
                    detail: format!("resolving adapter environment: {error:#}"),
                    remediation: Some(
                        "set HOME and XDG_CONFIG_HOME to readable directories".into(),
                    ),
                },
                hooks_summary(0, 0),
            ]
        }
    };
    let inspections = inspect_adapters(&env);
    let wired: HashSet<_> = inspections
        .iter()
        .filter(|inspection| inspection.row.status == Status::Pass)
        .map(|inspection| physical_artifact(&inspection.paths.artifact))
        .collect();
    let artifacts: HashSet<_> = inspections
        .iter()
        .filter(|inspection| {
            std::fs::metadata(&inspection.paths.artifact).is_ok_and(|metadata| metadata.is_file())
        })
        .map(|inspection| physical_artifact(&inspection.paths.artifact))
        .collect();
    let hooks = hooks_summary(wired.len(), artifacts.len());
    let mut section: Vec<_> = inspections
        .into_iter()
        .map(|inspection| inspection.row)
        .collect();
    section.push(hooks);
    section
}
