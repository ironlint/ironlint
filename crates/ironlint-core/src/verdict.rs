use serde::{Deserialize, Serialize};

/// Versioned acceptance verdict schema.
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
