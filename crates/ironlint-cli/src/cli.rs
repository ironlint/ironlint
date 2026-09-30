use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "ironlint",
    version,
    about = "Policy-enforcement pipeline for AI coding agents"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Evaluate the v1 policy. Acceptance runs every check; change may filter by file.
    Check {
        #[arg(long = "file", action = clap::ArgAction::Append)]
        file: Vec<PathBuf>,
        #[arg(long, default_value = "human")]
        format: OutputFormat,
        #[arg(long, default_value = ".ironlint.yml")]
        config: PathBuf,
        /// Evaluation event. Defaults to acceptance.
        #[arg(
            long,
            value_parser = clap::builder::PossibleValuesParser::new(["change", "accept"])
        )]
        event: Option<String>,
        /// Root of the tree evaluated by a v1 policy. Relative --file paths
        /// resolve under this directory.
        #[arg(long)]
        root: Option<PathBuf>,
        /// Internal adapter capability: stop checks when the parent closes stdin.
        #[arg(long, hide = true)]
        cancel_on_stdin_close: bool,
    },
    /// Bless this config + its `.ironlint/scripts/` scripts in the out-of-repo trust store.
    Trust {
        #[arg(long, default_value = ".ironlint.yml")]
        config: PathBuf,
    },
    /// Parse and validate the config without running any gates.
    Validate {
        #[arg(long, default_value = ".ironlint.yml")]
        config: PathBuf,
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Detect stack and scaffold a starter .ironlint.yml, then wire ironlint's hook
    /// into your coding agents.
    Init {
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        /// Harness(es) to wire up (repeatable); `all` selects every supported
        /// harness. Omit to auto-detect and confirm.
        #[arg(long = "harness", value_name = "NAME")]
        harnesses: Vec<String>,
        /// Patch user-level settings instead of project-local.
        #[arg(long)]
        global: bool,
        /// Skip the install confirmation prompt.
        #[arg(long)]
        yes: bool,
        /// Scaffold the config but install no hooks (legacy behavior).
        #[arg(long)]
        no_hook: bool,
        /// Skip config scaffolding; only wire hooks.
        #[arg(long)]
        hook_only: bool,
        /// Remove ironlint hooks and materialized artifacts.
        #[arg(long)]
        uninstall: bool,
        /// Install/remove the optional local git pre-commit hook.
        #[arg(long)]
        git_hook: bool,
        /// Print intended changes without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Diagnose the local install, config, and adapter wiring.
    ///
    /// Read-only. Exits 0 if every check passes or only warns; exits 1 on any failure.
    Doctor {
        /// Directory containing `.ironlint.yml`. Defaults to cwd.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        /// Output format. `human` (default) prints a checklist; `json` prints a
        /// machine-readable report — see `docs/operating/diagnostics.md` for the schema.
        #[arg(long, default_value = "human")]
        format: OutputFormat,
    },
    /// Show which checks apply to `<file>` and their run commands.
    ///
    /// Read-only — no check runs.
    Explain {
        /// Path to inspect. Relative to cwd.
        file: PathBuf,
        #[arg(long, default_value = "human")]
        format: OutputFormat,
        #[arg(long, default_value = ".ironlint.yml")]
        config: PathBuf,
        /// Root of the tree evaluated by a v1 policy. Relative file paths
        /// resolve under this directory. Defaults to the current directory.
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Print the configured check set.
    ///
    /// Read-only. Does not run any check. Default format prints each check
    /// with its files glob(s) and run command, annotated by origin.
    ShowResolvedConfig {
        #[arg(long, default_value = ".ironlint.yml")]
        config: PathBuf,
        #[arg(long, default_value = "tsv")]
        format: ShowFormat,
    },
    /// Print the canonical check-authoring guide (the `.ironlint.yml` schema and
    /// patterns). Read-only.
    Schema,
    /// Update ironlint to the latest release.
    ///
    /// Re-runs the dist installer (`ironlint-cli-installer.sh` on Unix,
    /// `.ps1` on Windows) in place. The installer is idempotent, so this also
    /// covers the already-current case (it re-runs but exits 0). Exit codes:
    /// `0` on a successful update; `1` on failure — including when this build
    /// wasn't installed by the ironlint installer (Homebrew/cargo/source
    /// builds) and so can't self-update, in which case it prints the
    /// channel-specific command that will.
    Update,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ShowFormat {
    Tsv,
    Yaml,
    Json,
}
