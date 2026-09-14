#![warn(clippy::cognitive_complexity)]

mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use std::ffi::OsStr;
use std::path::PathBuf;

fn main() -> Result<()> {
    let v1_usage_event = explicit_v1_json_event();
    // Parse with the fallible API so a *usage* error (typo'd flag, missing
    // value, bare invocation) can be remapped to exit 1 (config/usage tier)
    // instead of clap's default 2. Exit 2 is reserved for a real **Block**
    // verdict; adapters map exit 2 to a policy block and show its stdout as
    // the reason, so a typo must never look like a block. `e.use_stderr()` is
    // false for `--help`/`--version` (which should still exit 0) and true for
    // genuine parse errors — so help stays exit 0 while errors become exit 1.
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            if e.use_stderr() {
                if let Some(event) = v1_usage_event {
                    let code = commands::check::emit_v1_error(
                        cli::OutputFormat::Json,
                        event,
                        &e.to_string(),
                        1,
                    );
                    std::process::exit(code);
                }
            }
            e.print().expect("write clap output to stdout/stderr");
            std::process::exit(i32::from(e.use_stderr()));
        }
    };
    let code = match cli.command {
        Command::Check {
            file,
            diff,
            content,
            format,
            config,
            checks,
            event,
            root,
            explain,
            allow_external_paths,
            force,
            require_match,
        } => commands::check::run(
            file,
            diff,
            content,
            format,
            &config,
            checks,
            event,
            explain,
            allow_external_paths,
            force,
            require_match,
            root.as_deref(),
        )?,
        Command::Trust { config } => commands::trust::run(&config)?,
        Command::Validate { config, format } => commands::validate::run(&config, format)?,
        Command::Init {
            dir,
            harnesses,
            global,
            yes,
            no_hook,
            hook_only,
            uninstall,
            no_git_hook,
            dry_run,
        } => commands::init::run(
            &dir,
            &commands::init::Options {
                harnesses,
                global,
                yes,
                no_hook,
                hook_only,
                uninstall,
                dry_run,
                // `--no-hooks` (legacy scaffold-only) implies no git floor.
                git_hook: !no_git_hook && !no_hook,
            },
        )?,
        Command::Doctor { dir, format } => commands::doctor::run(&dir, format)?,
        Command::Explain {
            file,
            format,
            config,
            root,
        } => commands::explain::run(&file, format, &config, root.as_deref())?,
        Command::ShowResolvedConfig { config, format } => {
            commands::show_resolved_config::run(&config, format)?
        }
        Command::Schema => commands::schema::run()?,
        Command::Update => commands::update::run()?,
        Command::Watch { dir } => commands::watch::run(&dir)?,
        Command::GateBash => commands::gate_bash::run()?,
    };
    std::process::exit(code);
}

fn explicit_v1_json_event() -> Option<&'static str> {
    let mut args = std::env::args_os().skip(1).peekable();
    if args.next().as_deref() != Some(OsStr::new("check")) {
        return None;
    }

    let mut event = V1UsageEvent::Absent;
    let mut json = false;
    let mut config = None;
    let mut config_explicit = false;
    while let Some(arg) = args.next() {
        if arg == OsStr::new("--") {
            break;
        }
        if arg == OsStr::new("--event") {
            let value = args.next_if(|value| !value.to_string_lossy().starts_with('-'));
            record_v1_event(&mut event, value.as_deref());
            continue;
        }
        if arg == OsStr::new("--config") {
            config_explicit = true;
            config = config.or_else(|| {
                args.next_if(|value| !value.to_string_lossy().starts_with('-'))
                    .map(PathBuf::from)
            });
            continue;
        }
        if arg == OsStr::new("--format") {
            json |= args
                .next_if(|value| !value.to_string_lossy().starts_with('-'))
                .as_deref()
                == Some(OsStr::new("json"));
            continue;
        }
        if let Some(value) = arg.to_str().and_then(|arg| arg.strip_prefix("--event=")) {
            record_v1_event(&mut event, Some(OsStr::new(value)));
        } else if arg.to_string_lossy().starts_with("--event=") {
            record_v1_event(&mut event, None);
        }
        if arg.to_str().and_then(|arg| arg.strip_prefix("--format=")) == Some("json") {
            json = true;
        }
        if let Some(value) = arg.to_str().and_then(|arg| arg.strip_prefix("--config=")) {
            config_explicit = true;
            config = config.or_else(|| Some(PathBuf::from(value)));
        } else if arg.to_string_lossy().starts_with("--config=") {
            config_explicit = true;
        }
    }

    if !json {
        return None;
    }
    let config = config.unwrap_or_else(|| PathBuf::from(".ironlint.yml"));
    let selected_event = match event {
        V1UsageEvent::Absent => "accept",
        V1UsageEvent::Valid(event) => event,
        V1UsageEvent::Invalid => "invalid",
    };
    let Ok(config) = commands::config::resolve_config(&config) else {
        return matches!(event, V1UsageEvent::Valid(_)).then_some(selected_event);
    };
    (commands::config::is_versioned_config(&config)
        || (matches!(event, V1UsageEvent::Valid(_)) && !config_explicit))
        .then_some(selected_event)
}

#[derive(Clone, Copy)]
enum V1UsageEvent {
    Absent,
    Valid(&'static str),
    Invalid,
}

fn record_v1_event(event: &mut V1UsageEvent, value: Option<&OsStr>) {
    *event = match (*event, v1_event(value)) {
        (V1UsageEvent::Absent, Some(event)) => V1UsageEvent::Valid(event),
        _ => V1UsageEvent::Invalid,
    };
}

fn v1_event(value: Option<&OsStr>) -> Option<&'static str> {
    match value {
        Some(value) if value == OsStr::new("change") => Some("change"),
        Some(value) if value == OsStr::new("accept") => Some("accept"),
        _ => None,
    }
}
