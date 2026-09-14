use crate::cli::OutputFormat;
use anyhow::Result;
use serde_json::json;
use std::path::Path;

pub fn run(config: &Path, format: OutputFormat) -> Result<i32> {
    let config = match crate::commands::config::resolve_config(config) {
        Ok(p) => p,
        Err(msg) => {
            return Ok(crate::commands::error_report::emit_error(format, &msg, 1));
        }
    };
    let result = if crate::commands::config::is_versioned_config(&config) {
        ironlint_core::config::validate_v1_file(&config)
    } else {
        ironlint_core::config::parse_file_with_extends(&config).map(|cfg| cfg.checks.len())
    };
    match result {
        Ok(check_count) => {
            match format {
                OutputFormat::Human => println!("ok: {check_count} check(s)"),
                OutputFormat::Json => {
                    let body = json!({ "status": "ok", "checks": check_count });
                    println!("{body}");
                }
            }
            Ok(0)
        }
        Err(e) => Ok(crate::commands::error_report::emit_error(
            format,
            &format!("{e:#}"),
            1,
        )),
    }
}
