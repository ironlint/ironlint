//! `ironlint show-resolved-config` — print the configured check set.
//!
//! Prints each check in id order, annotated by the origin file it was defined
//! in. `tsv` (default) emits `check_id<TAB>origin<TAB>files(comma-joined)<TAB>run`;
//! `yaml` and `json` emit a sequence of `{ check, origin, files, run }`.
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
        Ok((config_path, crate::commands::config::ReadOnlyConfig::V1(v1))) => {
            build_v1_rows(&v1, &config_path)
        }
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

fn build_v1_rows(
    cfg: &crate::commands::config::V1InspectionConfig,
    origin: &Path,
) -> Vec<ResolvedCheck> {
    cfg.checks
        .iter()
        .map(|(id, check)| ResolvedCheck {
            check: id.clone(),
            origin: origin.display().to_string(),
            files: check.files.clone().unwrap_or_default(),
            run: check.run.clone(),
        })
        .collect()
}

fn print_tsv(rows: &[ResolvedCheck]) {
    for r in rows {
        println!(
            "{}\t{}\t{}\t{}",
            r.check,
            r.origin,
            r.files.join(","),
            r.run
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
