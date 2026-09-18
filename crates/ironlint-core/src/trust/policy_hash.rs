use crate::adapter::sha256_digest_hex;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use super::worktree::WorktreeScope;

/// Feed one labeled blob into the hasher with length prefixes on both the
/// label and the content, so no two distinct (label, bytes) pairs can collide
/// by concatenation.
fn hash_entry(hasher: &mut Sha256, label: &str, bytes: &[u8]) {
    hasher.update((label.len() as u64).to_le_bytes());
    hasher.update(label.as_bytes());
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

/// Filesystem classification of a path in the scripts hash walk, computed via
/// `symlink_metadata` (which does **not** follow symlinks) rather than
/// `is_dir()`/`is_file()` (which do).
#[derive(Debug)]
pub(super) enum EntryKind {
    Dir,
    File,
    Missing,
}

/// Classify `path` for the scripts hash walk without ever following a
/// symlink. This walk runs on **unblessed** repo content before the trust
/// verdict is decided — it is the security boundary — so an unusual entry
/// is a hard error rather than a silent skip: a skipped file is un-hashed
/// and thus not trust-covered, which is worse than refusing to proceed.
/// Concretely this refuses:
/// - a symlink (a self-referencing symlink would otherwise recurse
///   indefinitely; a symlink to a FIFO would block a later `fs::read`
///   forever; a symlink to a device could read unbounded data), and
/// - any other non-regular file (FIFO, socket, device, ...).
///
/// A missing path is not an error — the caller decides what "missing"
/// means for its position in the walk (e.g. an absent scripts dir has
/// nothing to hash). A path whose parent isn't even a directory (e.g. a
/// plain file sits where `.ironlint/` should be) is treated the same as
/// missing: there is nothing there to hash, and this isn't the
/// symlink/non-regular-file class of problem the walk is guarding against.
pub(super) fn classify_entry(path: &Path) -> Result<EntryKind> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            let file_type = meta.file_type();
            if file_type.is_symlink() {
                anyhow::bail!(
                    "scripts dir contains a symlink ({}); refuse to hash — replace it with a regular file",
                    path.display()
                );
            }
            if file_type.is_dir() {
                return Ok(EntryKind::Dir);
            }
            if file_type.is_file() {
                return Ok(EntryKind::File);
            }
            anyhow::bail!(
                "scripts dir contains a non-regular file ({}); refuse to hash",
                path.display()
            );
        }
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(EntryKind::Missing)
        }
        Err(e) => Err(e).with_context(|| format!("reading metadata for {}", path.display())),
    }
}

/// Recursively collect `(relative-path, bytes)` for every file under `dir`,
/// with `/`-separated relative paths for cross-platform determinism.
pub(super) fn collect_gate_files(dir: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    collect_into(dir, dir, &mut out)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn collect_into(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        match classify_entry(&path)? {
            EntryKind::Dir => collect_into(root, &path, out)?,
            EntryKind::File => {
                let rel = path
                    .strip_prefix(root)
                    .expect("walked path must live under the scripts root")
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                let bytes =
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
                out.push((rel, bytes));
            }
            EntryKind::Missing => {
                // TOCTOU: read_dir just enumerated this entry, so it should
                // exist. If it vanished between listing and stat, fail
                // loudly rather than silently under-hashing the scripts dir.
                anyhow::bail!(
                    "scripts dir entry disappeared mid-walk ({})",
                    path.display()
                );
            }
        }
    }
    Ok(())
}

/// Derive the `.ironlint/scripts` directory beside each policy path. Shared
/// by [`compute_hash`] (which folds these into the
/// hash) and [`blessed_summary`] (which enumerates them for display), so the
/// two can never disagree about which directories are in scope.
pub(super) fn policy_script_dirs(config_paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut script_dirs: Vec<PathBuf> = config_paths
        .iter()
        .map(|p| {
            p.parent()
                .unwrap_or_else(|| Path::new("."))
                .join(".ironlint")
                .join("scripts")
        })
        .collect();
    script_dirs.sort();
    script_dirs.dedup();
    script_dirs
}

/// A trust-verified snapshot of a v1 policy and every managed script folded
/// into its hash.
///
/// The bytes held here are the ones the operator approved. Evaluation must
/// parse the policy from [`ApprovedPolicy::policy_bytes`] and must refuse to
/// run whenever [`ApprovedPolicy::verify_unchanged`] reports drift; re-reading
/// the live policy path after verification is the TOCTOU this type closes.
pub struct ApprovedPolicy {
    config_path: PathBuf,
    policy_bytes: Vec<u8>,
    /// One digest per folded blob, in fold order. Deliberately **not** the blob
    /// bytes: the snapshot lives for the whole run and a fresh live copy is
    /// built before every check, so retaining managed-script content would
    /// double the resident footprint of a large `.ironlint/scripts/` tree for
    /// no verification benefit — drift only ever needs the digest comparison
    /// below.
    digests: Vec<(String, [u8; 32])>,
    pub(super) hash: String,
}

impl ApprovedPolicy {
    /// Verbatim bytes of the primary policy file that were hashed and approved.
    pub fn policy_bytes(&self) -> &[u8] {
        &self.policy_bytes
    }

    /// Bytes of blob content (labels + digests) this snapshot retains. The
    /// guarantee is structural — no script bytes are stored — and this exists
    /// so a future change that reintroduces them is caught by a test.
    #[cfg(test)]
    fn retained_blob_bytes(&self) -> usize {
        self.digests
            .iter()
            .map(|(label, digest)| label.len() + digest.len())
            .sum()
    }

    /// Re-read the policy and every managed script and confirm they still hold
    /// the exact approved bytes. Fails closed with the drifted entries named
    /// so the caller can refuse to execute unapproved content.
    pub fn verify_unchanged(&self) -> Result<()> {
        let live = read_and_hash(&self.config_path)
            .map_err(|e| anyhow::anyhow!("could not verify approved policy is unchanged: {e:#}"))?;
        if live.hash == self.hash {
            return Ok(());
        }
        let mut drifted: Vec<String> = live
            .digests
            .iter()
            .filter(|(label, digest)| {
                self.digests
                    .iter()
                    .find(|(approved_label, _)| approved_label == label)
                    .is_none_or(|(_, approved)| approved != digest)
            })
            .map(|(label, _)| format!("{} (modified or added)", display_label(label)))
            .collect();
        drifted.extend(
            self.digests
                .iter()
                .filter(|(label, _)| {
                    !live
                        .digests
                        .iter()
                        .any(|(live_label, _)| live_label == label)
                })
                .map(|(label, _)| format!("{} (removed)", display_label(label))),
        );
        anyhow::bail!(
            "approved policy or managed scripts changed during evaluation ({}); \
             refusing to execute unapproved bytes — re-run `ironlint trust` to review and re-approve",
            drifted.join(", ")
        )
    }
}

/// Render an internal `kind\0...` hash label as a human-readable path.
fn display_label(label: &str) -> String {
    let mut parts = label.split('\0');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("config"), Some(path), None, None) => format!("policy {path}"),
        (Some("scripts"), Some(dir), Some(rel), None) => format!("{dir}/{rel}"),
        _ => label.replace('\0', " "),
    }
}

/// Digest of one folded blob, retained instead of the blob's bytes.
fn blob_digest(bytes: &[u8]) -> [u8; 32] {
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&Sha256::digest(bytes));
    digest
}

/// Fold `policy_bytes` (standing in for the content of `canonical_config`) and
/// every managed script beside it into `hasher`, retaining one digest per
/// blob. The single fold implementation behind [`read_and_hash`] and
/// [`hash_policy_bytes`], so the path-based and byte-bound entry points can
/// never disagree about labels, order, or framing.
fn fold_policy(
    hasher: &mut Sha256,
    digests: &mut Vec<(String, [u8; 32])>,
    canonical_config: &Path,
    policy_bytes: &[u8],
) -> Result<()> {
    let label = format!("config\0{}", canonical_config.display());
    hash_entry(hasher, &label, policy_bytes);
    digests.push((label, blob_digest(policy_bytes)));

    for scripts_dir in policy_script_dirs(&[canonical_config.to_path_buf()]) {
        match classify_entry(&scripts_dir)? {
            EntryKind::Dir => {
                for (rel, bytes) in collect_gate_files(&scripts_dir)? {
                    let label = format!("scripts\0{}\0{rel}", scripts_dir.display());
                    hash_entry(hasher, &label, &bytes);
                    digests.push((label, blob_digest(&bytes)));
                }
            }
            EntryKind::Missing => {}
            EntryKind::File => {
                anyhow::bail!(
                    "expected {} to be a directory (scripts dir)",
                    scripts_dir.display()
                );
            }
        }
    }
    Ok(())
}

/// Read the policy and every managed script once, fold them into the trust
/// hash, and keep the exact policy bytes the hash covers. Every path-based
/// hash consumer goes through here so the approved bytes can never disagree
/// with the digest.
pub(super) fn read_and_hash(config_path: &Path) -> Result<ApprovedPolicy> {
    let config_paths = config_paths(config_path)?;
    let canonical = config_paths
        .first()
        .cloned()
        .unwrap_or_else(|| config_path.to_path_buf());
    let policy_bytes =
        std::fs::read(&canonical).with_context(|| format!("reading {}", canonical.display()))?;

    let mut hasher = Sha256::new();
    let mut digests: Vec<(String, [u8; 32])> = Vec::new();
    fold_policy(&mut hasher, &mut digests, &canonical, &policy_bytes)?;

    Ok(ApprovedPolicy {
        config_path: canonical,
        policy_bytes,
        digests,
        hash: sha256_digest_hex(&hasher.finalize()),
    })
}

/// Trust-hash `policy_bytes` **as the content of** `config_path`, plus every
/// managed script beside it.
///
/// Byte-bound sibling of [`compute_hash`]: callers that already hold the exact
/// policy bytes — `ironlint init`, which has just classified or written them —
/// hash those bytes instead of re-reading the path, so consent can never cover
/// content the caller did not classify. Framing is shared with the path-based
/// fold, so identical bytes produce an identical digest either way.
pub(super) fn hash_policy_bytes(config_path: &Path, policy_bytes: &[u8]) -> Result<String> {
    crate::config::v1::parse_v1_bytes(policy_bytes)
        .with_context(|| format!("validating v1 policy {}", config_path.display()))?;
    let canonical = config_path
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", config_path.display()))?;

    let mut hasher = Sha256::new();
    let mut digests: Vec<(String, [u8; 32])> = Vec::new();
    fold_policy(&mut hasher, &mut digests, &canonical, policy_bytes)?;
    Ok(sha256_digest_hex(&hasher.finalize()))
}

/// Compute the trust hash of a v1 policy and its managed scripts.
///
/// Every blob is folded with [`hash_entry`]'s
/// length-prefixed framing and a label bound to the blob's identity (its
/// canonical config path, or its scripts dir + relative path), so neither
/// reordering nor relabeling can produce a collision. Path-reading sibling of
/// [`hash_policy_bytes`], which hashes caller-supplied bytes instead.
pub fn compute_hash(config_path: &Path) -> Result<String> {
    Ok(read_and_hash(config_path)?.hash)
}

/// Compute the worktree-relative policy hash for `config_path` under `scope`.
///
/// Reuses the same `.ironlint/scripts/` enumeration, symlink refusal,
/// sorting, and `hash_entry` framing as
/// [`compute_hash`] — the only semantic difference is the labels, which use
/// worktree-root-relative paths so the digest is stable across linked
/// worktrees. Any config or scripts dir that escapes `scope.worktree_root`
/// makes the policy ineligible (`Ok(None)`) rather than silently omitting a
/// file.
pub(super) fn compute_worktree_hash(
    config_path: &Path,
    scope: &WorktreeScope,
) -> Result<Option<String>> {
    let config_paths = config_paths(config_path)?;
    if !all_under_root(&config_paths, &scope.worktree_root) {
        return Ok(None);
    }
    let mut hasher = Sha256::new();
    for path in &config_paths {
        let rel = worktree_rel(path, &scope.worktree_root)?;
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        hash_entry(&mut hasher, &format!("config\0{rel}"), &bytes);
    }
    let script_dirs = policy_script_dirs(&config_paths);
    if !all_under_root(&script_dirs, &scope.worktree_root) {
        return Ok(None);
    }
    for scripts_dir in &script_dirs {
        match classify_entry(scripts_dir)? {
            EntryKind::Dir => {
                let dir_rel = worktree_rel(scripts_dir, &scope.worktree_root)?;
                for (rel, bytes) in collect_gate_files(scripts_dir)? {
                    hash_entry(&mut hasher, &format!("scripts\0{dir_rel}\0{rel}"), &bytes);
                }
            }
            EntryKind::Missing => {}
            EntryKind::File => {
                anyhow::bail!(
                    "expected {} to be a directory (scripts dir)",
                    scripts_dir.display()
                );
            }
        }
    }
    Ok(Some(sha256_digest_hex(&hasher.finalize())))
}

pub(super) fn config_paths(config_path: &Path) -> Result<Vec<PathBuf>> {
    crate::config::v1::parse_v1_file(config_path)
        .with_context(|| format!("validating v1 policy {}", config_path.display()))?;
    Ok(vec![config_path.canonicalize().with_context(|| {
        format!("canonicalizing {}", config_path.display())
    })?])
}

/// True iff every path in `paths` is under `root` (after canonicalization).
fn all_under_root(paths: &[PathBuf], root: &Path) -> bool {
    paths.iter().all(|p| p.strip_prefix(root).is_ok())
}

/// `canon` relative to `root`, `/`-separated. `Err` if not under `root`.
fn worktree_rel(canon: &Path, root: &Path) -> Result<String> {
    let rel = canon.strip_prefix(root).with_context(|| {
        format!(
            "{} escapes worktree root {}",
            canon.display(),
            root.display()
        )
    })?;
    Ok(rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/"))
}

#[cfg(test)]
#[path = "tests/policy_hash.rs"]
mod tests;
