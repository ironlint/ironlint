use crate::cli::OutputFormat;
use anyhow::Result;
use ironlint_core::trust::TrustOutcome;
use ironlint_core::verdict::{V1CheckOutcome, V1Status, V1Verdict};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) const POSIX_SHELL: &str = "sh";

pub(crate) fn shell_available(command: &str) -> bool {
    Command::new(command)
        .arg("-c")
        .arg("exit 0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

pub fn run(
    files: Vec<PathBuf>,
    format: OutputFormat,
    config: &Path,
    event: Option<&str>,
    root: Option<&Path>,
) -> Result<i32> {
    let event = event.unwrap_or("accept");
    let config = match crate::commands::config::resolve_config(config) {
        Ok(path) => path,
        Err(reason) => return Ok(emit_v1_error(format, event, &reason, 1)),
    };
    let root = match normalize_v1_root(root.unwrap_or_else(|| Path::new("."))) {
        Ok(root) => root,
        Err(reason) => return Ok(emit_v1_error(format, event, &reason, 1)),
    };
    if event == "accept" && !files.is_empty() {
        return Ok(emit_v1_error(
            format,
            event,
            "acceptance has no file filter; use --event change with --file",
            1,
        ));
    }
    let changed = if event == "change" && !files.is_empty() {
        let mut paths = Vec::with_capacity(files.len());
        for file in files {
            let absolute = match normalize_v1_path(&root, &file) {
                Ok(path) => path,
                Err(reason) => return Ok(emit_v1_error(format, event, &reason, 1)),
            };
            paths.push(absolute.strip_prefix(&root).unwrap().to_path_buf());
        }
        Some(paths)
    } else {
        None
    };
    let approved = match ironlint_core::trust::check_trust(&config) {
        Ok(TrustOutcome::Trusted(approved)) => approved,
        Ok(TrustOutcome::Untrusted(error)) => {
            return Ok(emit_v1_error(format, event, &format!("{error:#}"), 4));
        }
        Ok(TrustOutcome::Unverifiable(error)) | Err(error) => {
            return Ok(emit_v1_error(format, event, &format!("{error:#}"), 1));
        }
    };
    let verdict =
        match ironlint_core::runner::evaluate_v1(&approved, &root, event, changed.as_deref()) {
            Ok(verdict) => verdict,
            Err(error) => return Ok(emit_v1_error(format, event, &format!("{error:#}"), 1)),
        };
    emit_v1(&verdict, format)?;
    Ok(match verdict.status {
        V1Status::Pass | V1Status::NotRun => 0,
        V1Status::Violation => 2,
        V1Status::Error => 3,
    })
}

pub(crate) fn normalize_v1_root(root: &Path) -> std::result::Result<PathBuf, String> {
    match root.canonicalize() {
        Ok(root) if root.is_dir() => Ok(root),
        Ok(root) => Err(format!("--root is not a directory: {}", root.display())),
        Err(error) => Err(format!(
            "failed to resolve --root {}: {error}",
            root.display()
        )),
    }
}

fn normalize_v1_trigger_path(root: &Path, path: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(root).ok().or_else(|| {
        if path.is_absolute() {
            path.ancestors().find_map(|ancestor| {
                (ancestor.canonicalize().ok().as_deref() == Some(root))
                    .then(|| path.strip_prefix(ancestor).ok())
                    .flatten()
            })
        } else {
            Some(path)
        }
    })?;
    let mut normalized = root.to_path_buf();
    for component in relative.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized = normalized.canonicalize().ok()?;
                if !normalized.starts_with(root) || !normalized.pop() {
                    return None;
                }
            }
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::RootDir | std::path::Component::Prefix(_) => return None,
        }
    }
    normalized.starts_with(root).then_some(normalized)
}

pub(crate) fn normalize_v1_path(root: &Path, path: &Path) -> std::result::Result<PathBuf, String> {
    if path.to_string_lossy().contains('\0') {
        return Err("v1 file paths may not contain NUL bytes".into());
    }
    let mut ancestor = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let mut missing_suffix = Vec::new();
    loop {
        let metadata = match std::fs::symlink_metadata(&ancestor) {
            Ok(metadata) => metadata,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                let Some(part) = ancestor.file_name() else {
                    return Err(format!("file path escapes --root: {}", path.display()));
                };
                missing_suffix.push(part.to_os_string());
                if !ancestor.pop() {
                    return Err(format!("file path escapes --root: {}", path.display()));
                }
                continue;
            }
            Err(error) => return Err(format!("failed to resolve {}: {error}", path.display())),
        };
        let canonical = match ancestor.canonicalize() {
            Ok(canonical) => canonical,
            Err(_) if metadata.file_type().is_symlink() => {
                return Err(format!("file path escapes --root: {}", path.display()));
            }
            Err(error) => return Err(format!("failed to resolve {}: {error}", path.display())),
        };
        if !canonical.starts_with(root) {
            return Err(format!("file path escapes --root: {}", path.display()));
        }
        if missing_suffix.iter().any(|part| part == OsStr::new("..")) {
            return Err(format!(
                "failed to resolve {}: missing ancestor",
                path.display()
            ));
        }
        if let Some(normalized) = normalize_v1_trigger_path(root, path) {
            return Ok(normalized);
        }
        let mut normalized = canonical;
        for part in missing_suffix.iter().rev() {
            normalized.push(part);
        }
        return Ok(normalized);
    }
}

pub(crate) fn emit_v1_error(format: OutputFormat, event: &str, reason: &str, code: i32) -> i32 {
    let verdict = V1Verdict::from_results(event, vec![], vec![], Some(reason.to_string()));
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&verdict).unwrap()),
        OutputFormat::Human => eprintln!("error: {reason}"),
    }
    code
}

fn emit_v1(verdict: &V1Verdict, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(verdict)?),
        OutputFormat::Human => {
            for result in &verdict.results {
                eprintln!(
                    "{}: {}",
                    result.id,
                    match result.outcome {
                        V1CheckOutcome::Pass => "pass",
                        V1CheckOutcome::Violation => "violation",
                        V1CheckOutcome::Error => "error",
                    }
                );
                if let Some(reason) = &result.reason {
                    eprintln!("error: [{}] {reason}", result.id);
                }
                if !result.stdout.is_empty() {
                    eprintln!(
                        "[{}] stdout: {}",
                        result.id,
                        String::from_utf8_lossy(&result.stdout)
                    );
                }
                if result.stdout_truncated {
                    eprintln!("[{}] stdout: truncated", result.id);
                }
                if !result.stderr.is_empty() {
                    eprintln!(
                        "[{}] stderr: {}",
                        result.id,
                        String::from_utf8_lossy(&result.stderr)
                    );
                }
                if result.stderr_truncated {
                    eprintln!("[{}] stderr: truncated", result.id);
                }
            }
            if let Some(error) = &verdict.error {
                eprintln!("error: {error}");
            }
            for skipped in &verdict.not_run {
                eprintln!("{}: not_run ({})", skipped.id, skipped.reason);
            }
            println!(
                "{}",
                match verdict.status {
                    V1Status::Pass => "pass",
                    V1Status::Violation => "violation",
                    V1Status::Error => "error",
                    V1Status::NotRun => "not_run",
                }
            );
        }
    }
    Ok(())
}
