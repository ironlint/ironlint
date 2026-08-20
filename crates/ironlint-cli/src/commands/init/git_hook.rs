//! The git pre-commit "floor" hook installed by `ironlint init` and removed
//! by `ironlint init --uninstall` (specs/2026-08-17-git-floor-hook-and-self-defense-design.md,
//! W1).
//!
//! One hook covers every committer — supported agents, unsupported agents,
//! humans — independent of any harness adapter being installed, present, or
//! alive: git itself fires it at the `git commit` boundary, and it runs
//! `ironlint check --diff` over the staged change set.
//!
//! Chaining, not capture: the hook is a marker-bracketed block appended into
//! `.git/hooks/pre-commit` (via `git rev-parse --git-common-dir`, so linked
//! worktrees inherit the floor automatically). A pre-existing hook runs
//! first; if it exits nonzero git aborts before our block, which is the
//! correct precedence. Reinstall replaces the marker block in place — second
//! installs are byte-identical when the binary path is unchanged.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const MARKER_START: &str = "# >>> ironlint pre-commit floor >>>";
pub const MARKER_END: &str = "# <<< ironlint pre-commit floor <<<";
const SHEBANG: &str = "#!/bin/sh";

/// Outcome of an install/uninstall operation, carrying the hook path so the
/// caller can print it in the init plan/summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookState {
    /// Freshly created (or marker-appended into an unmarked existing hook).
    Installed(PathBuf),
    /// Marker block replaced / removed, file otherwise preserved.
    Updated(PathBuf),
    /// Reinstall with an unchanged binary path; file byte-identical.
    AlreadyPresent(PathBuf),
    /// The project dir is not inside a git work tree (or git is absent).
    Skipped(String),
    /// The hook file was flushed away entirely (only our block existed).
    Removed(PathBuf),
    /// No marker block found to remove.
    NotFound,
}

/// ANSI-free labels for the init plan/summary lines ("git floor:").
pub fn summarize(state: &HookState, uninstalling: bool) -> String {
    match state {
        HookState::Installed(p) => {
            format!(
                "installed {}{}",
                p.display(),
                if uninstalling { " (would)" } else { "" }
            )
        }
        HookState::Updated(p) if uninstalling => {
            format!("marker removed from {}", p.display())
        }
        HookState::Updated(p) => format!("updated {}", p.display()),
        HookState::AlreadyPresent(p) => format!("already present {}", p.display()),
        HookState::Skipped(why) => format!("skipped: {why}"),
        HookState::Removed(p) => format!("removed {}", p.display()),
        HookState::NotFound => "not installed".to_string(),
    }
}

/// Resolve the hook file (`<git-common-dir>/hooks/pre-commit`) for
/// `project_dir`. `Ok(None)` when `project_dir` is not inside a git work tree
/// (`git rev-parse --git-common-dir` fails) — the caller reports a skip.
pub fn hook_path(project_dir: &Path) -> Result<Option<PathBuf>> {
    let out = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(project_dir)
        .output()
        .context("failed to spawn `git rev-parse --git-common-dir`")?;
    if !out.status.success() {
        return Ok(None);
    }
    let dir = String::from_utf8(out.stdout)
        .map_err(|_| anyhow!("`git rev-parse --git-common-dir` printed non-UTF-8"))?;
    let dir = dir.trim();
    if dir.is_empty() {
        return Ok(None);
    }
    let common = PathBuf::from(dir);
    let common = if common.is_absolute() {
        common
    } else {
        project_dir.join(common)
    };
    Ok(Some(common.join("hooks").join("pre-commit")))
}

/// Install (or refresh) the floor hook. Semantics per the marker table:
/// absent → create with shebang + marker block, `chmod +x`; present with
/// marker → replace the block in place (idempotent reinstall / binary-path
/// update); present without marker → append the block (existing content
/// runs first).
pub fn install(project_dir: &Path, bin: &Path) -> Result<HookState> {
    let Some(hook) = hook_path(project_dir)? else {
        return Ok(HookState::Skipped(
            "not inside a git work tree (no floor installed)".into(),
        ));
    };
    let block = hook_block(bin);
    match std::fs::read(&hook) {
        Ok(bytes) => {
            let existing = String::from_utf8(bytes).map_err(|_| {
                anyhow!(
                    "existing {} is not UTF-8; refusing to touch it",
                    hook.display()
                )
            })?;
            match replace_marker(&existing, &block) {
                Some(next) if next == existing => Ok(HookState::AlreadyPresent(hook)),
                Some(next) => {
                    write_exec(&hook, next.as_bytes())?;
                    Ok(HookState::Updated(hook))
                }
                None => {
                    let mut next = existing;
                    if !next.ends_with('\n') {
                        next.push('\n');
                    }
                    next.push_str(&block);
                    next.push('\n');
                    write_exec(&hook, next.as_bytes())?;
                    Ok(HookState::Installed(hook))
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let body = format!("{SHEBANG}\n{block}\n");
            write_exec(&hook, body.as_bytes())?;
            Ok(HookState::Installed(hook))
        }
        Err(e) => Err(e).with_context(|| format!("failed to read {}", hook.display())),
    }
}

/// Remove the marker block. If the remaining hook is empty modulo
/// shebang/whitespace, delete the file; otherwise preserve the pre-existing
/// content byte-for-byte.
pub fn uninstall(project_dir: &Path) -> Result<HookState> {
    let Some(hook) = hook_path(project_dir)? else {
        return Ok(HookState::Skipped(
            "not inside a git work tree (no floor to remove)".into(),
        ));
    };
    match std::fs::read_to_string(&hook) {
        Ok(existing) => {
            let Some(start) = existing.find(MARKER_START) else {
                return Ok(HookState::NotFound);
            };
            let end_rel = existing[start..].find(MARKER_END).ok_or_else(|| {
                anyhow!(
                    "{} has a floor start marker but no end marker; refusing to guess",
                    hook.display()
                )
            })?;
            let end = start + end_rel + MARKER_END.len();
            let rest = format!("{}{}", &existing[..start], &existing[end..]);
            if is_deletable(&rest) {
                std::fs::remove_file(&hook)
                    .with_context(|| format!("failed to remove {}", hook.display()))?;
                Ok(HookState::Removed(hook))
            } else {
                std::fs::write(&hook, rest)
                    .with_context(|| format!("failed to write {}", hook.display()))?;
                Ok(HookState::Updated(hook))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HookState::NotFound),
        Err(e) => Err(e).with_context(|| format!("failed to read {}", hook.display())),
    }
}

/// The marker-bracketed hook body (no shebang, no trailing newline). The
/// `<bin>` slot is single-quote-shell-escaped, so a binary path containing
/// spaces or quotes survives; `BIN` is seeded from `$IRONLINT_BIN` first so
/// a test (or an operator) can override the recorded path without editing
/// the hook.
pub fn hook_block(bin: &Path) -> String {
    format!(
        "{MARKER_START}\n\
         # Managed by ironlint init — remove with `ironlint init --uninstall`.\n\
         diff_file=\"${{TMPDIR:-/tmp}}/ironlint-precommit.$$\"\n\
         trap 'rm -f \"$diff_file\"' EXIT\n\
         git diff --cached --diff-filter=ACMR --no-color --src-prefix=a/ --dst-prefix=b/ >\"$diff_file\" || exit 0\n\
         [ -s \"$diff_file\" ] || exit 0\n\
         BIN=\"${{IRONLINT_BIN:-}}\"\n\
         [ -n \"$BIN\" ] || BIN={0}\n\
         [ -x \"$BIN\" ] || BIN=\"$(command -v ironlint || true)\"\n\
         if [ -z \"$BIN\" ]; then\n\
           echo \"ironlint floor: binary not found; allowing commit (reinstall: ironlint init)\" >&2\n\
           exit 0\n\
         fi\n\
                  # Round-2 review: a deleted .ironlint.yml would otherwise block every\n\
         # commit with exit 1 forever (spec-locked config-error mapping).\n\
         # Zero-cost in non-ironlint repos; unwedges the config-deleted case.\n\
         [ -f .ironlint.yml ] || exit 0\n\
\"$BIN\" check --diff \"$diff_file\"\n\
         code=$?\n\
         case \"$code\" in\n\
           0) exit 0 ;;\n\
           3) if [ \"${{IRONLINT_FAIL_CLOSED_ON_INTERNAL:-0}}\" = \"1\" ]; then\n\
                echo \"ironlint floor: internal error — blocking (IRONLINT_FAIL_CLOSED_ON_INTERNAL=1)\" >&2\n\
                exit 1\n\
              fi\n\
              echo \"ironlint floor: internal error — allowing commit\" >&2\n\
              exit 0 ;;\n\
           4) echo \"ironlint floor: config/checks untrusted — run: ironlint trust\" >&2\n\
              exit 1 ;;\n\
           *) exit \"$code\" ;;\n\
         esac\n\
         {MARKER_END}",
        sh_quote(bin)
    )
}

/// Single-quote a path for embedding in the `BIN='...'` assignment inside
/// the hook: wrap in single quotes, escaping embedded single quotes per the
/// POSIX `'\''` idiom.
fn sh_quote(s: &Path) -> String {
    format!("'{}'", s.to_string_lossy().replace('\'', "'\\''"))
}

/// Replace the marker-bracketed region of `existing` with `block`, returning
/// `None` when no start marker is present. Text before the start marker and
/// after the end marker is preserved byte-for-byte.
fn replace_marker(existing: &str, block: &str) -> Option<String> {
    let start = existing.find(MARKER_START)?;
    let end_rel = existing[start..].find(MARKER_END)?;
    let end = start + end_rel + MARKER_END.len();
    Some(format!(
        "{}{}{}",
        &existing[..start],
        block,
        &existing[end..]
    ))
}

/// True when `s` is empty, whitespace-only, or a lone shebang line — the
/// residue of an ironlint-only hook whose marker was removed. Such a file is
/// deleted rather than left as dead weight.
fn is_deletable(s: &str) -> bool {
    let mut meaningful = 0;
    for line in s.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        meaningful += 1;
        if meaningful > 1 || !t.starts_with("#!") {
            return false;
        }
    }
    true
}

/// Write `bytes` to `path` and mark it executable (POSIX `0o755`); on
/// non-unix the write alone happens (the repo claims no extra Windows
/// support).
fn write_exec(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("failed to chmod +x {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a throwaway git repo, returning the worktree dir.
    fn git_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let out = Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "git init failed in test env");
        dir
    }

    fn hook_file(dir: &Path) -> PathBuf {
        Command::new("git")
            .args(["rev-parse", "--git-common-dir"])
            .current_dir(dir)
            .output()
            .map(|o| {
                let d = String::from_utf8(o.stdout).unwrap().trim().to_string();
                if Path::new(&d).is_absolute() {
                    PathBuf::from(d).join("hooks").join("pre-commit")
                } else {
                    dir.join(d).join("hooks").join("pre-commit")
                }
            })
            .unwrap()
    }

    #[test]
    fn hook_path_none_outside_git_repo() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(hook_path(dir.path()).unwrap(), None);
    }

    #[test]
    fn install_absent_creates_executable_shebang_hook() {
        let dir = git_dir();
        let bin = PathBuf::from("/usr/local/bin/ironlint");
        let state = install(dir.path(), &bin).unwrap();
        let hook = hook_file(dir.path());
        assert_eq!(state, HookState::Installed(hook.clone()));
        let body = std::fs::read_to_string(&hook).unwrap();
        assert!(body.starts_with("#!/bin/sh\n"));
        assert!(body.contains(MARKER_START) && body.contains(MARKER_END));
        assert!(body.contains(&bin.to_string_lossy().to_string()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&hook).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "hook must be executable");
        }
    }

    #[test]
    fn reinstall_same_bin_is_byte_identical() {
        let dir = git_dir();
        let bin = PathBuf::from("/opt/ironlint/ironlint");
        assert!(matches!(
            install(dir.path(), &bin).unwrap(),
            HookState::Installed(_)
        ));
        let hook = hook_file(dir.path());
        let first = std::fs::read(&hook).unwrap();
        assert_eq!(
            install(dir.path(), &bin).unwrap(),
            HookState::AlreadyPresent(hook.clone())
        );
        assert_eq!(
            first,
            std::fs::read(&hook).unwrap(),
            "must be byte-identical"
        );
    }

    #[test]
    fn reinstall_changed_bin_replaces_only_marker_block() {
        let dir = git_dir();
        let hook = hook_file(dir.path());
        // Custom content sits ABOVE the floor's marker block (a user-edited
        // hook). Reinstalling with a different binary path must replace only
        // the marker region, preserving the custom content byte-for-byte.
        let block_a = hook_block(Path::new("/opt/a/bin"));
        std::fs::write(
            &hook,
            format!("#!/bin/sh\n# my own line\necho hi\n{block_a}\n"),
        )
        .unwrap();
        let state = install(dir.path(), Path::new("/opt/b/bin")).unwrap();
        let body = std::fs::read_to_string(&hook).unwrap();
        assert_eq!(state, HookState::Updated(hook.clone()));
        assert!(
            body.starts_with("#!/bin/sh\n# my own line\necho hi\n"),
            "custom content must survive the replace: {body}"
        );
        assert!(body.contains("/opt/b/bin"));
        assert!(!body.contains("/opt/a/bin"));
        assert!(
            body.trim_end().ends_with(MARKER_END),
            "trailing marker lost: {body}"
        );
    }

    #[test]
    fn install_appends_when_unmarked_hook_exists_and_preserves_it() {
        let dir = git_dir();
        let hook = hook_file(dir.path());
        std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
        std::fs::write(&hook, "#!/bin/sh\necho \"custom hook\"\n").unwrap();
        let state = install(dir.path(), Path::new("/bin/ironlint")).unwrap();
        assert_eq!(state, HookState::Installed(hook.clone()));
        let body = std::fs::read_to_string(&hook).unwrap();
        assert!(
            body.starts_with("#!/bin/sh\necho \"custom hook\"\n"),
            "{body}"
        );
        assert!(body.contains(MARKER_START));
        // Custom content precedes the marker block.
        assert!(
            body.find("custom hook").unwrap() < body.find(MARKER_START).unwrap(),
            "custom content must run before the floor"
        );
    }

    #[test]
    fn uninstall_removes_marker_and_preserves_preexisting_content() {
        let dir = git_dir();
        install(dir.path(), Path::new("/bin/ironlint")).unwrap();
        let hook = hook_file(dir.path());
        std::fs::write(
            &hook,
            std::fs::read_to_string(&hook).unwrap() + "\nexit 0\n",
        )
        .unwrap();
        let state = uninstall(dir.path()).unwrap();
        assert_eq!(state, HookState::Updated(hook.clone()));
        let body = std::fs::read_to_string(&hook).unwrap();
        assert!(!body.contains(MARKER_START) && !body.contains(MARKER_END));
        assert!(body.contains("exit 0\n"), "custom tail lost: {body}");
        assert!(hook.exists());
    }

    #[test]
    fn uninstall_deletes_hook_when_only_floor_remains() {
        let dir = git_dir();
        install(dir.path(), Path::new("/bin/ironlint")).unwrap();
        let hook = hook_file(dir.path());
        // Reinstate the canonical ironlint-only shape: shebang + block.
        let block = hook_block(Path::new("/bin/ironlint"));
        std::fs::write(&hook, format!("#!/bin/sh\n{block}\n")).unwrap();
        let state = uninstall(dir.path()).unwrap();
        assert_eq!(state, HookState::Removed(hook.clone()));
        assert!(!hook.exists());
    }

    #[test]
    fn uninstall_without_marker_reports_not_found() {
        let dir = git_dir();
        let hook = hook_file(dir.path());
        std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
        std::fs::write(&hook, "#!/bin/sh\necho custom\n").unwrap();
        assert_eq!(uninstall(dir.path()).unwrap(), HookState::NotFound);
        assert!(hook.exists(), "must not touch a hook without our marker");
        assert_eq!(
            std::fs::read_to_string(&hook).unwrap(),
            "#!/bin/sh\necho custom\n"
        );
    }

    #[test]
    fn uninstall_missing_file_reports_not_found() {
        let dir = git_dir();
        assert_eq!(uninstall(dir.path()).unwrap(), HookState::NotFound);
    }

    #[test]
    fn is_deletable_flags_empty_shebang_and_whitespace() {
        assert!(is_deletable(""));
        assert!(is_deletable("\n\n  \n"));
        assert!(is_deletable("#!/bin/sh\n"));
        assert!(is_deletable("#!/bin/sh\n\n  \n"));
        assert!(!is_deletable("echo hi\n"));
        assert!(!is_deletable("#!/bin/sh\necho hi\n"));
    }

    #[test]
    fn replace_marker_splices_and_preserves_edges() {
        let block = hook_block(Path::new("/bin/ironlint"));
        let existing = format!("pre\n{block}\npost\n");
        let out = replace_marker(&existing, &block).unwrap();
        // Identical block → recomposition must round-trip.
        assert_eq!(out, existing);
        // Different block only swaps the middle.
        let out2 = replace_marker(
            &existing,
            "# >>> ironlint pre-commit floor >>>\nNEW\n# <<< ironlint pre-commit floor <<<",
        )
        .unwrap();
        assert!(out2.starts_with("pre\n# >>> ironlint pre-commit floor >>>\nNEW\n"));
        assert!(
            out2.ends_with("# <<< ironlint pre-commit floor <<<\npost\n"),
            "{out2}"
        );
        // No start marker → None.
        assert_eq!(replace_marker("no marker here", &block), None);
        // Start marker without end marker → None.
        assert_eq!(
            replace_marker(&format!("{MARKER_START}\ndangling"), &block),
            None
        );
    }

    #[test]
    fn hook_block_pins_exit_mapping_contract() {
        let block = hook_block(Path::new("/bin/ironlint"));
        // W1-R4 exit mapping is locked: 2/1/4 block (4 with trust
        // remediation), 3 fail-open honoring IRONLINT_FAIL_CLOSED_ON_INTERNAL.
        assert!(block.contains(
            "git diff --cached --diff-filter=ACMR --no-color --src-prefix=a/ --dst-prefix=b/"
        ));
        assert!(block.contains("check --diff \"$diff_file\""));
        assert!(block.contains("IRONLINT_FAIL_CLOSED_ON_INTERNAL"));
        assert!(block.contains("ironlint trust"));
        assert!(block.contains("*) exit \"$code\""));
        assert!(block.contains("binary not found; allowing commit"));
    }

    #[test]
    fn hook_block_quotes_bin_path() {
        let block = hook_block(Path::new("/opt/iron lint/bin"));
        assert!(block.contains("BIN='/opt/iron lint/bin'"), "{block}");
    }
}
