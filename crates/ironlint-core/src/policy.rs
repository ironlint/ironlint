//! Immutable policy captures for evaluation and inspection, independent of
//! the CLI's local execution consent.

use crate::config::{parse_v1_bytes, V1Config};
use crate::trust::policy_hash::{capture_identity, read_policy, PolicyIdentity};
use anyhow::{Context, Result};
use std::path::Path;

#[derive(Debug)]
pub struct PolicySnapshot {
    pub(crate) identity: PolicyIdentity,
    policy: V1Config,
}

impl PolicySnapshot {
    /// Capture and validate the exact bytes covered by both hash identities.
    /// This performs no consent lookup and writes no files.
    pub fn load(path: &Path) -> Result<Self> {
        let (canonical, bytes) = read_policy(path)?;
        Self::from_captured(canonical, bytes)
    }

    pub(crate) fn from_bytes(path: &Path, bytes: &[u8]) -> Result<Self> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("canonicalizing {}", path.display()))?;
        Self::from_captured(canonical, bytes.to_vec())
    }

    fn from_captured(canonical: std::path::PathBuf, bytes: Vec<u8>) -> Result<Self> {
        let policy = parse_v1_bytes(&bytes)
            .with_context(|| format!("validating v1 policy {}", canonical.display()))?;
        let identity = capture_identity(canonical, bytes)?;
        Ok(Self { identity, policy })
    }

    pub fn policy(&self) -> &V1Config {
        &self.policy
    }
    pub fn config_path(&self) -> &Path {
        &self.identity.config_path
    }
    pub fn policy_bytes(&self) -> &[u8] {
        &self.identity.policy_bytes
    }
    pub fn hash(&self) -> &str {
        &self.identity.hash
    }
    pub fn verify_unchanged(&self) -> Result<()> {
        self.identity.verify_unchanged()
    }

    /// Cooperatively verify within the evaluator's unchanged total deadline.
    pub fn verify_unchanged_until(&self, deadline: Option<std::time::Instant>) -> Result<()> {
        self.identity.verify_unchanged_until(deadline)
    }

    #[cfg(test)]
    pub(crate) fn retained_blob_bytes(&self) -> usize {
        self.identity
            .digests
            .iter()
            .map(|(label, digest)| label.len() + digest.len())
            .sum()
    }
}
