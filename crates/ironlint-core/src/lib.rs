//! IronLint core library: config, engines, verdict, trust.

#![warn(clippy::cognitive_complexity)]

pub mod config;
mod deadline;
pub mod engine;
#[doc(hidden)]
pub mod filesystem;
pub mod hash;
pub mod runner;
pub mod trust;
pub mod verdict;

pub mod policy;
