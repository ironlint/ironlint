pub mod extends;
pub mod parser;
pub mod scope;
pub mod types;
pub(crate) mod v1;

pub use parser::{parse_file, parse_str};
pub use types::*;

use anyhow::Result;
use std::path::Path;

pub fn parse_file_with_extends(path: &Path) -> Result<Config> {
    extends::resolve(path)
}

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
        let err = super::validate_v1_str(
            "version: 1\nchecks:\n  feedback:\n    on: [change]\n    run: exit 0\n",
        )
        .unwrap_err();
        assert!(err.to_string().contains("must include accept"), "{err:#}");
    }
}
