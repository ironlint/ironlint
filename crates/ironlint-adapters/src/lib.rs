//! Versioned installation and ownership for separately shipped harness packages.
#![warn(clippy::cognitive_complexity)]
pub mod adapter;
pub use adapter::*;
#[cfg(not(test))]
use ironlint_core::filesystem;
use ironlint_core::trust;
// Compile the same filesystem source for unit tests to retain its private
// deterministic fault injection without exposing test controls in production.
#[cfg(test)]
#[path = "../../ironlint-core/src/filesystem.rs"]
mod filesystem;
