//! `ironlint show-resolved-config` — print the configured check set.
//!
//! Prints each check in id order, annotated by the origin file it was defined
//! in. `tsv` (default) emits `check_id<TAB>origin<TAB>files(comma-joined)<TAB>run`;
//! followed by the optional timeout override and effective command timeout.
//! `yaml` and `json` emit rows with those same fields.
//! Read-only. Does not run any check.

use crate::cli::ShowFormat;
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct ResolvedCheck {
    check: String,
    origin: String,
    files: Vec<String>,
    run: String,
    timeout_secs: Option<u64>,
    effective_timeout_secs: u64,
}

pub fn run(config: &Path, format: ShowFormat) -> Result<i32> {
    let config_path = match crate::commands::config::resolve_config(config) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("error: {msg}");
            return Ok(1);
        }
    };
    let rows = match crate::commands::config::load_read_only_with_path(&config_path) {
        Ok((config_path, v1)) => build_v1_rows(&v1, &config_path),
        Err(e) => {
            eprintln!("error: {:#}", e);
            return Ok(1);
        }
    };
    match format {
        ShowFormat::Tsv => print_tsv(&rows),
        ShowFormat::Yaml => print_yaml(&rows)?,
        ShowFormat::Json => print_json(&rows)?,
    }
    Ok(0)
}

fn build_v1_rows(cfg: &ironlint_core::config::V1Config, origin: &Path) -> Vec<ResolvedCheck> {
    cfg.checks()
        .iter()
        .map(|(id, check)| ResolvedCheck {
            check: id.clone(),
            origin: origin.display().to_string(),
            files: check.files().unwrap_or_default().to_vec(),
            run: check.run().to_owned(),
            timeout_secs: check.timeout_secs(),
            effective_timeout_secs: check.effective_timeout_secs(cfg.execution()),
        })
        .collect()
}

fn print_tsv(rows: &[ResolvedCheck]) {
    for r in rows {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            r.check,
            r.origin,
            r.files.join(","),
            r.run,
            r.timeout_secs
                .map(|timeout| timeout.to_string())
                .unwrap_or_default(),
            r.effective_timeout_secs
        );
    }
}

fn print_yaml(rows: &[ResolvedCheck]) -> Result<()> {
    print!("{}", serde_yaml::to_string(rows)?);
    Ok(())
}

fn print_json(rows: &[ResolvedCheck]) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(rows)?);
    Ok(())
}
