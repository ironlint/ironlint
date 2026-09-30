//! `ironlint explain <file>` — read-only check applicability report.
//!
//! For each check in ID order, reports required acceptance, change applicability,
//! file scopes, command, and configured timeout. Human output prints one line
//! per check; JSON prints the same fields as an array. No commands execute.
//! Errors go to stderr; exit 1.

use crate::cli::OutputFormat;
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct V1ExplainEntry<'a> {
    check: &'a str,
    acceptance: &'static str,
    change: &'static str,
    files: &'a [String],
    run: &'a str,
    timeout_secs: Option<u64>,
    effective_timeout_secs: u64,
}

pub fn run(file: &Path, format: OutputFormat, config: &Path, root: Option<&Path>) -> Result<i32> {
    let config = match crate::commands::config::resolve_config(config) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("error: {msg}");
            return Ok(1);
        }
    };
    let v1 = match crate::commands::config::load_read_only_with_path(&config) {
        Ok((_config_path, v1)) => v1,
        Err(e) => {
            eprintln!("error: {:#}", e);
            return Ok(1);
        }
    };
    let root = root.unwrap_or_else(|| Path::new("."));
    let root = match crate::commands::check::normalize_v1_root(root) {
        Ok(root) => root,
        Err(reason) => {
            eprintln!("error: {reason}");
            return Ok(1);
        }
    };
    let file = match crate::commands::check::normalize_v1_path(&root, file) {
        Ok(file) => file,
        Err(reason) => {
            eprintln!("error: {reason}");
            return Ok(1);
        }
    };
    let file = file
        .strip_prefix(&root)
        .expect("normalized v1 path is in root");
    match format {
        OutputFormat::Human => print_v1_human(&v1, file),
        OutputFormat::Json => print_v1_json(&v1, file)?,
    }
    Ok(0)
}

fn v1_change_status(check: &ironlint_core::config::V1Check, file: &Path) -> &'static str {
    use ironlint_core::config::{SelectionDecision, V1Event};
    match check.selection(V1Event::Change, Some(&[file.to_path_buf()])) {
        SelectionDecision::ChangeDisabled => "not-enabled",
        decision if decision.is_selected() => "match",
        _ => "skip",
    }
}

fn print_v1_human(config: &ironlint_core::config::V1Config, file: &Path) {
    for (id, check) in config.checks() {
        let files = check.files().unwrap_or_default().join(",");
        let timeout = check
            .timeout_secs()
            .map_or_else(|| "default".to_owned(), |timeout| timeout.to_string());
        println!(
            "{id}  acceptance=required  change={}  files={files}  run={}  timeout_secs={timeout} effective_timeout_secs={}",
            v1_change_status(check, file),
            check.run(),
            check.effective_timeout_secs(config.execution())
        );
    }
}

fn print_v1_json(config: &ironlint_core::config::V1Config, file: &Path) -> Result<()> {
    let entries: Vec<V1ExplainEntry<'_>> = config
        .checks()
        .iter()
        .map(|(id, check)| V1ExplainEntry {
            check: id,
            acceptance: "required",
            change: v1_change_status(check, file),
            files: check.files().unwrap_or_default(),
            run: check.run(),
            timeout_secs: check.timeout_secs(),
            effective_timeout_secs: check.effective_timeout_secs(config.execution()),
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&entries)?);
    Ok(())
}
