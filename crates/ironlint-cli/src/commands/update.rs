//! `ironlint update` — self-update to the latest GitHub release.
//!
//! Shells out to the dist installer (`ironlint-cli-installer.sh` on Unix,
//! `ironlint-cli-installer.ps1` on Windows) rather than embedding an HTTP
//! client. The previous implementation used the `axoupdater` crate, which
//! pulled the entire async TLS stack (`tokio`, `reqwest`, `hyper-rustls`,
//! `rustls`, …) into the binary for a single sync command — the heaviest
//! compile-time cost in the dependency tree. Re-running the same installer
//! the user originally installed with is the operation `axoupdater` performed
//! under the hood; doing it directly removes ~30 transitive crates and keeps
//! the behavior identical from the user's perspective.
//!
//! The installer is only meaningful when this binary was installed *by* the
//! installer — otherwise there's no receipt and no install prefix to target.
//! We detect that the same way `axoupdater` did: look for the dist install
//! receipt (`ironlint-cli-receipt.json`) under the config dir it would have
//! been written to. All decision/formatting logic lives in the pure `render`
//! helper (unit-tested in-process); the only I/O is the small [`perform`]
//! shim that resolves the receipt and runs bounded download/install subprocesses.
//! End-to-end tests use isolated receipts and fake downloaders; no live update
//! is needed to exercise success or partial-download failure.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;

/// The dist *app name* (the package name `ironlint-cli`, not the `ironlint`
/// binary) — this is what dist prefixes installer assets with and what the
/// install receipt is keyed to (`~/.config/ironlint-cli/`), so it's the name
/// the receipt file is named after (`ironlint-cli-receipt.json`). Verified
/// against the shipped `dist-manifest.json` and `ironlint-cli-installer.sh`.
const APP_NAME: &str = "ironlint-cli";

/// The Unix installer the no-receipt deferral points at — the same script
/// the curl one-liner runs, and what `perform` execs on a receipt-managed
/// install. Matches the released asset name (`ironlint-cli-installer.sh`).
const INSTALLER_SH_URL: &str =
    "https://github.com/ironlint/ironlint/releases/latest/download/ironlint-cli-installer.sh";

/// The Windows installer — the PowerShell equivalent of [`INSTALLER_SH_URL`].
/// `perform` execs this via `powershell` on a receipt-managed Windows install.
const INSTALLER_PS1_URL: &str =
    "https://github.com/ironlint/ironlint/releases/latest/download/ironlint-cli-installer.ps1";

/// Repo root, for the `cargo install --git` line in the no-receipt deferral.
/// `ironlint-cli` isn't published to crates.io, so a bare `cargo install
/// ironlint-cli` would fail — the git form is the correct one.
const REPO_URL: &str = "https://github.com/ironlint/ironlint";

/// The classified result of an update attempt — everything `render` needs to
/// produce output and an exit code, with no live updater state.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// Self-update succeeded (the installer ran and exited 0). We don't
    /// introspect the new version — the installer prints its own progress —
    /// so this carries no version pair, only the success signal.
    Updated,
    /// No install receipt: this build came from Homebrew, `cargo install`, or a
    /// source build, so it can't drive the installer. Defer to its real channel.
    NotInstallerManaged,
    /// Anything else went wrong; carries a human-readable reason.
    Failed { reason: String },
}

/// `ironlint update` entry point. Returns the process exit code: `0` on a
/// successful update, `1` on any failure (including not-installer-managed).
pub fn run() -> Result<i32> {
    let (message, code) = render(&perform());
    if code == 0 {
        println!("{message}");
    } else {
        eprintln!("{message}");
    }
    Ok(code)
}

/// Resolve the receipt and, if present, run the installer. The only
/// network/filesystem surface in this module; kept deliberately tiny so the
/// tested logic sits in the pure helpers below.
fn perform() -> Outcome {
    if !receipt_exists(APP_NAME) {
        return Outcome::NotInstallerManaged;
    }
    run_installer()
}

#[derive(Clone, Copy)]
struct UpdateLimits {
    download: Duration,
    total: Duration,
    cleanup: Duration,
}

const UPDATE_LIMITS: UpdateLimits = UpdateLimits {
    download: Duration::from_secs(60),
    total: Duration::from_secs(300),
    cleanup: Duration::from_secs(1),
};

/// Download completely before executing, retaining the installer's stdio and
/// checking both outcomes. The private seam supplies isolated tools and short
/// deadlines to tests without changing production environment semantics.
fn run_installer() -> Outcome {
    let (downloader, installer) = if cfg!(windows) {
        ("powershell", "powershell")
    } else {
        ("curl", "sh")
    };
    run_installer_with(
        Path::new(downloader),
        Path::new(installer),
        &std::env::temp_dir(),
        UPDATE_LIMITS,
    )
}

fn run_installer_with(
    downloader: &Path,
    installer: &Path,
    temp_root: &Path,
    limits: UpdateLimits,
) -> Outcome {
    let Some(deadline) = Instant::now().checked_add(limits.total) else {
        return Outcome::Failed {
            reason: "update deadline overflow".into(),
        };
    };
    let mut temporary = match TemporaryInstaller::create(temp_root) {
        Ok(temporary) => temporary,
        Err(reason) => return Outcome::Failed { reason },
    };
    let result = download_and_install(downloader, installer, &temporary.path, deadline, limits);
    let cleanup = temporary.remove();
    match (result, cleanup) {
        (Ok(()), Ok(())) => Outcome::Updated,
        (Err(reason), Ok(())) => Outcome::Failed { reason },
        (result, Err(error)) => Outcome::Failed {
            reason: format!(
                "{}; failed to remove temporary installer {}: {error}",
                result.err().unwrap_or_else(|| "installer completed".into()),
                temporary.path.display()
            ),
        },
    }
}

fn download_and_install(
    downloader: &Path,
    installer: &Path,
    path: &Path,
    deadline: Instant,
    limits: UpdateLimits,
) -> std::result::Result<(), String> {
    let download_deadline = Instant::now()
        .checked_add(limits.download)
        .unwrap_or(deadline)
        .min(deadline);
    run_stage(
        download_command(downloader, path),
        "download",
        download_deadline,
        limits.cleanup,
    )?;
    installer_file(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("failed to finalize downloaded installer: {error}"))?;
    run_stage(
        installer_command(installer, path),
        "installer",
        deadline,
        limits.cleanup,
    )
}

fn installer_file(path: &Path) -> std::io::Result<File> {
    // Windows FlushFileBuffers requires write access. Neither create nor
    // truncate is enabled: finalization must retain the downloaded bytes.
    OpenOptions::new().read(true).write(true).open(path)
}

fn download_command(downloader: &Path, path: &Path) -> Command {
    let mut command = Command::new(downloader);
    if cfg!(windows) {
        command
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command"])
            .arg(format!(
                "try {{ Invoke-WebRequest -Uri '{INSTALLER_PS1_URL}' -OutFile $env:IRONLINT_UPDATE_INSTALLER -ErrorAction Stop }} catch {{ Write-Error $_; exit 1 }}"
            ))
            .env("IRONLINT_UPDATE_INSTALLER", path);
    } else {
        command
            .args(["-LsSf", "--max-time", "60", "-o"])
            .arg(path)
            .arg(INSTALLER_SH_URL);
    }
    command
}

fn installer_command(installer: &Path, path: &Path) -> Command {
    let mut command = Command::new(installer);
    if cfg!(windows) {
        command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
    }
    command.arg(path);
    command
}

fn run_stage(
    mut command: Command,
    stage: &str,
    deadline: Instant,
    cleanup: Duration,
) -> std::result::Result<(), String> {
    if Instant::now() >= deadline {
        return Err(format!("{stage} deadline exceeded"));
    }
    command.stdin(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to spawn {stage}: {error}"))?;
    match wait_until(&mut child, deadline) {
        Ok(Some(status)) if status.success() => Ok(()),
        Ok(Some(status)) => Err(format!(
            "{stage} exited with code {}",
            status.code().unwrap_or(-1)
        )),
        result => {
            let reason = match result {
                Ok(None) => format!("{stage} deadline exceeded"),
                Err(error) => format!("failed to wait for {stage}: {error}"),
                Ok(Some(_)) => unreachable!(),
            };
            Err(stop_stage(&mut child, cleanup)
                .map_or(reason.clone(), |error| format!("{reason}; {error}")))
        }
    }
}

fn wait_until(child: &mut Child, deadline: Instant) -> std::io::Result<Option<ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        std::thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

fn stop_stage(child: &mut Child, cleanup: Duration) -> Option<String> {
    #[cfg(unix)]
    {
        use nix::sys::signal::{killpg, Signal};
        use nix::unistd::Pid;
        let _ = killpg(Pid::from_raw(child.id().cast_signed()), Signal::SIGKILL);
    }
    let _ = child.kill();
    let deadline = Instant::now()
        .checked_add(cleanup)
        .unwrap_or_else(Instant::now);
    match wait_until(child, deadline) {
        Ok(Some(_)) => None,
        Ok(None) => Some("child cleanup deadline exceeded; process exit was not observed".into()),
        Err(error) => Some(format!("failed to reap child: {error}")),
    }
}

struct TemporaryInstaller {
    path: PathBuf,
    removed: bool,
}

impl TemporaryInstaller {
    fn create(root: &Path) -> std::result::Result<Self, String> {
        static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
        let suffix = if cfg!(windows) { "ps1" } else { "sh" };
        for _ in 0..100 {
            let path = root.join(format!(
                "ironlint-update-{}-{}.{suffix}",
                std::process::id(),
                NEXT_FILE.fetch_add(1, Ordering::Relaxed)
            ));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(_) => {
                    return Ok(Self {
                        path,
                        removed: false,
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(format!("failed to create temporary installer: {error}")),
            }
        }
        Err("temporary installer name collisions exhausted".into())
    }

    fn remove(&mut self) -> std::io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.removed = true;
        Ok(())
    }
}

impl Drop for TemporaryInstaller {
    fn drop(&mut self) {
        if !self.removed {
            let _ = self.remove();
        }
    }
}

/// Whether a dist install receipt exists for `app_name` in any of the
/// locations the dist installer writes one. Mirrors `axoupdater`'s
/// `get_config_paths` resolution order so the no-receipt path is deterministic
/// under the same env vars the e2e test sets (`AXOUPDATER_CONFIG_PATH`,
/// `XDG_CONFIG_HOME`, `HOME`, `LOCALAPPDATA`). Existence-only — we don't
/// parse the receipt, so dist receipt-schema changes can't break us.
fn receipt_exists(app_name: &str) -> bool {
    receipt_exists_in(&receipt_paths(app_name), app_name)
}

/// Pure core of [`receipt_exists`]: true if any candidate dir holds a
/// `<app_name>-receipt.json`. Split out so tests can exercise the resolution
/// without mutating process env (which races under parallel test threads).
fn receipt_exists_in(dirs: &[PathBuf], app_name: &str) -> bool {
    let receipt = format!("{app_name}-receipt.json");
    dirs.iter().any(|d| d.join(&receipt).exists())
}

/// Candidate config dirs that may hold a receipt, in resolution order:
/// `AXOUPDATER_CONFIG_WORKING_DIR` (cwd) → `AXOUPDATER_CONFIG_PATH` (literal
/// dir, returned alone) → `XDG_CONFIG_HOME/<app>` → (Unix) `$HOME/.config/<app>`
/// / (Windows) `%LOCALAPPDATA%/<app>`. Matches `axoupdater::get_config_paths`.
///
/// Two deliberate divergences from axoupdater, both to avoid re-adding a dep:
/// (1) `AXOUPDATER_CONFIG_PATH` early-returns like axoupdater, but the
/// `AXOUPDATER_CONFIG_WORKING_DIR` branch uses `std::env::current_dir` rather
/// than axoupdater's `Utf8PathBuf` conversion (same observable result);
/// (2) the Unix home fallback reads `$HOME` directly instead of
/// `homedir::my_home()` (which falls back to getpwuid when `HOME` is unset).
/// Environments with `HOME` unset (some systemd units, Docker containers) won't
/// find a receipt — accepted as a rare edge case not worth a `homedir` dep.
fn receipt_paths(app_name: &str) -> Vec<PathBuf> {
    // axoupdater checks this first: if set, look in the cwd only.
    if std::env::var("AXOUPDATER_CONFIG_WORKING_DIR").is_ok() {
        return std::env::current_dir().map(|d| vec![d]).unwrap_or_default();
    }
    // If set, axoupdater returns *only* this path — no fallthrough to XDG/HOME.
    if let Ok(p) = std::env::var("AXOUPDATER_CONFIG_PATH") {
        return vec![PathBuf::from(p)];
    }
    let mut paths = Vec::new();
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        paths.push(PathBuf::from(xdg).join(app_name));
    }
    if cfg!(windows) {
        if let Ok(la) = std::env::var("LOCALAPPDATA") {
            paths.push(PathBuf::from(la).join(app_name));
        }
    } else if let Ok(home) = std::env::var("HOME") {
        paths.push(PathBuf::from(home).join(".config").join(app_name));
    }
    paths
}

/// Appended to a successful-update message. A newer binary can embed newer hook
/// artifacts than the copies already materialized into the user's coding agents,
/// and `update` deliberately touches only the binary — so nudge them to
/// re-materialize the hooks. `ironlint init --hook-only` re-wires hooks
/// idempotently without rescaffolding the config.
const REFRESH_HINT: &str =
    "  hooks may be newer in this build — refresh them: ironlint init --hook-only";

/// Turn an `Outcome` into a user-facing message and exit code. Pure.
fn render(outcome: &Outcome) -> (String, i32) {
    match outcome {
        Outcome::Updated => (format!("updated ironlint\n{REFRESH_HINT}"), 0),
        Outcome::NotInstallerManaged => (not_installer_managed_message(), 1),
        Outcome::Failed { reason } => (format!("error: update failed: {reason}"), 1),
    }
}

/// The deferral shown when there's no install receipt: point the user at the
/// channel that *will* update their binary.
fn not_installer_managed_message() -> String {
    format!(
        "error: this ironlint wasn't installed by the ironlint installer, so it can't self-update.\n  \
         • reinstall (recommended): curl -LsSf {INSTALLER_SH_URL} | sh\n  \
         • or, with cargo:          cargo install --git {REPO_URL} ironlint-cli --force"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        values: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl EnvGuard {
        fn set(values: &[(&'static str, Option<&str>)]) -> Self {
            let saved = values
                .iter()
                .map(|(name, value)| {
                    let original = std::env::var_os(name);
                    match value {
                        Some(value) => std::env::set_var(name, value),
                        None => std::env::remove_var(name),
                    }
                    (*name, original)
                })
                .collect();
            Self { values: saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (name, value) in &self.values {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn env_guard_restores_non_unicode_values() {
        use std::os::unix::ffi::OsStringExt;

        let _lock = ENV_LOCK.lock().unwrap();
        let _restore_original = EnvGuard::set(&[("HOME", Some("/__ironlint_test_restore__"))]);
        let non_unicode = std::ffi::OsString::from_vec(vec![b'/', 0xFF]);
        std::env::set_var("HOME", &non_unicode);

        {
            let _guard = EnvGuard::set(&[("HOME", Some("/__ironlint_test_override__"))]);
        }

        assert_eq!(std::env::var_os("HOME"), Some(non_unicode));
    }

    #[test]
    fn render_updated_includes_refresh_hint_and_exits_zero() {
        let (msg, code) = render(&Outcome::Updated);
        assert_eq!(code, 0);
        assert!(msg.contains("updated ironlint"), "{msg}");
        // A fresh binary may ship newer hooks; the update output must nudge the
        // user to re-materialize them via the existing hook-only init path.
        assert!(msg.contains("ironlint init --hook-only"), "{msg}");
    }

    #[test]
    fn render_not_installer_managed_points_to_real_update_paths_and_exits_one() {
        let (msg, code) = render(&Outcome::NotInstallerManaged);
        assert_eq!(code, 1);
        assert!(msg.contains("can't self-update"), "{msg}");
        // The installer one-liner (also re-establishes the receipt for next time).
        assert!(msg.contains("ironlint-cli-installer.sh"), "{msg}");
        // The cargo path must be the git form — ironlint-cli isn't on crates.io.
        assert!(msg.contains("cargo install --git"), "{msg}");
        assert!(msg.contains("ironlint-cli --force"), "{msg}");
        // brew stays out until a tap actually exists.
        assert!(!msg.contains("brew"), "{msg}");
    }

    #[test]
    fn render_failed_includes_reason_and_exits_one() {
        let (msg, code) = render(&Outcome::Failed {
            reason: "installer exited with code 1".into(),
        });
        assert_eq!(code, 1);
        assert!(msg.contains("installer exited with code 1"), "{msg}");
        // A failed update leaves the binary as-is — no hook refresh to suggest.
        assert!(!msg.contains("hook-only"), "{msg}");
    }

    #[test]
    fn run_installer_failure_reason_has_no_doubled_exit_status_phrase() {
        // ExitStatus Display is "exit status: N"; the reason must render the
        // raw code so the message isn't "installer exited with code exit
        // status: 1". Drive the real format path via a failing `sh -c`.
        let status = std::process::Command::new("sh")
            .args(["-c", "exit 7"])
            .status()
            .unwrap();
        assert!(!status.success());
        let reason = format!("installer exited with code {}", status.code().unwrap_or(-1));
        assert_eq!(reason, "installer exited with code 7");
        assert!(
            !reason.contains("exit status"),
            "doubled phrase leaked into reason: {reason}"
        );
    }

    #[cfg(unix)]
    fn fake_tool(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    fn fake_download(dir: &Path, payload: &str, ending: &str) -> PathBuf {
        fake_tool(
            dir,
            "curl",
            &format!(
                "while [ \"$#\" -gt 0 ]; do
                   if [ \"$1\" = -o ]; then shift; target=$1; fi
                   shift
                 done
                 printf '%s\\n' '{}' > \"$target\"
                 {ending}",
                payload.replace('\'', "'\\''")
            ),
        )
    }

    #[cfg(unix)]
    fn short_limits() -> UpdateLimits {
        UpdateLimits {
            download: Duration::from_secs(2),
            total: Duration::from_secs(4),
            cleanup: Duration::from_secs(1),
        }
    }

    #[cfg(unix)]
    fn completion_limits() -> UpdateLimits {
        // Completion tests allow instrumented process startup under parallel
        // load; only the hung-process regressions need short deadlines.
        UpdateLimits {
            download: Duration::from_secs(10),
            total: Duration::from_secs(20),
            cleanup: Duration::from_secs(1),
        }
    }

    #[cfg(unix)]
    fn assert_no_temporary_installer(dir: &Path) {
        assert!(!std::fs::read_dir(dir).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("ironlint-update-")
        }));
    }

    #[cfg(unix)]
    fn assert_failed(outcome: Outcome, expected: &str) {
        match outcome {
            Outcome::Failed { reason } => assert!(reason.contains(expected), "{reason}"),
            other => panic!("expected {expected}, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn fully_downloaded_installer_executes_and_temporary_file_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("installer-ran");
        let downloader = fake_download(
            dir.path(),
            &format!("printf success > '{}'", marker.display()),
            "exit 0",
        );
        let outcome = run_installer_with(
            &downloader,
            Path::new("/bin/sh"),
            dir.path(),
            completion_limits(),
        );
        assert_eq!(outcome, Outcome::Updated);
        assert_eq!(std::fs::read_to_string(marker).unwrap(), "success");
        assert_no_temporary_installer(dir.path());
    }

    #[cfg(unix)]
    #[test]
    fn missing_downloader_or_installer_reports_the_failed_stage_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("absent");
        assert_failed(
            run_installer_with(
                &missing,
                Path::new("/bin/sh"),
                dir.path(),
                completion_limits(),
            ),
            "spawn download",
        );
        assert_no_temporary_installer(dir.path());
        let downloader = fake_download(dir.path(), "exit 0", "exit 0");
        assert_failed(
            run_installer_with(&downloader, &missing, dir.path(), completion_limits()),
            "spawn installer",
        );
        assert_no_temporary_installer(dir.path());
    }

    #[cfg(unix)]
    #[test]
    fn installer_nonzero_is_reported_and_temporary_file_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let downloader = fake_download(dir.path(), "exit 13", "exit 0");
        assert_failed(
            run_installer_with(
                &downloader,
                Path::new("/bin/sh"),
                dir.path(),
                completion_limits(),
            ),
            "installer exited with code 13",
        );
        assert_no_temporary_installer(dir.path());
    }

    #[cfg(unix)]
    #[test]
    fn downloader_deadline_stops_its_process_group_and_removes_partial_script() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("descendant.pid");
        let downloader = fake_download(
            dir.path(),
            "exit 0",
            &format!(
                "/bin/sleep 30 &\nprintf '%s' $! > '{}'\nwait",
                pid_file.display()
            ),
        );
        let started = Instant::now();
        assert_failed(
            run_installer_with(
                &downloader,
                Path::new("/bin/sh"),
                dir.path(),
                short_limits(),
            ),
            "download deadline exceeded",
        );
        assert!(started.elapsed() < Duration::from_secs(6));
        assert_pid_stopped(&pid_file);
        assert_no_temporary_installer(dir.path());
    }

    #[cfg(unix)]
    fn assert_pid_stopped(pid_file: &Path) {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        let pid = Pid::from_raw(std::fs::read_to_string(pid_file).unwrap().parse().unwrap());
        let deadline = Instant::now() + Duration::from_secs(1);
        while kill(pid, None).is_ok() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            kill(pid, None),
            Err(nix::errno::Errno::ESRCH),
            "child {pid} survived cleanup"
        );
    }

    #[cfg(unix)]
    #[test]
    fn installer_uses_remaining_overall_deadline_and_is_reaped() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("installer.pid");
        let downloader = fake_download(
            dir.path(),
            &format!(
                "printf '%s' $$ > '{}'\nexec /bin/sleep 30",
                pid_file.display()
            ),
            "/bin/sleep 0.10\nexit 0",
        );
        let started = Instant::now();
        assert_failed(
            run_installer_with(
                &downloader,
                Path::new("/bin/sh"),
                dir.path(),
                short_limits(),
            ),
            "installer deadline exceeded",
        );
        assert!(started.elapsed() < Duration::from_secs(6));
        assert_pid_stopped(&pid_file);
        assert_no_temporary_installer(dir.path());
    }

    #[cfg(unix)]
    #[test]
    fn expired_deadline_does_not_spawn_a_stage() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("spawned");
        let tool = fake_tool(
            dir.path(),
            "stage",
            &format!("touch '{}'", marker.display()),
        );
        assert_failed(
            Outcome::Failed {
                reason: run_stage(
                    Command::new(tool),
                    "download",
                    Instant::now(),
                    Duration::ZERO,
                )
                .unwrap_err(),
            },
            "download deadline exceeded",
        );
        assert!(!marker.exists());
    }

    #[cfg(unix)]
    #[test]
    fn unusable_temporary_directory_reports_failure_without_spawning() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("absent");
        assert_failed(
            run_installer_with(&missing, &missing, &missing, short_limits()),
            "create temporary installer",
        );
        assert_no_temporary_installer(dir.path());
    }

    #[cfg(unix)]
    #[test]
    fn update_deadline_overflow_does_not_create_or_execute_an_installer() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("absent");
        let limits = UpdateLimits {
            total: Duration::MAX,
            ..short_limits()
        };
        assert_failed(
            run_installer_with(&missing, &missing, dir.path(), limits),
            "deadline overflow",
        );
        assert_no_temporary_installer(dir.path());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_failure_is_reported_even_after_installer_success() {
        let dir = tempfile::tempdir().unwrap();
        let downloader = fake_download(dir.path(), "rm \"$0\"; mkdir \"$0\"", "exit 0");
        assert_failed(
            run_installer_with(
                &downloader,
                Path::new("/bin/sh"),
                dir.path(),
                completion_limits(),
            ),
            "failed to remove temporary installer",
        );
    }

    #[test]
    fn cleanup_guard_stops_owning_a_path_after_successful_removal() {
        let dir = tempfile::tempdir().unwrap();
        let mut temporary = TemporaryInstaller::create(dir.path()).unwrap();
        let path = temporary.path.clone();
        temporary.remove().unwrap();
        std::fs::write(&path, "foreign replacement").unwrap();
        drop(temporary);
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "foreign replacement"
        );
    }

    #[test]
    fn temporary_installer_guard_cleans_up_on_early_return() {
        let dir = tempfile::tempdir().unwrap();
        let temporary = TemporaryInstaller::create(dir.path()).unwrap();
        let path = temporary.path.clone();
        drop(temporary);
        assert!(!path.exists());
    }

    #[test]
    fn finalized_installer_handle_is_writable_without_truncating_downloaded_bytes() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("installer.sh");
        let contents = b"#!/bin/sh\nexit 0\n";
        std::fs::write(&path, contents).unwrap();
        let mut file = installer_file(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), contents);
        // Rewrite the same first byte: the handle must allow the write access
        // Windows requires for FlushFileBuffers, without changing the script.
        file.write_all(&contents[..1])
            .expect("finalization requires a writable handle");
        file.sync_all().unwrap();
        drop(file);
        assert_eq!(std::fs::read(path).unwrap(), contents);
    }

    #[test]
    fn receipt_paths_respects_axoupdater_config_path_first() {
        // When AXOUPDATER_CONFIG_PATH is set, axoupdater returns *only* that
        // path (no XDG/HOME fallthrough). Use a unique sentinel so a real env
        // var on the host can't mask the assertion.
        //
        // These tests mutate process-global env, so ENV_LOCK keeps their
        // receipt-path assertions from observing each other's setup.
        let _lock = ENV_LOCK.lock().unwrap();
        let sentinel = "/__ironlint_test_axoupdater_config_path__";
        let _env = EnvGuard::set(&[
            ("AXOUPDATER_CONFIG_WORKING_DIR", None),
            ("AXOUPDATER_CONFIG_PATH", Some(sentinel)),
        ]);
        let paths = receipt_paths(APP_NAME);
        // Early-return: the candidate list is exactly [sentinel], nothing else.
        assert_eq!(paths, vec![PathBuf::from(sentinel)]);
    }

    #[test]
    fn receipt_paths_prioritizes_working_directory_over_explicit_path() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::set(&[
            ("AXOUPDATER_CONFIG_WORKING_DIR", Some("1")),
            (
                "AXOUPDATER_CONFIG_PATH",
                Some("/__ironlint_test_axoupdater_config_path__"),
            ),
        ]);

        assert_eq!(
            receipt_paths(APP_NAME),
            vec![std::env::current_dir().unwrap()]
        );
    }

    #[test]
    fn receipt_paths_uses_xdg_then_platform_home_fallback() {
        let _lock = ENV_LOCK.lock().unwrap();
        let xdg = "/__ironlint_test_xdg_config_home__";
        let home = "/__ironlint_test_home__";
        let _env = EnvGuard::set(&[
            ("AXOUPDATER_CONFIG_WORKING_DIR", None),
            ("AXOUPDATER_CONFIG_PATH", None),
            ("XDG_CONFIG_HOME", Some(xdg)),
            ("HOME", Some(home)),
            ("LOCALAPPDATA", Some("/__ironlint_test_local_app_data__")),
        ]);

        let paths = receipt_paths(APP_NAME);
        assert_eq!(paths[0], PathBuf::from(xdg).join(APP_NAME));
        #[cfg(windows)]
        assert_eq!(
            paths[1],
            PathBuf::from("/__ironlint_test_local_app_data__").join(APP_NAME)
        );
        #[cfg(not(windows))]
        assert_eq!(paths[1], PathBuf::from(home).join(".config").join(APP_NAME));
    }

    #[test]
    fn receipt_exists_in_true_when_receipt_file_present_in_any_dir() {
        let with = tempfile::tempdir().unwrap();
        let without = tempfile::tempdir().unwrap();
        std::fs::write(with.path().join(format!("{APP_NAME}-receipt.json")), "{}").unwrap();
        // Present in the first candidate.
        assert!(receipt_exists_in(
            &[with.path().into(), without.path().into()],
            APP_NAME
        ));
        // Present in a later candidate.
        assert!(receipt_exists_in(
            &[without.path().into(), with.path().into()],
            APP_NAME
        ));
        // Absent everywhere.
        assert!(!receipt_exists_in(&[without.path().into()], APP_NAME));
    }

    #[test]
    fn installer_urls_point_at_ironlint_org_not_stale_christopherarter() {
        // Repo moved from christopherarter/ironlint to ironlint/ironlint;
        // the workspace Cargo.toml `repository` field is already updated, and
        // the installer URLs must match so `update` doesn't 404.
        assert!(
            INSTALLER_SH_URL.starts_with("https://github.com/ironlint/ironlint/"),
            "{INSTALLER_SH_URL}"
        );
        assert!(
            INSTALLER_PS1_URL.starts_with("https://github.com/ironlint/ironlint/"),
            "{INSTALLER_PS1_URL}"
        );
        assert_eq!(REPO_URL, "https://github.com/ironlint/ironlint");
    }
}
