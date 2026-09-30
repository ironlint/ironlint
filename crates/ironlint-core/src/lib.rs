//! IronLint core library: config, engines, verdict, trust.

#![warn(clippy::cognitive_complexity)]

pub mod adapter;
pub mod config;
pub mod engine;
#[doc(hidden)]
pub mod filesystem;
pub mod runner;
pub mod trust;
pub mod verdict;
