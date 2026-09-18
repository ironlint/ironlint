//! `ironlint init` — scaffold a starter `.ironlint.yml`.
//!
//! Emits a universal, stack-agnostic baseline regardless of what toolchain
//! manifests exist in the project root. ironlint knows nothing about any
//! tool — checks own their own messages and report violations by exiting
//! nonzero. Harness onboarding (wiring ironlint's hook into claude-code,
//! codex, pi, opencode) is a separate phase handled by `onboard.rs`.

mod git_hook;
mod onboard;
mod render;
mod select;

use anyhow::{anyhow, Context, Result};
use std::path::Path;

// Seven bools are required by the CLI surface (harness wiring flags + the
// opt-in git hook); the struct_excessive_bools lint would force a
// state-machine refactor that obscures direct flag mapping.
#[allow(clippy::struct_excessive_bools)]
pub struct Options {
    pub harnesses: Vec<String>,
    pub global: bool,
    pub yes: bool,
    pub no_hook: bool,
    pub hook_only: bool,
    pub uninstall: bool,
    pub dry_run: bool,
    /// Install the optional git pre-commit hook, or remove it during uninstall.
    pub git_hook: bool,
}

/// The universal, stack-agnostic v1 starter config.
const BASELINE: &str = r#"version: 1

checks:
  no-fixme:
    run: "! grep -RInE 'FIXME' . --exclude-dir=.git --exclude=.ironlint.yml"
  no-merge-markers:
    run: "! grep -RInE '^(<<<<<<< |=======$|>>>>>>> )' . --exclude-dir=.git"

# --- examples (uncomment and adapt) ---
#
# Run a fast check after matching changes and again for acceptance:
#
#   format:
#     files: ["**/*.rs"]
#     on: [change, accept]
#     run: "cargo fmt --all --check"
"#;

pub fn run(dir: &Path, opts: &Options) -> Result<i32> {
    if opts.no_hook && opts.hook_only {
        return Err(anyhow!("--no-hook and --hook-only are mutually exclusive"));
    }

    if !opts.hook_only && !opts.uninstall {
        if opts.dry_run {
            // `scaffold_config` writes the config AND calls `trust::bless`, so it
            // must be skipped entirely on the dry-run path — a dry-run that
            // mutates the security-critical trust store is disqualifying. The
            // preview must also mirror `scaffold_config`'s classification, or
            // it lies about what a real run would do.
            let cfg_path = dir.join(".ironlint.yml");
            match classify_existing(&cfg_path)? {
                ExistingConfig::Missing => {
                    println!("would scaffold and trust: {}", cfg_path.display());
                }
                ExistingConfig::UnmodifiedBaseline(_) => println!(
                    "{} is the unmodified baseline (would record consent)",
                    cfg_path.display()
                ),
                ExistingConfig::Other => println!(
                    "config: {} already present (would skip)",
                    cfg_path.display()
                ),
            }
        } else {
            scaffold_config(dir, &ironlint_core::trust::bless_bytes)?;
        }
    }

    if opts.no_hook {
        return Ok(0);
    }

    let env = ironlint_core::adapter::AdapterEnv::from_process(dir.to_path_buf())?;
    onboard::run_hook_phase(&env, opts)
}

/// What `init` found at the config path.
#[derive(Debug, PartialEq, Eq)]
enum ExistingConfig {
    /// No config: scaffold and bless the baseline.
    Missing,
    /// The exact baseline bytes `init` writes, carried along so consent can be
    /// recorded for *those* bytes. Re-recording consent for them is the
    /// recovery path after a previous run wrote the file but failed to store
    /// consent; the file itself is never rewritten.
    UnmodifiedBaseline(Vec<u8>),
    /// Anything else is user-owned: never modified, never blessed.
    Other,
}

fn classify_existing(cfg_path: &Path) -> Result<ExistingConfig> {
    match std::fs::read(cfg_path) {
        Ok(bytes) if bytes == BASELINE.as_bytes() => Ok(ExistingConfig::UnmodifiedBaseline(bytes)),
        Ok(_) => Ok(ExistingConfig::Other),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ExistingConfig::Missing),
        Err(e) => Err(e).with_context(|| format!("reading {}", cfg_path.display())),
    }
}

/// Scaffold + trust `.ironlint.yml`. An existing config is never modified: the
/// exact unmodified baseline is (re)blessed — that is the retry path when a
/// previous run wrote the baseline but consent storage failed — and anything
/// else is skipped untouched.
///
/// `bless` receives the classified (or just-written) bytes alongside the path
/// and must derive consent from *those* bytes; re-reading the path instead
/// would let a concurrent writer get its content blessed (see
/// `ironlint_core::trust::bless_bytes`).
fn scaffold_config(dir: &Path, bless: &dyn Fn(&Path, &[u8]) -> Result<()>) -> Result<()> {
    let cfg_path = dir.join(".ironlint.yml");
    match classify_existing(&cfg_path)? {
        ExistingConfig::Missing => {
            std::fs::write(&cfg_path, BASELINE)?;
            bless(&cfg_path, BASELINE.as_bytes()).map_err(|e| {
                anyhow!(
                    "scaffolded {} but could not trust it: {e:#}",
                    cfg_path.display()
                )
            })?;
            println!("scaffolded and trusted: {}", cfg_path.display());
        }
        ExistingConfig::UnmodifiedBaseline(bytes) => {
            bless(&cfg_path, &bytes).map_err(|e| {
                anyhow!(
                    "{} holds the unmodified scaffolded baseline but consent could not be recorded: {e:#}",
                    cfg_path.display()
                )
            })?;
            println!(
                "config: {} already present (unmodified baseline, trusted)",
                cfg_path.display()
            );
        }
        ExistingConfig::Other => {
            println!("config: {} already present (skipped)", cfg_path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(dry_run: bool, no_hook: bool) -> Options {
        Options {
            harnesses: vec![],
            global: false,
            yes: false,
            no_hook,
            hook_only: false,
            uninstall: false,
            dry_run,
            git_hook: false,
        }
    }

    #[test]
    fn baseline_is_v1_and_has_universal_checks() {
        assert!(BASELINE.starts_with("version: 1\n"));
        assert!(BASELINE.contains("no-fixme:"));
        assert!(BASELINE.contains("no-merge-markers:"));
        assert!(BASELINE.contains("on: [change, accept]"));
        assert!(!BASELINE.contains("$IRONLINT_TMPFILE"));
        for tool in ["biome", "eslint", "ruff"] {
            assert!(!BASELINE.contains(tool));
        }
    }

    #[test]
    fn run_rejects_no_hook_and_hook_only_together() {
        let tmp = tempfile::tempdir().unwrap();
        let opts = Options {
            harnesses: vec![],
            global: false,
            yes: false,
            no_hook: true,
            hook_only: true,
            uninstall: false,
            dry_run: false,
            git_hook: true,
        };
        let err = run(tmp.path(), &opts).unwrap_err();
        assert!(err.to_string().contains("mutually exclusive"));
    }

    #[test]
    fn run_with_existing_config_and_no_hook_is_ok_not_error() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(".ironlint.yml"), "checks: {}\n").unwrap();
        let opts = Options {
            harnesses: vec![],
            global: false,
            yes: false,
            no_hook: true,
            hook_only: false,
            uninstall: false,
            dry_run: false,
            git_hook: true,
        };
        let code = run(tmp.path(), &opts).unwrap();
        assert_eq!(code, 0); // previously this path returned Err
    }

    /// Release-blocker regression: baseline written, consent storage failed, so
    /// a retry must record consent for the untouched baseline instead of
    /// skipping it.
    #[test]
    fn scaffold_retry_after_consent_failure_records_consent() {
        let tmp = tempfile::tempdir().unwrap();
        let failing = |_: &Path, _: &[u8]| Err(anyhow!("trust store unavailable"));
        let error = scaffold_config(tmp.path(), &failing).unwrap_err();
        assert!(error.to_string().contains("could not trust it"), "{error}");
        let cfg = tmp.path().join(".ironlint.yml");
        let written = std::fs::read(&cfg).unwrap();
        assert_eq!(written, BASELINE.as_bytes());

        let blessed = std::cell::Cell::new(false);
        let succeeding = |_: &Path, _: &[u8]| -> Result<()> {
            blessed.set(true);
            Ok(())
        };
        scaffold_config(tmp.path(), &succeeding).unwrap();
        assert!(
            blessed.get(),
            "retry must record consent for the untouched baseline"
        );
        assert_eq!(
            std::fs::read(&cfg).unwrap(),
            written,
            "retry must not rewrite the config"
        );
    }

    /// Consent must bind to the bytes `init` classified, never to a second
    /// read of the config path: a writer landing between the classification
    /// and the blessing must not get its bytes approved.
    #[test]
    fn retry_blesses_the_classified_bytes_not_a_later_rewrite() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = tmp.path().join(".ironlint.yml");
        std::fs::write(&cfg, BASELINE).unwrap();
        let payload = "version: 1\nchecks:\n  pwn: {run: 'echo pwned'}\n";

        let recorded = std::cell::RefCell::new(Vec::new());
        // The writer stands in for an editor save, a `git checkout` in another
        // terminal, or any repo process rewriting the path while `init` runs.
        let racing = |path: &Path, bytes: &[u8]| -> Result<()> {
            std::fs::write(path, payload)?;
            *recorded.borrow_mut() = bytes.to_vec();
            Ok(())
        };
        scaffold_config(tmp.path(), &racing).unwrap();

        assert_eq!(
            std::fs::read_to_string(&cfg).unwrap(),
            payload,
            "the racing write must have landed on disk"
        );
        assert_eq!(
            recorded.borrow().as_slice(),
            BASELINE.as_bytes(),
            "consent must cover the bytes init classified, not a later rewrite of the path"
        );
    }

    /// A user-edited or pre-existing config is never blessed or modified.
    #[test]
    fn scaffold_never_touches_a_user_config() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = tmp.path().join(".ironlint.yml");
        std::fs::write(&cfg, "version: 1\nchecks:\n  mine: {run: 'exit 0'}\n").unwrap();
        let before = std::fs::read(&cfg).unwrap();
        let must_not_bless = |_: &Path, _: &[u8]| -> Result<()> {
            panic!("a user config must never be blessed");
        };
        scaffold_config(tmp.path(), &must_not_bless).unwrap();
        assert_eq!(std::fs::read(&cfg).unwrap(), before);
    }

    /// A dry run classifies and reports but writes nothing and never blesses.
    #[test]
    fn dry_run_classifies_without_writing_or_blessing() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = tmp.path().join(".ironlint.yml");
        let dry = opts(true, true);

        assert_eq!(run(tmp.path(), &dry).unwrap(), 0);
        assert!(!cfg.exists(), "dry run must not scaffold");

        std::fs::write(&cfg, BASELINE).unwrap();
        assert_eq!(run(tmp.path(), &dry).unwrap(), 0);
        assert_eq!(std::fs::read(&cfg).unwrap(), BASELINE.as_bytes());

        let user = "version: 1\nchecks:\n  mine: {run: 'exit 0'}\n";
        std::fs::write(&cfg, user).unwrap();
        assert_eq!(run(tmp.path(), &dry).unwrap(), 0);
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), user);
    }
}
