use anyhow::{Context, Result};
use std::path::Path;

use super::policy_hash::ApprovedPolicy;
use super::store::{
    acquire_store_lock, canonical_key, read_store, read_store_for_bless, trust_store_path,
    write_store, TrustEntry, TrustStore, TRUST_STORE_VERSION,
};
use crate::policy::PolicySnapshot;

/// Outcome of a trust-enforcement attempt against a specific store.
///
/// Kept distinct so callers (concretely: `ironlint-cli`'s `check` command,
/// Task 3.2 / Finding C3) can map them to different exit codes — an
/// untrusted or tampered config must be surfaced loudly (exit 4, and
/// pre-write adapters block), while a config the trust layer can't even
/// *evaluate* is a structural config problem the engine's own `load()` will
/// report a moment later on its own terms, so it keeps the ordinary
/// config-error exit code.
///
/// `ensure_trusted`/`ensure_trusted_in` collapse both non-`Trusted` variants
/// back into a plain `Err` — every existing caller that only cares "is this
/// trusted, yes or no" (`doctor`, the trust_extends integration tests, this
/// module's own unit tests) keeps working unchanged.
pub enum TrustOutcome {
    /// The policy and every managed script matched the store entry. Carries
    /// the exact approved bytes so evaluation never re-reads the live paths.
    Trusted(ApprovedPolicy),
    /// The config's hash resolved fine but doesn't match (or has no) entry
    /// in the store — genuinely untrusted/tampered — OR the trust store
    /// itself couldn't be read (corrupt/unreadable): either way we cannot
    /// answer "is this trusted?" in the affirmative, so fail closed and
    /// surface it loudly.
    Untrusted(anyhow::Error),
    /// The trust hash could not be *computed* at all — the policy doesn't
    /// parse, a referenced path is missing, a
    /// scripts dir contains a symlink, etc. This is a structural config
    /// problem, not a trust decision: defer to the engine's own load error.
    Unverifiable(anyhow::Error),
}

/// Classify a trust-enforcement attempt against `store_path`. See
/// [`TrustOutcome`] for what each variant means and why the split exists.
pub fn check_trust_in(config_path: &Path, store_path: &Path) -> TrustOutcome {
    let snapshot = match PolicySnapshot::load(config_path) {
        Ok(snapshot) => snapshot,
        Err(e) => return TrustOutcome::Unverifiable(e),
    };
    let key = canonical_key(snapshot.config_path());
    let store = match read_store(store_path) {
        Ok(s) => s,
        Err(e) => return TrustOutcome::Untrusted(e),
    };
    // 1. Direct trust (unchanged, first).
    if store
        .entries
        .get(&key)
        .is_some_and(|entry| entry.hash == snapshot.hash())
    {
        return TrustOutcome::Trusted(ApprovedPolicy::new(snapshot));
    }
    // 2. Inherited worktree trust (only after a direct miss).
    if inherited_trusted(&snapshot, &store) {
        return TrustOutcome::Trusted(ApprovedPolicy::new(snapshot));
    }
    TrustOutcome::Untrusted(anyhow::anyhow!(
        "config/scripts not trusted — review and run `ironlint trust`"
    ))
}

/// Step 5: look up `worktree_entries[common_dir][config_rel]` by worktree hash.
fn inherited_trusted(snapshot: &PolicySnapshot, store: &TrustStore) -> bool {
    let Some((scope, hash)) = &snapshot.identity.worktree else {
        return false;
    };
    store
        .worktree_entries
        .get(scope.common_dir.to_string_lossy().as_ref())
        .and_then(|m| m.get(&scope.config_rel))
        .is_some_and(|e| e.hash == *hash)
}

/// Verify `config_path` (and its scripts) match a blessed entry in the
/// store at `store_path`. Fails closed with a fixed, actionable message.
///
/// Thin boolean wrapper over [`check_trust_in`] for callers that only need
/// "is this trusted" — see that function if you need to distinguish
/// untrusted from unverifiable (e.g. to pick an exit code).
pub fn ensure_trusted_in(config_path: &Path, store_path: &Path) -> Result<()> {
    match check_trust_in(config_path, store_path) {
        TrustOutcome::Trusted(_) => Ok(()),
        TrustOutcome::Untrusted(e) | TrustOutcome::Unverifiable(e) => Err(e),
    }
}

/// Recompute the hash of `config_path` and write it to the store as blessed.
///
/// Validates the v1 policy before blessing. The read-modify-write against
/// `store_path` is guarded by an exclusive
/// [`acquire_store_lock`], so concurrent `ironlint trust` runs (e.g. parallel
/// agent sessions) serialize instead of racing and losing entries; a corrupt
/// existing store is tolerated (see [`read_store_for_bless`]) so blessing
/// doubles as the recovery path.
pub fn bless_in(config_path: &Path, store_path: &Path, now: &str) -> Result<()> {
    let snapshot = PolicySnapshot::load(config_path)
        .context("refusing to trust a config that does not parse as v1")?;
    write_blessed_entry(&snapshot, store_path, now)
}

/// Recompute the hash of `policy_bytes` **as the content of** `config_path`
/// and write it to the store as blessed.
///
/// Byte-bound sibling of [`bless_in`]: consent covers exactly the bytes the
/// caller classified, never a second read of the path. A writer that rewrites
/// `config_path` between the caller's classification and this call therefore
/// cannot get its bytes approved — the subsequent [`check_trust_in`] hashes the
/// live file and reports it untrusted.
pub fn bless_bytes_in(
    config_path: &Path,
    policy_bytes: &[u8],
    store_path: &Path,
    now: &str,
) -> Result<()> {
    let snapshot = PolicySnapshot::from_bytes(config_path, policy_bytes)
        .context("refusing to trust a config that does not parse as v1")?;
    write_blessed_entry(&snapshot, store_path, now)
}

/// Store the captured direct hash under its canonical key plus the eligible
/// worktree entry. Single write path shared by both bless entry points so the
/// store contract cannot diverge between them.
fn write_blessed_entry(snapshot: &PolicySnapshot, store_path: &Path, now: &str) -> Result<()> {
    let key = canonical_key(snapshot.config_path());

    let _lock = acquire_store_lock(store_path)?;
    let mut store = read_store_for_bless(store_path)?;
    store.version = TRUST_STORE_VERSION;
    store.entries.insert(
        key,
        TrustEntry {
            hash: snapshot.hash().to_owned(),
            blessed_at: now.to_string(),
        },
    );
    write_worktree_entry(&mut store, snapshot, now);
    write_store(store_path, &store)
}

/// Record the eligible worktree hash captured with the direct hash. Discovery
/// remains best-effort; policy and scripts are never re-read while writing.
fn write_worktree_entry(store: &mut TrustStore, snapshot: &PolicySnapshot, now: &str) {
    let Some((scope, hash)) = &snapshot.identity.worktree else {
        return;
    };
    let common = scope.common_dir.to_string_lossy().to_string();
    store.worktree_entries.entry(common).or_default().insert(
        scope.config_rel.clone(),
        TrustEntry {
            hash: hash.clone(),
            blessed_at: now.to_string(),
        },
    );
}

/// Thin wrapper: enforce trust against the real out-of-repo store.
pub fn ensure_trusted(config_path: &Path) -> Result<()> {
    ensure_trusted_in(config_path, &trust_store_path()?)
}

/// Thin wrapper: classify trust against the real out-of-repo store.
///
/// See [`check_trust_in`]/[`TrustOutcome`]. The outer `Result` only ever
/// fails when the store's location itself can't be resolved (no
/// `$XDG_CONFIG_HOME` / `$HOME`) — an environment problem, not a per-config
/// trust decision.
pub fn check_trust(config_path: &Path) -> Result<TrustOutcome> {
    Ok(check_trust_in(config_path, &trust_store_path()?))
}

/// Thin wrapper: bless against the real out-of-repo store, stamping `blessed_at`
/// with the current UTC time.
pub fn bless(config_path: &Path) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    bless_in(config_path, &trust_store_path()?, &now)
}

/// Thin wrapper: byte-bound bless against the real out-of-repo store,
/// stamping `blessed_at` with the current UTC time. See [`bless_bytes_in`].
pub fn bless_bytes(config_path: &Path, policy_bytes: &[u8]) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    bless_bytes_in(config_path, policy_bytes, &trust_store_path()?, &now)
}

#[cfg(test)]
#[path = "tests/decision.rs"]
mod tests;
