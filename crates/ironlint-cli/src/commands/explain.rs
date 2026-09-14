//! `ironlint explain <file>` — read-only check applicability report.
//!
//! For each check in the resolved config (BTreeMap id order), reports whether
//! the check's file globs apply to the given path (`match`) or not (`skip`).
//! `human` (default) prints one line per check
//! `<check-id>  <match|skip>  files=<globs>  run=<run>`; `json` prints an array
//! of `{ check, status, files, run }` objects. No check logic is executed.
//! Errors go to stderr; exit 1.

use crate::cli::OutputFormat;
use anyhow::Result;
use ironlint_core::runner::IronLintEngine;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct ExplainEntry<'a> {
    check: &'a str,
    status: &'static str,
    files: &'a [String],
    run: &'a str,
}

#[derive(Serialize)]
struct V1ExplainEntry<'a> {
    check: &'a str,
    acceptance: &'static str,
    change: &'static str,
    files: &'a [String],
    run: &'a str,
}

fn status_for(engine: &IronLintEngine, check_id: &str, file: &Path) -> &'static str {
    if engine.check_matches_path(check_id, file) {
        "match"
    } else {
        "skip"
    }
}

pub fn run(file: &Path, format: OutputFormat, config: &Path, root: Option<&Path>) -> Result<i32> {
    let config = match crate::commands::config::resolve_config(config) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("error: {msg}");
            return Ok(1);
        }
    };
    match crate::commands::config::load_read_only_with_path(&config) {
        Ok((_config_path, crate::commands::config::ReadOnlyConfig::V1(v1))) => {
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
            return Ok(0);
        }
        Ok((_, crate::commands::config::ReadOnlyConfig::Legacy { .. })) if root.is_some() => {
            eprintln!("error: legacy checks do not accept --root");
            return Ok(1);
        }
        Ok((_, crate::commands::config::ReadOnlyConfig::Legacy { .. })) => {}
        Err(e) => {
            eprintln!("error: {:#}", e);
            return Ok(1);
        }
    }
    let engine = match IronLintEngine::load(&config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {:#}", e);
            return Ok(1);
        }
    };
    match format {
        OutputFormat::Human => print_human(&engine, file),
        OutputFormat::Json => print_json(&engine, file)?,
    }
    Ok(0)
}

fn v1_change_status(
    check: &crate::commands::config::V1InspectionCheck,
    file: &Path,
) -> &'static str {
    if !check.on.iter().any(|event| event == "change") {
        return "not-enabled";
    }
    let Some(globs) = &check.files else {
        return "match";
    };
    ironlint_core::config::scope::ScopeMatcher::new(globs)
        .map(|matcher| {
            if matcher.matches(file) {
                "match"
            } else {
                "skip"
            }
        })
        .unwrap_or("skip")
}

fn print_v1_human(config: &crate::commands::config::V1InspectionConfig, file: &Path) {
    for (id, check) in &config.checks {
        let files = check.files.as_deref().unwrap_or_default().join(",");
        println!(
            "{id}  acceptance=required  change={}  files={files}  run={}",
            v1_change_status(check, file),
            check.run
        );
    }
}

fn print_v1_json(config: &crate::commands::config::V1InspectionConfig, file: &Path) -> Result<()> {
    let entries: Vec<V1ExplainEntry<'_>> = config
        .checks
        .iter()
        .map(|(id, check)| V1ExplainEntry {
            check: id,
            acceptance: "required",
            change: v1_change_status(check, file),
            files: check.files.as_deref().unwrap_or_default(),
            run: &check.run,
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&entries)?);
    Ok(())
}

fn print_human(engine: &IronLintEngine, file: &Path) {
    for (id, check) in engine.checks() {
        let status = status_for(engine, id, file);
        let files = check.files.join(",");
        println!(
            "{id}  {status}  files={files}  run={}",
            check.run.as_deref().unwrap_or("(steps)")
        );
    }
}

fn print_json(engine: &IronLintEngine, file: &Path) -> Result<()> {
    let entries: Vec<ExplainEntry<'_>> = engine
        .checks()
        .iter()
        .map(|(id, check)| ExplainEntry {
            check: id,
            status: status_for(engine, id, file),
            files: &check.files,
            run: check.run.as_deref().unwrap_or("(steps)"),
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&entries)?);
    Ok(())
}
