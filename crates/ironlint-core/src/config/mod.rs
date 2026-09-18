pub mod scope;
pub(crate) mod v1;

use anyhow::Result;
use std::path::Path;

pub fn validate_v1_file(path: &Path) -> Result<usize> {
    Ok(v1::parse_v1_file(path)?.checks.len())
}

pub fn validate_v1_str(input: &str) -> Result<usize> {
    Ok(v1::parse_v1_str(input)?.checks.len())
}

#[cfg(test)]
mod tests {
    #[test]
    fn validate_v1_str_rejects_feedback_only_policy() {
        let error = super::validate_v1_str(
            "version: 1\nchecks:\n  feedback:\n    on: [change]\n    run: exit 0\n",
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("must include accept"),
            "{error:#}"
        );
    }
}
