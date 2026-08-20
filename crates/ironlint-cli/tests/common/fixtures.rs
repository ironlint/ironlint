//! W4 contract-fixture loading (specs/2026-08-17-git-floor-hook-and-self-defense-design.md).
//!
//! Each harness's `adapters/<harness>/fixtures/` dir holds pinned,
//! provenance-stamped payloads captured from a LIVE harness session. The
//! contract suites load these as the happy-shape pin; embedded synthetic
//! payloads remain for malformed/adversarial edge cases.
//!
//! Provenance is mandatory: a fixture without a parseable `_provenance`
//! header (with non-empty `harness_version` and `captured_at`) is an `Err` —
//! the suites fail rather than trust an unprovenanced payload. A MISSING
//! fixture (capture still pending) is `Ok(None)`: the caller falls back to
//! its synthetic payload and prints a loud capture-pending note, so the gap
//! is visible in every run until the capture procedure
//! (`adapters/<harness>/fixtures/README.md`) has been followed.

use crate::common::repo_path;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// A loaded fixture: the verbatim `payload` plus its `_provenance` header.
pub struct ContractFixture {
    pub payload: Value,
    pub provenance: Value,
}

/// Path to `adapters/<harness>/fixtures/<name>.json` from the repo root.
pub fn fixture_path(harness: &str, name: &str) -> PathBuf {
    repo_path("adapters")
        .join(harness)
        .join("fixtures")
        .join(format!("{name}.json"))
}

/// Load a fixture by name. See the module doc for the `None` vs `Err`
/// contract.
pub fn load_fixture(harness: &str, name: &str) -> anyhow::Result<Option<ContractFixture>> {
    let path = fixture_path(harness, name);
    if !path.exists() {
        eprintln!(
            "WARNING: contract fixture missing: {} — capture pending \
             (see adapters/{harness}/fixtures/README.md); falling back to synthetic payload",
            path.display()
        );
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    let v: Value = serde_json::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("fixture {} is not valid JSON: {e}", path.display()))?;
    let provenance = v.get("_provenance").ok_or_else(|| {
        anyhow::anyhow!(
            "fixture {} lacks the mandatory _provenance header",
            path.display()
        )
    })?;
    // W4-R3 meta: provenance must carry a non-empty harness_version and
    // captured_at — this is what makes a drift visible in `git diff`.
    for key in ["harness_version", "captured_at"] {
        let ok = provenance
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty());
        if !ok {
            anyhow::bail!(
                "fixture {} provenance missing non-empty `{key}`",
                path.display()
            );
        }
    }
    let payload = v
        .get("payload")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("fixture {} lacks the payload key", path.display()))?;
    Ok(Some(ContractFixture {
        payload,
        provenance: provenance.clone(),
    }))
}

/// Meta-test helper (W4-R3): every fixture file that EXISTS in a harness's
/// fixtures dir must load with parseable provenance. Returns the list of
/// loaded fixtures so callers can additionally assert adapter-expected
/// fields. A missing file is fine (capture pending); a broken one is not.
pub fn assert_fixture_dir_provenance(harness: &str) -> Vec<ContractFixture> {
    let dir = repo_path("adapters").join(harness).join("fixtures");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let name = p.file_stem().unwrap().to_string_lossy().to_string();
        let fx = load_fixture(harness, &name)
            .unwrap_or_else(|e| panic!("fixture {} failed provenance check: {e:#}", p.display()))
            .expect("existing fixture must load");
        out.push(fx);
    }
    out
}

/// Three-state capture-status verdict for one fixtures dir (round-3 review).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureStatus {
    /// At least one `.json` fixture exists — provenance/shape checks run
    /// separately via [`assert_fixture_dir_provenance`].
    Captured,
    /// README-only: capture pending is DECLARED — warn loudly, but pass
    /// locally and in CI (the pending state is on record, never silent).
    PendingDeclared,
    /// Neither fixtures nor a README: UNDECLARED empty — a vanished fixtures
    /// dir must not go silently green.
    UndeclaredEmpty,
}

fn capture_status(dir: &Path) -> CaptureStatus {
    let has_json = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
        })
        .unwrap_or(false);
    if has_json {
        return CaptureStatus::Captured;
    }
    if dir.join("README.md").is_file() {
        return CaptureStatus::PendingDeclared;
    }
    CaptureStatus::UndeclaredEmpty
}

/// Enforce the three-state rule, returning the undeclared-empty message as an
/// `Err` when `in_ci` (so callers/tests can assert the hard-fail path without
/// touching process env or argv).
fn assert_capture_status(dir: &Path, harness: &str, in_ci: bool) -> Result<(), String> {
    match capture_status(dir) {
        CaptureStatus::Captured => Ok(()),
        CaptureStatus::PendingDeclared => {
            eprintln!(
                "W4-PARTIAL: no {harness} fixtures captured yet — capture pending is \
                 declared in adapters/{harness}/fixtures/README.md; running the capture \
                 procedure there is a follow-up (back-burnered, not dropped)."
            );
            Ok(())
        }
        CaptureStatus::UndeclaredEmpty => {
            let msg = format!(
                "undeclared-empty fixtures dir {}: restore README.md (declare capture \
                 pending) or run the capture procedure in adapters/{harness}/fixtures/README.md",
                dir.display()
            );
            if in_ci {
                Err(msg)
            } else {
                eprintln!("WARNING: {msg}");
                Ok(())
            }
        }
    }
}

/// Is this run a CI run? `CI=true` (GitHub Actions sets it) or an explicit
/// `--ci` arg for local simulation of the CI hard-fail path.
fn is_ci() -> bool {
    std::env::var("CI").is_ok_and(|v| !v.is_empty() && v != "false")
        || std::env::args().any(|a| a == "--ci")
}

/// W4 capture-status meta-test (round-3 review): README-only = capture
/// pending DECLARED (warn, pass in CI); undeclared-empty (missing dir, or no
/// README and no fixtures) = warn locally, HARD FAIL in CI. A dir that holds
/// fixtures is captured and passes through to the provenance/shape checks.
pub fn assert_fixture_dir_capture_status(harness: &str) {
    let dir = repo_path("adapters").join(harness).join("fixtures");
    if let Err(msg) = assert_capture_status(&dir, harness, is_ci()) {
        panic!("{msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "ironlint-fixtures-test-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn readme_only_is_pending_declared() {
        let d = tmp("readme-only");
        fs::write(d.join("README.md"), "capture pending\n").unwrap();
        assert_eq!(capture_status(&d), CaptureStatus::PendingDeclared);
        assert_capture_status(&d, "test", true).unwrap(); // CI stays green
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn bare_dir_without_readme_is_undeclared_empty() {
        let d = tmp("bare");
        assert_eq!(capture_status(&d), CaptureStatus::UndeclaredEmpty);
        assert_capture_status(&d, "test", false).unwrap(); // local warn
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn undeclared_empty_fails_hard_in_ci() {
        let d = tmp("bare-ci");
        let err = assert_capture_status(&d, "test", true).unwrap_err();
        assert!(err.contains("undeclared-empty"), "err: {err}");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn missing_dir_is_undeclared_empty() {
        let d = std::env::temp_dir().join(format!(
            "ironlint-fixtures-test-{}-missing",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&d);
        assert_eq!(capture_status(&d), CaptureStatus::UndeclaredEmpty);
    }

    #[test]
    fn json_fixture_is_captured() {
        let d = tmp("captured");
        fs::write(d.join("apply_patch.json"), "{}").unwrap();
        assert_eq!(capture_status(&d), CaptureStatus::Captured);
        assert_capture_status(&d, "test", true).unwrap();
        fs::remove_dir_all(&d).unwrap();
    }
}
