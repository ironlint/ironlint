use crate::cli::OutputFormat;
use crate::commands::error_report::emit_error;
use anyhow::{Context, Result};
use ironlint_core::runner::{
    CheckExplain, CheckInput, CheckOptions, ExplainOutcome, IronLintEngine,
};
use ironlint_core::trust::TrustOutcome;
use ironlint_core::verdict::{Status, Verdict};
use ironlint_core::verdict::{V1Status, V1Verdict};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The POSIX shell the engine spawns checks through (`engine/gate.rs`).
pub(crate) const POSIX_SHELL: &str = "sh";

/// Probes whether a given command can be executed via `Command::new(cmd).arg
/// ("-c").arg("exit 0").status()`. Returns `false` on `ErrorKind::NotFound` or
/// any other spawn failure (permission denied, not executable). Kept simple
/// to stay well under the cognitive-complexity cap.
///
/// `doctor` calls this with `POSIX_SHELL`; tests probe a guaranteed-absent name.
pub(crate) fn shell_available(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("-c")
        .arg("exit 0")
        .status()
        .map(|_| true)
        .unwrap_or(false)
}

/// Probes whether `name` is a runnable binary on PATH by spawning
/// `name --version` with every stdio stream nulled — so its version banner
/// never leaks to the caller's terminal and it can never block reading stdin.
/// Returns `false` on `ErrorKind::NotFound` or any other spawn failure; the
/// process's own exit code is irrelevant (a binary that runs at all is
/// "present"). `--version` is a universal, side-effect-free flag that `jq`,
/// `python3`, and `sh` all exit from without reading stdin.
///
/// `doctor` calls this to surface the JSON-hook adapters' runtime deps
/// (`jq`, `python3`): if either is absent the hook fails OPEN and every edit
/// is silently un-gated. Kept simple to stay under the cognitive-complexity cap.
pub(crate) fn binary_available(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|_| true)
        .unwrap_or(false)
}

/// Message printed (to stderr) and exit-1'd at the top of `check::run` when no
/// POSIX shell is on PATH. Exit 1 (config tier), NOT 3 — 3 is InternalError,
/// which adapters fail-open on; a missing shell must fail loud, never silent.
const NO_SHELL_MSG: &str = "no POSIX shell (`sh`) found on PATH. IronLint runs checks via `sh -c`\
     \nand cannot enforce anything without it. On Windows, run IronLint inside \
     \nGit Bash or WSL. See docs/getting-started.md.";

#[allow(clippy::too_many_arguments)]
#[allow(clippy::fn_params_excessive_bools)]
pub fn run(
    file: Vec<PathBuf>,
    diff: Option<PathBuf>,
    content: Option<String>,
    format: OutputFormat,
    config: &Path,
    checks: Vec<String>,
    event: Option<String>,
    explain: bool,
    allow_external_paths: bool,
    force: bool,
    require_match: bool,
    root: Option<&Path>,
) -> Result<i32> {
    let v1_event = event
        .as_deref()
        .filter(|event| matches!(*event, "change" | "accept"));
    let config = match crate::commands::config::resolve_config(config) {
        Ok(p) => p,
        Err(msg) => {
            return Ok(match v1_event {
                Some(event) => emit_v1_error(format, event, &msg, 1),
                None => emit_error(format, &msg, 1),
            });
        }
    };
    let config = config.as_path();
    if crate::commands::config::is_versioned_config(config) {
        return run_v1(
            config,
            root.unwrap_or_else(|| Path::new(".")),
            file,
            diff.as_ref(),
            content.as_ref(),
            format,
            &checks,
            event.as_ref(),
            explain,
            force,
            require_match,
        );
    }
    if root.is_some() {
        return Ok(emit_error(format, "legacy checks do not accept --root", 1));
    }
    if matches!(event.as_deref(), Some("change" | "accept")) {
        return Ok(emit_error(
            format,
            "legacy checks accept only `write` or `pre-commit` events",
            1,
        ));
    }
    if force && checks.is_empty() {
        return Ok(emit_error(
            format,
            "--force requires at least one --check <id>",
            1,
        ));
    }
    // Fail loud when the POSIX shell the engine spawns is absent (stock
    // Windows). Without `sh` every check fails to spawn → exit 3 → adapters
    // fail open → the user "enforces" nothing. We surface this as a config-tier
    // exit 1 (not 3) so nobody is fooled into thinking enforcement is active.
    if !shell_available(POSIX_SHELL) {
        return Ok(emit_error(format, NO_SHELL_MSG, 1));
    }
    // Trust gate: refuse an unblessed or tampered config/checks before the engine
    // loads or any check runs. This hashes the config + `.ironlint/scripts/` now; a
    // write between here and check execution is a known, accepted TOCTOU window
    // (the direnv-model limitation — no file locking in 0.4).
    //
    // Exit code split (Task 3.2 / Finding C3, a sanctioned extension of the
    // locked 0/1/2/3 contract): a genuinely untrusted/tampered config gets
    // its OWN exit code, 4, distinct from exit 1 (config/parse error) — every
    // adapter previously mapped exit 1 to allow, so an untrusted config was
    // silently un-gated. A config the trust layer can't even evaluate (parse
    // error, missing extends target, ...) is not a trust decision at all; it
    // keeps exit 1 and is left for `engine.load` below to report on its own
    // terms.
    match ironlint_core::trust::check_trust(config) {
        Ok(TrustOutcome::Trusted) => {}
        Ok(TrustOutcome::Untrusted(e)) => {
            return Ok(emit_error(format, &format!("{e:#}"), 4));
        }
        Ok(TrustOutcome::Unverifiable(e)) | Err(e) => {
            return Ok(emit_error(format, &format!("{e:#}"), 1));
        }
    }
    let event_explicit = event.is_some();
    let event = event.unwrap_or_else(|| "write".to_string());
    let options = CheckOptions {
        checks: HashSet::new(),
        event,
        allow_external_paths,
        force,
    };
    let mut engine = match IronLintEngine::builder().with_options(options).load(config) {
        Ok(e) => e,
        Err(e) => return Ok(emit_error(format, &format!("{e:#}"), 1)),
    };
    if let Some(code) = validate_check_filter(&engine, &checks, format) {
        return Ok(code);
    }
    let check_filter: HashSet<String> = checks.into_iter().collect();
    engine.set_check_filter(check_filter.clone());

    if let Some(d) = diff {
        if !file.is_empty() {
            return Ok(emit_error(
                format,
                "provide exactly one of --file or --diff",
                1,
            ));
        }
        return run_diff(
            &mut engine,
            config,
            &d,
            &check_filter,
            if event_explicit {
                DiffDispatch::Explicit
            } else {
                DiffDispatch::Implicit
            },
            allow_external_paths,
            format,
            explain,
            require_match,
        );
    }
    match file.as_slice() {
        [f] => run_file(&engine, f.clone(), content, format, explain, require_match),
        [] => {
            // Bare `check` = repo-wide sweep. The sweep derives each check's
            // lifecycle from its `on:` list, so a caller-chosen event is a
            // contradiction, and `--force` (scope bypass for one file) has
            // no meaning against a walked set.
            if event_explicit {
                return Ok(emit_error(
                    format,
                    "--event requires --file or --diff (a bare sweep runs each check's own lifecycle)",
                    1,
                ));
            }
            if force {
                return Ok(emit_error(format, "--force requires --file", 1));
            }
            crate::commands::sweep::run(
                &mut engine,
                config,
                &check_filter,
                format,
                explain,
                require_match,
                allow_external_paths,
            )
        }
        _ => Ok(emit_error(
            format,
            "legacy checks accept exactly one --file",
            1,
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_v1(
    config: &Path,
    root: &Path,
    files: Vec<PathBuf>,
    diff: Option<&PathBuf>,
    content: Option<&String>,
    format: OutputFormat,
    checks: &[String],
    event: Option<&String>,
    explain: bool,
    force: bool,
    require_match: bool,
) -> Result<i32> {
    let event_name = event.map(String::as_str).unwrap_or("accept");
    if !matches!(event_name, "change" | "accept") {
        return Ok(emit_v1_error(
            format,
            event_name,
            "v1 accepts only `change` or `accept` events",
            1,
        ));
    }
    let invalid = diff.is_some()
        || content.is_some()
        || !checks.is_empty()
        || explain
        || force
        || require_match
        || (event_name == "accept" && !files.is_empty())
        || (event.is_none() && !files.is_empty());
    if invalid {
        return Ok(emit_v1_error(
            format,
            event_name,
            "v1 accepts only --event change with repeatable --file; acceptance has no file filter",
            1,
        ));
    }
    let root = match normalize_v1_root(root) {
        Ok(root) => root,
        Err(reason) => return Ok(emit_v1_error(format, event_name, &reason, 1)),
    };
    let changed = if event_name == "change" && !files.is_empty() {
        let mut paths = Vec::with_capacity(files.len());
        for file in files {
            let absolute = match normalize_v1_path(&root, &file) {
                Ok(path) => path,
                Err(reason) => return Ok(emit_v1_error(format, event_name, &reason, 1)),
            };
            paths.push(absolute.strip_prefix(&root).unwrap().to_path_buf());
        }
        Some(paths)
    } else {
        None
    };
    match ironlint_core::trust::check_trust(config) {
        Ok(TrustOutcome::Trusted) => {}
        Ok(TrustOutcome::Untrusted(e)) => {
            return Ok(emit_v1_error(format, event_name, &format!("{e:#}"), 4));
        }
        Ok(TrustOutcome::Unverifiable(e)) | Err(e) => {
            return Ok(emit_v1_error(format, event_name, &format!("{e:#}"), 1));
        }
    }
    let verdict =
        match ironlint_core::runner::evaluate_v1(config, &root, event_name, changed.as_deref()) {
            Ok(verdict) => verdict,
            Err(e) => {
                return Ok(emit_v1_error(format, event_name, &format!("{e:#}"), 1));
            }
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
        Err(e) => Err(format!("failed to resolve --root {}: {e}", root.display())),
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
            Err(error) => {
                return Err(format!("failed to resolve {}: {error}", path.display()));
            }
        };
        let canonical = match ancestor.canonicalize() {
            Ok(canonical) => canonical,
            Err(_error) if metadata.file_type().is_symlink() => {
                return Err(format!("file path escapes --root: {}", path.display()));
            }
            Err(error) => {
                return Err(format!("failed to resolve {}: {error}", path.display()));
            }
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
                if let Some(reason) = &result.reason {
                    eprintln!("error: [{}] {reason}", result.id);
                } else {
                    eprintln!("{}: {}", result.id, format_v1_outcome(result));
                }
                for (stream, output, truncated) in [
                    ("stdout", &result.stdout, result.stdout_truncated),
                    ("stderr", &result.stderr, result.stderr_truncated),
                ] {
                    if !output.is_empty() {
                        eprintln!(
                            "[{}] {stream}: {}",
                            result.id,
                            String::from_utf8_lossy(output)
                        );
                    }
                    if truncated {
                        eprintln!("[{}] {stream}: truncated", result.id);
                    }
                }
            }
            println!("{}", format_v1_status(verdict.status));
        }
    }
    Ok(())
}

fn format_v1_outcome(result: &ironlint_core::verdict::V1CheckResult) -> &'static str {
    match result.outcome {
        ironlint_core::verdict::V1CheckOutcome::Pass => "pass",
        ironlint_core::verdict::V1CheckOutcome::Violation => "violation",
        ironlint_core::verdict::V1CheckOutcome::Error => "error",
    }
}

fn format_v1_status(status: V1Status) -> &'static str {
    match status {
        V1Status::Pass => "pass",
        V1Status::Violation => "violation",
        V1Status::Error => "error",
        V1Status::NotRun => "not_run",
    }
}

fn run_file(
    engine: &IronLintEngine,
    file: PathBuf,
    content: Option<String>,
    format: OutputFormat,
    explain: bool,
    require_match: bool,
) -> Result<i32> {
    let content = match content {
        Some(c) => resolve_content_value(c)?,
        None => std::fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?,
    };
    let report = match engine.check_with_explain(CheckInput::File {
        path: file,
        content,
    }) {
        Ok(r) => r,
        Err(e) => {
            // e.g. an external path resolving outside config_dir: an
            // argument/config error, so exit 1 (mirrors the load-error path).
            return Ok(emit_error(format, &format!("{e:#}"), 1));
        }
    };
    if explain {
        print_explain(&report.explain);
    }
    emit(&report.verdict, format)?;
    Ok(exit_code(&report.verdict, require_match))
}

/// Aggregate of per-file check runs, ready to fold into one Verdict.
pub(crate) struct FoldedOutcomes {
    pub blocks: Vec<ironlint_core::verdict::Block>,
    pub errors: Vec<ironlint_core::verdict::GateError>,
    pub passed: Vec<String>,
    pub explains: Vec<CheckExplain>,
    pub elapsed_ms: u64,
}

/// Run `paths` one file at a time through the engine (write-lifecycle
/// semantics: on-disk content on stdin), folding the per-file verdicts.
/// A file we can't read (missing, permissions, non-UTF-8) is a SKIP, not a
/// hard error: fabricating empty content would run every check against ""
/// and let a real violation pass vacuously, but aborting the whole batch
/// would hide a real Block in a sibling file. Record the skip, warn loudly,
/// and move on. Extracted from `run_diff` so the bare-sweep path reuses the
/// identical fold.
pub(crate) fn check_files_individually(
    engine: &IronLintEngine,
    paths: &[PathBuf],
) -> Result<FoldedOutcomes> {
    let mut out = FoldedOutcomes {
        blocks: Vec::new(),
        errors: Vec::new(),
        passed: Vec::new(),
        explains: Vec::new(),
        elapsed_ms: 0,
    };
    for path in paths {
        let content = match read_changed_file(path) {
            Ok(c) => c,
            Err(reason) => {
                let label = reason.label();
                eprintln!(
                    "WARNING: skipping file {} ({label}): {reason}",
                    path.display()
                );
                out.explains.push(CheckExplain {
                    check_id: path.display().to_string(),
                    outcome: ExplainOutcome::Skipped {
                        reason: label.to_string(),
                    },
                });
                continue;
            }
        };
        let r = engine.check_with_explain(CheckInput::File {
            path: path.clone(),
            content,
        })?;
        out.elapsed_ms = out.elapsed_ms.saturating_add(r.verdict.elapsed_ms);
        out.blocks.extend(r.verdict.blocks);
        out.errors.extend(r.verdict.errors);
        out.passed.extend(r.verdict.passed);
        out.explains.extend(r.explain);
    }
    Ok(out)
}

/// How `--diff` dispatch interprets the check set, decided once at the CLI
/// boundary. `Explicit` = the caller pinned `--event` (single-lifecycle
/// semantics); `Implicit` = bare `--diff` (the git pre-commit floor invokes
/// it this way), so each check dispatches per its own `on:` lifecycle.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DiffDispatch {
    Explicit,
    Implicit,
}

/// Check every non-deleted changed file in a unified diff. Checks read each
/// file's current on-disk content (checks don't consume diffs).
///
/// Dispatch depends on [`DiffDispatch`]:
///
/// - **Explicit** (single-lifecycle, unchanged semantics): `pre-commit` runs
///   each check once over the set (`check_set`, run-once); `write` runs each
///   check per changed file through `check_with_explain`.
/// - **Implicit**: each check dispatches per its own `on:` lifecycle,
///   mirroring the bare sweep — write-lifecycle checks once per matching
///   file, pre-commit-lifecycle checks once over the whole set. A dual-
///   lifecycle check is batched only, so it is never double-run.
#[allow(clippy::too_many_arguments)]
fn run_diff(
    engine: &mut IronLintEngine,
    config: &Path,
    diff: &Path,
    user_checks: &HashSet<String>,
    dispatch: DiffDispatch,
    allow_external_paths: bool,
    format: OutputFormat,
    explain: bool,
    require_match: bool,
) -> Result<i32> {
    let unified = std::fs::read_to_string(diff)?;
    let changed = ironlint_core::diff::parser::parse_unified(&unified)?;
    if changed.is_empty() {
        return Ok(emit_error(format, "no changed files in diff", 1));
    }
    let non_deleted: Vec<PathBuf> = changed
        .iter()
        .filter(|f| f.op != ironlint_core::diff::ChangeOp::Deleted)
        .map(|f| f.path.clone())
        .collect();

    // Explicit event: single-lifecycle dispatch, unchanged semantics.
    if dispatch == DiffDispatch::Explicit {
        // Pre-commit: run each check once over the entire changed set.
        if engine.event() == "pre-commit" {
            let verdict = engine.check_set(&non_deleted)?;
            emit(&verdict, format)?;
            return Ok(exit_code(&verdict, require_match));
        }

        // Write (and any future per-file event): loop once per changed file.
        let folded = match check_files_individually(engine, &non_deleted) {
            Ok(f) => f,
            Err(e) => return Ok(emit_error(format, &format!("{e:#}"), 1)),
        };
        let verdict = Verdict::from_outcomes(
            folded.blocks,
            folded.errors,
            folded.passed,
            folded.elapsed_ms,
        );
        if explain {
            print_explain(&folded.explains);
        }
        emit(&verdict, format)?;
        return Ok(exit_code(&verdict, require_match));
    }

    // Implicit: per-check lifecycle dispatch, mirroring the bare sweep.
    let classes = crate::commands::sweep::classify_checks(engine.checks(), user_checks);
    let mut blocks = Vec::new();
    let mut errors = Vec::new();
    let mut passed = Vec::new();
    let mut explains = Vec::new();
    let mut elapsed: u64 = 0;

    // Phase 1 — write-lifecycle checks, one invocation per matching file.
    if !classes.per_file.is_empty() {
        engine.set_check_filter(classes.per_file.clone());
        // Prune to files at least one phase-1 check scopes to; without this,
        // every changed file would produce a no-op engine call and a
        // telemetry row (same reasoning as the sweep's phase 1).
        let scoped: Vec<PathBuf> = non_deleted
            .iter()
            .filter(|f| {
                classes
                    .per_file
                    .iter()
                    .any(|id| engine.check_matches_path(id, f))
            })
            .cloned()
            .collect();
        match check_files_individually(engine, &scoped) {
            Ok(folded) => {
                blocks.extend(folded.blocks);
                errors.extend(folded.errors);
                passed.extend(folded.passed);
                explains.extend(folded.explains);
                elapsed = elapsed.saturating_add(folded.elapsed_ms);
            }
            Err(e) => return Ok(emit_error(format, &format!("{e:#}"), 1)),
        }
    }

    // Phase 2 — pre-commit-lifecycle checks: ONE invocation per check over
    // the whole changed set (the engine scope-filters per check). A second
    // engine is loaded because the event is fixed at load time; the trust
    // gate already ran for this config path in `check::run`.
    if !classes.batched.is_empty() {
        let options = CheckOptions {
            checks: classes.batched.clone(),
            event: "pre-commit".to_string(),
            allow_external_paths,
            force: false,
        };
        let batch_engine = match IronLintEngine::builder().with_options(options).load(config) {
            Ok(e) => e,
            // Defensive: `config` already parsed and loaded successfully once
            // in `check::run` for this exact path, so this reload isn't
            // expected to fail in practice. Kept as a real error path (not
            // unwrapped) in case that invariant ever breaks.
            Err(e) => return Ok(emit_error(format, &format!("{e:#}"), 1)),
        };
        match batch_engine.check_set(&non_deleted) {
            Ok(v) => {
                elapsed = elapsed.saturating_add(v.elapsed_ms);
                blocks.extend(v.blocks);
                errors.extend(v.errors);
                passed.extend(v.passed);
            }
            // Defensive: `check_set` currently has no fallible path; kept so
            // a future fallible variant isn't silently swallowed.
            Err(e) => return Ok(emit_error(format, &format!("{e:#}"), 1)),
        }
    }

    let verdict = Verdict::from_outcomes(blocks, errors, passed, elapsed);
    if explain {
        print_explain(&explains);
    }
    emit(&verdict, format)?;
    Ok(exit_code(&verdict, require_match))
}

/// Why a diff-referenced file couldn't be turned into check input. Kept
/// separate from `Skipped`/`Fire`/`Pass` in [`ExplainOutcome`] — a read
/// failure happens before the engine ever sees the file, so it's classified
/// here and folded into the engine's existing skip vocabulary at the call
/// site rather than growing a new one.
enum SkipReason {
    /// `read_to_string` failed with `ErrorKind::InvalidData` — the file's
    /// bytes are not valid UTF-8 (image, UTF-16, other binary fixture).
    NonUtf8,
    /// Any other read failure (deleted between diff-gen and check,
    /// permissions, ...). Carries the io error kind for the stderr note.
    Unreadable(std::io::ErrorKind),
}

impl SkipReason {
    /// Stable, matchable reason string — surfaced in both the stderr warning
    /// and the `ExplainOutcome::Skipped { reason }` row.
    fn label(&self) -> &'static str {
        match self {
            Self::NonUtf8 => "non_utf8",
            Self::Unreadable(_) => "unreadable",
        }
    }
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonUtf8 => write!(f, "not valid UTF-8"),
            Self::Unreadable(kind) => write!(f, "could not be read: {kind}"),
        }
    }
}

/// Read a diff-referenced file's on-disk content, classifying any failure
/// into a [`SkipReason`] instead of a hard error. Extracted so the `run_diff`
/// loop body stays under the cognitive-complexity cap.
fn read_changed_file(path: &Path) -> Result<String, SkipReason> {
    std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::InvalidData {
            SkipReason::NonUtf8
        } else {
            SkipReason::Unreadable(e.kind())
        }
    })
}

fn validate_check_filter(
    engine: &IronLintEngine,
    checks: &[String],
    format: OutputFormat,
) -> Option<i32> {
    if checks.is_empty() {
        return None;
    }
    let known: HashSet<&str> = engine.check_ids().collect();
    let unknown: Vec<&str> = checks
        .iter()
        .map(|s| s.as_str())
        .filter(|id| !known.contains(id))
        .collect();
    if unknown.is_empty() {
        None
    } else {
        Some(emit_error(
            format,
            &format!("unknown check id(s): {}", unknown.join(", ")),
            1,
        ))
    }
}

pub(crate) fn print_explain(rows: &[CheckExplain]) {
    for row in rows {
        let outcome = match &row.outcome {
            ExplainOutcome::Fire => "fire".to_string(),
            ExplainOutcome::Pass => "pass".to_string(),
            ExplainOutcome::Skipped { reason } => format!("skipped {reason}"),
        };
        eprintln!("{} {}", row.check_id, outcome);
    }
}

pub(crate) fn exit_code(v: &Verdict, require_match: bool) -> i32 {
    let no_match = v.status == Status::Pass
        && v.passed.is_empty()
        && v.blocks.is_empty()
        && v.errors.is_empty();
    match v.status {
        Status::Block => 2,
        Status::InternalError => 3,
        Status::Pass if no_match && require_match => 2,
        _ => 0,
    }
}

pub(crate) fn emit(v: &Verdict, format: OutputFormat) -> Result<()> {
    let no_match = v.status == Status::Pass
        && v.passed.is_empty()
        && v.blocks.is_empty()
        && v.errors.is_empty();
    match format {
        OutputFormat::Json => {
            // JSON mode: no extra stdout (the verdict already carries
            // passed=[] for a no-match run; CI detects it from the shape).
            // --require-match still affects the EXIT code, handled by the
            // caller via exit_code(v, require_match).
            println!("{}", serde_json::to_string_pretty(v)?);
        }
        OutputFormat::Human => {
            for b in &v.blocks {
                eprintln!("block: [{}] {}", b.check, b.file.as_deref().unwrap_or(""));
                eprintln!("  {}", b.message);
            }
            for e in &v.errors {
                eprintln!(
                    "error: [{}] {} ({})",
                    e.check,
                    e.file.as_deref().unwrap_or(""),
                    e.reason
                );
                if let Some(d) = &e.detail {
                    eprintln!("  {d}");
                }
            }
            let line = match v.status {
                Status::Pass if no_match => "pass (no checks matched)",
                Status::Pass => "pass",
                Status::Block => "block",
                Status::InternalError => "internal_error",
                _ => "unknown",
            };
            println!("{line}");
        }
    }
    Ok(())
}

fn resolve_content_value(value: String) -> Result<String> {
    if value == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("failed to read --content from stdin (expected UTF-8)")?;
        Ok(buf)
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A command name that cannot exist on PATH, so `shell_available` reports
    /// the unavailable path. Keeps the probe honest on a machine where `sh`
    /// does exist (macOS/Linux): we can't remove `sh`, but we can probe a name
    /// that is guaranteed not to resolve.
    const NO_SUCH_SHELL: &str = "ironlint-definitely-not-a-real-shell-xyz123";

    #[test]
    fn shell_available_false_for_nonexistent_command() {
        assert!(!shell_available(NO_SUCH_SHELL));
    }

    #[cfg(unix)]
    #[test]
    fn shell_available_true_for_sh_on_unix() {
        // On a Unix dev/CI machine `sh` is always present; this documents the
        // happy path and guards against a regression that breaks the probe.
        assert!(shell_available("sh"));
    }

    #[test]
    fn binary_available_false_for_nonexistent_command() {
        assert!(!binary_available(NO_SUCH_SHELL));
    }

    #[cfg(unix)]
    #[test]
    fn binary_available_true_for_sh_on_unix() {
        // `sh` is always present on a Unix dev/CI box; the generic PATH probe
        // must resolve it regardless of what `sh --version` prints or exits.
        assert!(binary_available("sh"));
    }
}
