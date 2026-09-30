use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::policy::PolicySnapshot;

/// A read-only, human-facing enumeration of exactly what trust covers.
///
/// Covers the digest itself, the number of resolved checks, and every file
/// under `.ironlint/scripts/` folded into it. The model, labels, and hash come
/// from one immutable capture, so the report needs no second policy read or
/// script walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlessedSummary {
    /// The config path as passed to [`blessed_summary`].
    pub config_path: PathBuf,
    /// The authoritative digest, `"sha256:<hex>"`, identical to what
    /// [`super::compute_hash`] would return for the same `config_path`.
    pub config_hash: String,
    /// Number of configured checks.
    pub checks: usize,
    /// Every file relative path under `.ironlint/scripts/`, sorted and deduped.
    pub scripts: Vec<String>,
    /// Human-readable trust scope: `"linked worktrees"` when the policy is
    /// eligible for worktree-family inheritance, else `"this config path"`.
    pub scope: String,
}

/// Enumerate what trust covers for `config_path`.
///
/// Reports the digest plus the resolved check count and the scripts under
/// `.ironlint/scripts/` folded into it. Read-only — never writes the store or
/// the filesystem; safe to call any time after a config parses (typically
/// right after a successful [`bless`]).
///
/// Faithful to the full trust surface [`super::compute_hash`] folds (config +
/// scripts) — a summary that silently omitted the scripts surface would
/// misrepresent what was actually blessed.
pub fn blessed_summary(config_path: &Path) -> Result<BlessedSummary> {
    let snapshot = PolicySnapshot::load(config_path)?;
    let config_hash = snapshot.hash().to_owned();
    let checks = snapshot.policy().checks().len();
    let mut scripts: Vec<_> = snapshot
        .identity
        .digests
        .iter()
        .filter_map(|(label, _)| {
            label
                .strip_prefix("scripts\0")
                .and_then(|label| label.split_once('\0'))
                .map(|(_, rel)| rel.to_owned())
        })
        .collect();
    scripts.sort();
    scripts.dedup();
    let scope = if snapshot.identity.worktree.is_some() {
        "linked worktrees"
    } else {
        "this config path"
    }
    .to_owned();

    Ok(BlessedSummary {
        config_path: config_path.to_path_buf(),
        config_hash,
        checks,
        scripts,
        scope,
    })
}

#[cfg(test)]
#[path = "tests/summary.rs"]
mod tests;
