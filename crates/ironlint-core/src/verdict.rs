use serde::{Deserialize, Serialize};

/// Verdict JSON schema version.
///
/// Bumped to 6 for `GateError.detail`: an internal-error verdict now carries a
/// human-readable remediation string naming the effective timeout and the
/// (truncated) run command that crashed.
pub const SCHEMA_VERSION: u32 = 6;

/// Floor schema version all current verdicts satisfy.
pub const MIN_REQUIRED_SCHEMA_VERSION: u32 = 4;

/// Versioned acceptance verdict schema. Kept separate from the legacy
/// `SCHEMA_VERSION` until the transition path is removed.
pub const V1_SCHEMA_VERSION: u32 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum V1Status {
    Pass,
    Violation,
    Error,
    #[serde(rename = "not_run")]
    NotRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum V1CheckOutcome {
    Pass,
    Violation,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1CheckResult {
    pub id: String,
    pub outcome: V1CheckOutcome,
    pub exit_status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1NotRun {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1Verdict {
    pub schema: u32,
    pub event: String,
    pub status: V1Status,
    pub results: Vec<V1CheckResult>,
    pub not_run: Vec<V1NotRun>,
    pub error: Option<String>,
}

impl V1Verdict {
    pub fn from_results(
        event: impl Into<String>,
        results: Vec<V1CheckResult>,
        not_run: Vec<V1NotRun>,
        error: Option<String>,
    ) -> Self {
        let status = if error.is_some()
            || !not_run.is_empty()
            || results.iter().any(|r| r.outcome == V1CheckOutcome::Error)
        {
            V1Status::Error
        } else if results
            .iter()
            .any(|r| r.outcome == V1CheckOutcome::Violation)
        {
            V1Status::Violation
        } else if results.is_empty() {
            V1Status::NotRun
        } else {
            V1Status::Pass
        };
        Self {
            schema: V1_SCHEMA_VERSION,
            event: event.into(),
            status,
            results,
            not_run,
            error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub schema_version: u32,
    pub ironlint_version: String,
    pub status: Status,
    pub blocks: Vec<Block>,
    pub errors: Vec<GateError>,
    /// Check ids that ran and passed (for `--explain` / telemetry).
    pub passed: Vec<String>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Status {
    Pass,
    Block,
    #[serde(rename = "internal_error")]
    InternalError,
}

/// A check that exited 2 on a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub check: String,
    /// Step within a multi-step check that blocked. `null` in Phase 1 (single
    /// `run`); populated in Phase 3 when `steps:` is introduced.
    pub step: Option<String>,
    /// File that triggered the block. `null` for run-once checks (e.g.
    /// `pre-commit` mode in Phase 4); always `Some` in Phase 1.
    pub file: Option<String>,
    /// Verbatim trimmed stdout+stderr from the check.
    pub message: String,
}

/// A check that crashed (not found / not executable / timeout / signal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateError {
    pub check: String,
    /// Step within a multi-step check that crashed. `null` in Phase 1.
    pub step: Option<String>,
    /// File under check. `null` for run-once checks.
    pub file: Option<String>,
    /// Stable reason string from `InternalReason::as_str`.
    pub reason: String,
    /// Human-readable remediation: names the run command (truncated) and, for
    /// timeouts, the effective timeout that fired. `null` when no detail is
    /// available (e.g. a synthetic error). Added in schema v6.
    pub detail: Option<String>,
}

impl Verdict {
    pub fn pass() -> Self {
        Self::from_outcomes(vec![], vec![], vec![], 0)
    }

    /// Build a verdict from collected outcomes.
    ///
    /// Status precedence: **Block wins over InternalError** — a confirmed
    /// policy violation (exit 2) must stop the edit even if an unrelated check
    /// crashed. Only when there are no blocks does a crash escalate to
    /// InternalError (exit 3, adapter fail-open).
    pub fn from_outcomes(
        blocks: Vec<Block>,
        errors: Vec<GateError>,
        passed: Vec<String>,
        elapsed_ms: u64,
    ) -> Self {
        let status = if !blocks.is_empty() {
            Status::Block
        } else if !errors.is_empty() {
            Status::InternalError
        } else {
            Status::Pass
        };
        Self {
            schema_version: SCHEMA_VERSION,
            ironlint_version: env!("CARGO_PKG_VERSION").to_string(),
            status,
            blocks,
            errors,
            passed,
            elapsed_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_pass() {
        let v = Verdict::from_outcomes(vec![], vec![], vec![], 0);
        assert_eq!(v.status, Status::Pass);
    }

    #[test]
    fn any_block_is_block() {
        let v = Verdict::from_outcomes(
            vec![Block {
                check: "g".into(),
                step: None,
                file: Some("f".into()),
                message: "m".into(),
            }],
            vec![],
            vec![],
            0,
        );
        assert_eq!(v.status, Status::Block);
    }

    #[test]
    fn block_wins_over_internal_error() {
        let v = Verdict::from_outcomes(
            vec![Block {
                check: "g".into(),
                step: None,
                file: Some("f".into()),
                message: "m".into(),
            }],
            vec![GateError {
                check: "h".into(),
                step: None,
                file: Some("f".into()),
                reason: "timeout".into(),
                detail: None,
            }],
            vec![],
            0,
        );
        assert_eq!(
            v.status,
            Status::Block,
            "a confirmed block must not be downgraded to fail-open by an unrelated crash"
        );
    }

    #[test]
    fn errors_only_is_internal_error() {
        let v = Verdict::from_outcomes(
            vec![],
            vec![GateError {
                check: "h".into(),
                step: None,
                file: Some("f".into()),
                reason: "not_found".into(),
                detail: None,
            }],
            vec![],
            0,
        );
        assert_eq!(v.status, Status::InternalError);
    }

    #[test]
    fn schema_version_is_6() {
        assert_eq!(SCHEMA_VERSION, 6);
    }

    #[test]
    fn v1_status_requires_complete_success() {
        let result = |outcome| V1CheckResult {
            id: "check".into(),
            outcome,
            exit_status: Some(0),
            stdout: vec![],
            stderr: vec![],
            stdout_truncated: false,
            stderr_truncated: false,
            reason: None,
        };
        assert_eq!(
            V1Verdict::from_results("accept", vec![result(V1CheckOutcome::Pass)], vec![], None)
                .status,
            V1Status::Pass
        );
        assert_eq!(
            V1Verdict::from_results(
                "accept",
                vec![result(V1CheckOutcome::Violation)],
                vec![],
                None,
            )
            .status,
            V1Status::Violation
        );
        assert_eq!(
            V1Verdict::from_results("change", vec![], vec![], None).status,
            V1Status::NotRun
        );
        assert_eq!(
            V1Verdict::from_results(
                "accept",
                vec![result(V1CheckOutcome::Pass)],
                vec![V1NotRun {
                    id: "later".into(),
                    reason: "execution_error".into(),
                }],
                None,
            )
            .status,
            V1Status::Error
        );
        assert_eq!(
            V1Verdict::from_results("accept", vec![], vec![], Some("timeout".into())).status,
            V1Status::Error
        );
    }

    #[test]
    fn v1_not_run_serializes_with_schema_seven_spelling() {
        let verdict = V1Verdict::from_results("change", vec![], vec![], None);
        let json = serde_json::to_value(verdict).unwrap();
        assert_eq!(json["schema"], V1_SCHEMA_VERSION);
        assert_eq!(json["status"], "not_run");
    }

    /// Locks the full verdict-JSON wire shape: top-level keys, `Status`
    /// string casing, and `schema_version` (visible and literal — must show
    /// `6`). Covers a block, an internal error, and a passed entry in one
    /// verdict so every array shape is exercised. `elapsed_ms` and
    /// `ironlint_version` are redacted — the former is caller-supplied
    /// timing, the latter tracks `CARGO_PKG_VERSION` and would break this
    /// snapshot on every version bump otherwise.
    #[test]
    fn verdict_json_wire_shape() {
        let verdict = Verdict::from_outcomes(
            vec![Block {
                check: "no-todo".into(),
                step: Some("no-any".into()),
                file: Some("src/a.rs".into()),
                message: "TODO found".into(),
            }],
            vec![GateError {
                check: "flaky".into(),
                step: None,
                file: Some("src/b.rs".into()),
                reason: "not_found".into(),
                detail: Some("not_found running: missing-cmd".into()),
            }],
            vec!["fmt".into()],
            1234,
        );
        insta::assert_json_snapshot!(verdict, {
            ".elapsed_ms" => "[ms]",
            ".ironlint_version" => "[version]",
        });
    }

    #[test]
    fn block_serializes_check_key_not_gate() {
        let b = Block {
            check: "rustfmt".into(),
            step: None,
            file: Some("a.rs".into()),
            message: "x".into(),
        };
        let j = serde_json::to_string(&b).unwrap();
        assert!(j.contains("\"check\":\"rustfmt\""), "{j}");
        assert!(!j.contains("\"gate\""), "{j}");
    }
}
