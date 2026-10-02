//! Shared SHA-256 framing for policy and installed-artifact identity.
use sha2::{Digest, Sha256};

/// Format a finalized 32-byte SHA-256 digest as `"sha256:<lowercase-hex>"`.
///
/// digest 0.11's output type (`hybrid_array::Array`) no longer impls
/// `LowerHex`, so callers hex-encode the bytes through here instead of `{:x}`.
/// Shared by one-shot [`sha256_hex`] and the streaming trust hasher.
pub fn sha256_digest_hex(digest: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity("sha256:".len() + digest.len() * 2);
    out.push_str("sha256:");
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// `"sha256:<lowercase-hex>"` of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    sha256_digest_hex(&h.finalize())
}
