use std::path::Path;

use crate::commands::init::git_hook::{hook_block, hook_path, MARKER_END, MARKER_START};

use super::{CheckResult, Status};

/// Inspect the optional hook's managed block. Its contents are never executed;
/// user-owned prefix/suffix commands do not affect the managed block's status.
pub(super) fn check_git_hook(dir: &Path) -> Option<CheckResult> {
    let path = match hook_path(dir) {
        Ok(Some(path)) => path,
        Ok(None) => return None,
        Err(error) => {
            return Some(failure(format!(
                "could not locate optional Git hook: {error:#}"
            )))
        }
    };
    let body = match std::fs::read_to_string(&path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            return Some(failure(format!(
                "reading Git hook {}: {error}",
                path.display()
            )))
        }
    };
    let (status, detail, remediation) = inspect_body(&body);
    let row = CheckResult {
        name: "git_hook",
        status,
        detail: format!("{}: {detail}", path.display()),
        remediation,
    };
    #[cfg(unix)]
    let row = with_execute_access(row, &path);
    Some(row)
}

#[cfg(unix)]
fn with_execute_access(mut row: CheckResult, path: &Path) -> CheckResult {
    if row.status == Status::Pass {
        use nix::unistd::{access, AccessFlags};
        if let Err(error) = access(path, AccessFlags::X_OK) {
            row.status = Status::Fail;
            row.detail.push_str(&format!(
                "; not executable for the current user ({error}); Git ignores an inactive hook"
            ));
            let quoted = path.to_string_lossy().replace('\'', "'\\''");
            row.remediation = Some(format!("run `chmod u+x '{quoted}'`"));
        }
    }
    row
}

fn failure(detail: String) -> CheckResult {
    CheckResult {
        name: "git_hook",
        status: Status::Fail,
        detail,
        remediation: Some(
            "inspect the Git hook path and error; repair it before reinstalling".into(),
        ),
    }
}

fn inspect_body(body: &str) -> (Status, &'static str, Option<String>) {
    let starts = body.matches(MARKER_START).count();
    let ends = body.matches(MARKER_END).count();
    if starts == 0 && ends == 0 {
        return (
            Status::Warn,
            "not managed by IronLint; user hook preserved",
            None,
        );
    }
    if starts != 1 || ends != 1 {
        return (
            Status::Fail,
            "ambiguous IronLint managed markers",
            Some("repair the managed markers; then run `ironlint init --git-hook`".into()),
        );
    }
    let start = body.find(MARKER_START).expect("one start marker");
    let end = body.find(MARKER_END).expect("one end marker");
    if end <= start {
        return (
            Status::Fail,
            "IronLint managed markers are out of order",
            Some("repair the managed markers; then run `ironlint init --git-hook`".into()),
        );
    }
    let managed = &body[start..end + MARKER_END.len()];
    if current_block(managed) {
        return (Status::Pass, "managed complete v1 acceptance hook", None);
    }
    let obsolete = [
        " diff ",
        "--event tool-edit",
        "--diff",
        "pre-tool-use",
        "gate-bash",
    ]
    .iter()
    .any(|command| {
        managed
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .any(|line| line.contains(command))
    });
    if obsolete {
        return (
            Status::Warn,
            "obsolete managed IronLint hook command",
            Some("run `ironlint init --git-hook` to refresh the owned block".into()),
        );
    }
    (
        Status::Warn,
        "modified managed IronLint hook block",
        Some("review the managed block; then run `ironlint init --git-hook`".into()),
    )
}

fn current_block(block: &str) -> bool {
    const PREFIX: &str = "[ -n \"$BIN\" ] || BIN=";
    let Some(quoted) = block.lines().find_map(|line| line.strip_prefix(PREFIX)) else {
        return false;
    };
    let Some(contents) = quoted
        .strip_prefix('\'')
        .and_then(|text| text.strip_suffix('\''))
    else {
        return false;
    };
    // Accept only the quoting emitted by hook_block, then compare the entire
    // block. A command merely mentioning acceptance is not a healthy hook.
    if contents.replace("'\\''", "").contains('\'') {
        return false;
    }
    hook_block(Path::new(&contents.replace("'\\''", "'"))) == block
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_block_accepts_quoted_binary_paths() {
        for path in [
            "/opt/iron lint/bin",
            "/opt/Chris's/bin",
            "/opt/pre-tool-use--diff/bin",
        ] {
            let block = hook_block(Path::new(path));
            assert!(current_block(&block));
            assert_eq!(inspect_body(&block).0, Status::Pass);
        }
    }

    #[test]
    fn current_block_rejects_injected_or_incomplete_commands() {
        let block = hook_block(Path::new("/bin/ironlint"));
        assert!(!current_block(
            &block.replace("BIN='/bin/ironlint'", "BIN='/bin/ironlint'; true")
        ));
        assert!(!current_block(&block.replace(" || exit \"$?\"", "")));
    }
}
