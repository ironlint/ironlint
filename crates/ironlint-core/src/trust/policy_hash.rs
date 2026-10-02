use super::script_files::{self, ScriptFile};
use crate::deadline;
use crate::hash::sha256_digest_hex;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::worktree::WorktreeScope;

/// Feed one labeled blob into the hasher with length prefixes on both the
/// label and the content, so no two distinct (label, bytes) pairs can collide
/// by concatenation.
fn hash_entry(hasher: &mut Sha256, label: &str, bytes: &[u8]) {
    hash_prefix(hasher, label, bytes.len() as u64);
    hasher.update(bytes);
}

fn hash_prefix(hasher: &mut Sha256, label: &str, len: u64) {
    hasher.update((label.len() as u64).to_le_bytes());
    hasher.update(label.as_bytes());
    hasher.update(len.to_le_bytes());
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

/// Derive the `.ironlint/scripts` directory beside each policy path. Shared
/// by [`compute_hash`] (which folds these into the
/// hash) and [`super::blessed_summary`] (which enumerates them for display), so the
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

/// Captured raw bytes and identity. Consent is deliberately absent here.
/// Script content is discarded after folding; only labels and digests remain.
#[derive(Debug)]
pub(crate) struct PolicyIdentity {
    pub(crate) config_path: PathBuf,
    pub(crate) policy_bytes: Vec<u8>,
    pub(crate) digests: Vec<(String, [u8; 32])>,
    pub(crate) hash: String,
    pub(crate) worktree: Option<(WorktreeScope, String)>,
}
impl PolicyIdentity {
    pub(crate) fn verify_unchanged(&self) -> Result<()> {
        self.verify_unchanged_until(None)
    }

    pub(crate) fn verify_unchanged_until(&self, until: Option<Instant>) -> Result<()> {
        deadline::check(until)?;
        // Verification needs the direct identity, not another Git scope lookup.
        let bytes = script_files::read_policy(&self.config_path, until)
            .context("could not verify approved policy is unchanged")?;
        let live = fold_policy(self.config_path.clone(), bytes, None, until)
            .context("could not verify approved policy is unchanged")?;
        if live.hash == self.hash {
            return Ok(());
        }
        if live.policy_bytes != self.policy_bytes {
            crate::config::parse_v1_bytes(&live.policy_bytes)
                .context("could not verify approved policy is unchanged")?;
        }
        let drifted = drifted_entries(&self.digests, &live.digests);
        anyhow::bail!(
            "approved policy or managed scripts changed during evaluation ({}); \
             refusing to execute unapproved bytes — re-run `ironlint trust` to review and re-approve",
            drifted.join(", ")
        )
    }
}

fn drifted_entries(approved: &[(String, [u8; 32])], live: &[(String, [u8; 32])]) -> Vec<String> {
    let mut drifted = Vec::new();
    let mut old = approved.iter().peekable();
    let mut new = live.iter().peekable();
    while old.peek().is_some() || new.peek().is_some() {
        match (old.peek(), new.peek()) {
            (Some(a), Some(b)) if a.0 == b.0 => {
                if a.1 != b.1 {
                    drifted.push(format!("{} (modified or added)", display_label(&b.0)));
                }
                old.next();
                new.next();
            }
            (Some(a), Some(b)) if a.0 < b.0 => {
                drifted.push(format!("{} (removed)", display_label(&a.0)));
                old.next();
            }
            (_, Some(b)) => {
                drifted.push(format!("{} (modified or added)", display_label(&b.0)));
                new.next();
            }
            (Some(a), None) => {
                drifted.push(format!("{} (removed)", display_label(&a.0)));
                old.next();
            }
            (None, None) => break,
        }
    }
    drifted
}

/// CLI consent wrapper, minted only by a successful trust decision. Library
/// users can evaluate the neutral snapshot without constructing consent.
#[derive(Debug)]
pub struct ApprovedPolicy(Box<crate::policy::PolicySnapshot>);
impl ApprovedPolicy {
    pub(crate) fn new(snapshot: crate::policy::PolicySnapshot) -> Self {
        Self(Box::new(snapshot))
    }
    pub fn snapshot(&self) -> &crate::policy::PolicySnapshot {
        &self.0
    }
    pub fn policy_bytes(&self) -> &[u8] {
        self.0.policy_bytes()
    }
    pub fn verify_unchanged(&self) -> Result<()> {
        self.0.verify_unchanged()
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

/// Fold the direct and eligible worktree identities from the same blobs.
struct PolicyFold {
    direct: Sha256,
    relative: Option<Sha256>,
    digests: Vec<(String, [u8; 32])>,
}
impl PolicyFold {
    fn add(&mut self, label: String, relative_label: Option<String>, bytes: &[u8]) {
        hash_entry(&mut self.direct, &label, bytes);
        if let (Some(hasher), Some(label)) = (&mut self.relative, relative_label) {
            hash_entry(hasher, &label, bytes);
        }
        self.digests.push((label, blob_digest(bytes)));
    }

    fn add_file(
        &mut self,
        label: String,
        relative_label: Option<String>,
        file: &ScriptFile,
        until: Option<Instant>,
    ) -> Result<()> {
        hash_prefix(&mut self.direct, &label, file.len());
        let mut relative = self.relative.as_mut().zip(relative_label);
        if let Some((hasher, label)) = &mut relative {
            hash_prefix(hasher, label, file.len());
        }
        let digest = script_files::stream(file, until, |chunk| {
            self.direct.update(chunk);
            if let Some((hasher, _)) = &mut relative {
                hasher.update(chunk);
            }
        })?;
        self.digests.push((label, digest));
        Ok(())
    }
}

fn fold_policy(
    canonical: PathBuf,
    policy_bytes: Vec<u8>,
    scope: Option<WorktreeScope>,
    until: Option<Instant>,
) -> Result<PolicyIdentity> {
    deadline::check(until)?;
    let script_dirs = policy_script_dirs(std::slice::from_ref(&canonical));
    let scope = scope.filter(|scope| {
        canonical.starts_with(&scope.worktree_root)
            && script_dirs
                .iter()
                .all(|dir| dir.starts_with(&scope.worktree_root))
    });
    let mut fold = PolicyFold {
        direct: Sha256::new(),
        relative: scope.as_ref().map(|_| Sha256::new()),
        digests: Vec::new(),
    };
    let relative_label = scope
        .as_ref()
        .map(|s| worktree_rel(&canonical, &s.worktree_root).map(|rel| format!("config\0{rel}")))
        .transpose()?;
    fold.add(
        format!("config\0{}", canonical.display()),
        relative_label,
        &policy_bytes,
    );
    for dir in script_dirs {
        fold_scripts(&mut fold, &dir, scope.as_ref(), until)?;
    }
    let worktree = scope
        .zip(fold.relative)
        .map(|(scope, hasher)| (scope, sha256_digest_hex(&hasher.finalize())));
    Ok(PolicyIdentity {
        config_path: canonical,
        policy_bytes,
        digests: fold.digests,
        hash: sha256_digest_hex(&fold.direct.finalize()),
        worktree,
    })
}

fn fold_scripts(
    fold: &mut PolicyFold,
    dir: &Path,
    scope: Option<&WorktreeScope>,
    until: Option<Instant>,
) -> Result<()> {
    deadline::check(until)?;
    match classify_entry(dir)? {
        EntryKind::Dir => {
            let dir_rel = scope
                .map(|s| worktree_rel(dir, &s.worktree_root))
                .transpose()?;
            let files = script_files::collect(dir, until)?;
            for file in &files {
                fold.add_file(
                    format!("scripts\0{}\0{}", dir.display(), file.rel),
                    dir_rel
                        .as_ref()
                        .map(|dir| format!("scripts\0{dir}\0{}", file.rel)),
                    file,
                    until,
                )?;
            }
            if files != script_files::collect(dir, until)? {
                anyhow::bail!("scripts changed during verification ({})", dir.display());
            }
        }
        EntryKind::Missing => {}
        EntryKind::File => {
            anyhow::bail!("expected {} to be a directory (scripts dir)", dir.display())
        }
    }
    Ok(())
}

pub(crate) fn read_policy(path: &Path) -> Result<(PathBuf, Vec<u8>)> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", path.display()))?;
    let bytes =
        std::fs::read(&canonical).with_context(|| format!("reading {}", canonical.display()))?;
    Ok((canonical, bytes))
}

pub(crate) fn capture_identity(canonical: PathBuf, bytes: Vec<u8>) -> Result<PolicyIdentity> {
    let scope = WorktreeScope::discover(&canonical);
    fold_policy(canonical, bytes, scope, None)
}

#[cfg(test)]
fn read_and_hash(config_path: &Path) -> Result<crate::policy::PolicySnapshot> {
    crate::policy::PolicySnapshot::load(config_path)
}

#[cfg(test)]
fn hash_policy_bytes(path: &Path, bytes: &[u8]) -> Result<String> {
    Ok(crate::policy::PolicySnapshot::from_bytes(path, bytes)?
        .hash()
        .to_owned())
}

/// Compute the direct hash, validating the exact policy bytes that are folded.
pub fn compute_hash(path: &Path) -> Result<String> {
    Ok(crate::policy::PolicySnapshot::load(path)?.hash().to_owned())
}

#[cfg(test)]
pub(super) fn compute_worktree_hash(path: &Path, scope: &WorktreeScope) -> Result<Option<String>> {
    let canonical = path.canonicalize()?;
    let bytes = std::fs::read(&canonical)?;
    crate::config::parse_v1_bytes(&bytes)?;
    let identity = fold_policy(canonical, bytes, Some(scope.clone()), None)?;
    Ok(identity.worktree.map(|(_, hash)| hash))
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
