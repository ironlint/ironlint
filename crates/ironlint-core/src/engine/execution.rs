//! The bounded command executor used by the versioned v1 evaluator.
//!
//! This intentionally remains separate from [`super::gate`]. The legacy path
//! feeds proposed content and exposes its per-file ABI; v1 reads the tree,
//! closes stdin, and exposes only its three reserved variables.

use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

const OUTPUT_CAP: usize = 64 * 1024;
const READ_CHUNK: usize = 8 * 1024;
const DRAIN_GRACE: Duration = Duration::from_millis(20);
const ALLOWED_ENV_VARS: &[&str] = &["PATH", "HOME", "LANG", "TZ", "TMPDIR"];

/// The fixed v1 command ABI. Changed paths are selection input, not command
/// arguments; scripts inspect the checked tree through `root` themselves.
pub(crate) struct V1ExecutionEnv<'a> {
    pub(crate) root: &'a Path,
    pub(crate) event: &'a str,
    pub(crate) bin: &'a Path,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum V1ExecutionOutcome {
    Pass,
    Violation,
    Error(V1ExecutionError),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum V1ExecutionError {
    NotFound,
    NotExecutable,
    Timeout,
    DeadlineOverflow,
    Signal(i32),
    HighExit(i32),
    Spawn(String),
}

#[derive(Debug)]
pub(crate) struct V1ExecutionResult {
    pub(crate) outcome: V1ExecutionOutcome,
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
}

#[derive(Default, Debug)]
struct CapturedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

/// Run one v1 command. `total_budget` is the caller's remaining invocation
/// budget; the caller should pass the remaining budget for each serial check.
/// The effective deadline is the smaller of that budget and `check_timeout`.
pub(crate) fn run_v1(
    run: &str,
    env: &V1ExecutionEnv<'_>,
    check_timeout: Duration,
    total_budget: Duration,
) -> V1ExecutionResult {
    let Some(deadline) = Instant::now().checked_add(check_timeout.min(total_budget)) else {
        return result(
            V1ExecutionOutcome::Error(V1ExecutionError::DeadlineOverflow),
            None,
            &[],
            &[],
            false,
            false,
        );
    };
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(run)
        .current_dir(env.root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.env_clear();
    command.envs(build_v1_env(env, &std::env::vars_os().collect::<Vec<_>>()));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return result(
                V1ExecutionOutcome::Error(V1ExecutionError::Spawn(error.to_string())),
                None,
                &[],
                &[],
                false,
                false,
            )
        }
    };
    drop(child.stdin.take());

    let stdout = Arc::new(Mutex::new(CapturedOutput::default()));
    let stderr = Arc::new(Mutex::new(CapturedOutput::default()));
    let stdout_pipe = child.stdout.take().expect("stdout piped");
    let stderr_pipe = child.stderr.take().expect("stderr piped");
    #[cfg(unix)]
    {
        make_nonblocking(&stdout_pipe);
        make_nonblocking(&stderr_pipe);
    }
    let draining = Arc::new(AtomicBool::new(true));
    let stdout_done = spawn_drain(stdout_pipe, Arc::clone(&stdout), Arc::clone(&draining));
    let stderr_done = spawn_drain(stderr_pipe, Arc::clone(&stderr), Arc::clone(&draining));
    let status = match child.wait_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(Some(status)) => status,
        Ok(None) => {
            stop_child(&mut child);
            reap_child(&mut child);
            wait_for_drain(&stdout_done, &stderr_done, Instant::now(), &draining);
            return captured_result(
                V1ExecutionOutcome::Error(V1ExecutionError::Timeout),
                None,
                &stdout,
                &stderr,
            );
        }
        Err(error) => {
            stop_child(&mut child);
            reap_child(&mut child);
            wait_for_drain(&stdout_done, &stderr_done, Instant::now(), &draining);
            return captured_result(
                V1ExecutionOutcome::Error(V1ExecutionError::Spawn(error.to_string())),
                None,
                &stdout,
                &stderr,
            );
        }
    };

    stop_child(&mut child);
    wait_for_drain(&stdout_done, &stderr_done, Instant::now(), &draining);
    let (exit_code, outcome) = classify(status);
    captured_result(outcome, exit_code, &stdout, &stderr)
}

fn result(
    outcome: V1ExecutionOutcome,
    exit_code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
    stdout_truncated: bool,
    stderr_truncated: bool,
) -> V1ExecutionResult {
    V1ExecutionResult {
        outcome,
        exit_code,
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
        stdout_truncated,
        stderr_truncated,
    }
}

fn captured_result(
    outcome: V1ExecutionOutcome,
    exit_code: Option<i32>,
    stdout: &Arc<Mutex<CapturedOutput>>,
    stderr: &Arc<Mutex<CapturedOutput>>,
) -> V1ExecutionResult {
    let stdout = stdout
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let stderr = stderr
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    result(
        outcome,
        exit_code,
        &stdout.bytes,
        &stderr.bytes,
        stdout.truncated,
        stderr.truncated,
    )
}

fn spawn_drain(
    mut pipe: impl Read + Send + 'static,
    captured: Arc<Mutex<CapturedOutput>>,
    draining: Arc<AtomicBool>,
) -> mpsc::Receiver<()> {
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut chunk = [0u8; READ_CHUNK];
        loop {
            let read = match pipe.read(&mut chunk) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if !draining.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
                Ok(0) | Err(_) => break,
                Ok(read) => read,
            };
            let mut output = captured
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let room = OUTPUT_CAP.saturating_sub(output.bytes.len());
            output.bytes.extend_from_slice(&chunk[..read.min(room)]);
            if read > room {
                output.truncated = true;
            }
        }
        let _ = done_tx.send(());
    });
    done_rx
}

fn wait_for_drain(
    stdout: &mpsc::Receiver<()>,
    stderr: &mpsc::Receiver<()>,
    deadline: Instant,
    draining: &AtomicBool,
) {
    let stdout_done = recv_until(stdout, deadline);
    let stderr_done = recv_until(stderr, deadline);
    if stdout_done && stderr_done {
        return;
    }
    draining.store(false, Ordering::Relaxed);
    let grace_deadline = Instant::now() + DRAIN_GRACE;
    if !stdout_done {
        let _ = recv_until(stdout, grace_deadline);
    }
    if !stderr_done {
        let _ = recv_until(stderr, grace_deadline);
    }
}

fn recv_until(receiver: &mpsc::Receiver<()>, deadline: Instant) -> bool {
    receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .is_ok()
}

fn build_v1_env(
    env: &V1ExecutionEnv<'_>,
    source: &[(OsString, OsString)],
) -> Vec<(OsString, OsString)> {
    let mut vars: Vec<_> = source
        .iter()
        .filter(|(name, _)| {
            let name = name.to_string_lossy();
            ALLOWED_ENV_VARS.contains(&name.as_ref()) || name.starts_with("LC_")
        })
        .cloned()
        .collect();
    vars.extend([
        (
            OsString::from("IRONLINT_ROOT"),
            env.root.as_os_str().to_os_string(),
        ),
        (OsString::from("IRONLINT_EVENT"), OsString::from(env.event)),
        (
            OsString::from("IRONLINT_BIN"),
            env.bin.as_os_str().to_os_string(),
        ),
    ]);
    vars
}

fn classify(status: ExitStatus) -> (Option<i32>, V1ExecutionOutcome) {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return (
                None,
                V1ExecutionOutcome::Error(V1ExecutionError::Signal(signal)),
            );
        }
    }
    match status.code() {
        Some(0) => (Some(0), V1ExecutionOutcome::Pass),
        Some(126) => (
            Some(126),
            V1ExecutionOutcome::Error(V1ExecutionError::NotExecutable),
        ),
        Some(127) => (
            Some(127),
            V1ExecutionOutcome::Error(V1ExecutionError::NotFound),
        ),
        Some(code) if code >= 128 => (
            Some(code),
            V1ExecutionOutcome::Error(V1ExecutionError::HighExit(code)),
        ),
        Some(code) if (1..=125).contains(&code) => (Some(code), V1ExecutionOutcome::Violation),
        _ => (
            None,
            V1ExecutionOutcome::Error(V1ExecutionError::HighExit(-1)),
        ),
    }
}

#[cfg(unix)]
fn make_nonblocking(pipe: &impl std::os::fd::AsFd) {
    use nix::fcntl::{fcntl, FcntlArg, OFlag};

    let Ok(flags) = fcntl(pipe, FcntlArg::F_GETFL) else {
        return;
    };
    let _ = fcntl(
        pipe,
        FcntlArg::F_SETFL(OFlag::from_bits_retain(flags) | OFlag::O_NONBLOCK),
    );
}

#[cfg(unix)]
fn kill_process_group(child: &Child) {
    use nix::sys::signal::{killpg, Signal};
    use nix::unistd::Pid;
    let _ = killpg(Pid::from_raw(child.id().cast_signed()), Signal::SIGKILL);
}

#[cfg(not(unix))]
fn kill_process_group(child: &mut Child) {
    let _ = child.kill();
}

#[cfg(unix)]
fn stop_child(child: &mut Child) {
    kill_process_group(child);
    let _ = child.kill();
}

#[cfg(not(unix))]
fn stop_child(child: &mut Child) {
    kill_process_group(child);
}

fn reap_child(child: &mut Child) {
    let _ = child.wait_timeout(DRAIN_GRACE);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(root: &'a Path, event: &'a str) -> V1ExecutionEnv<'a> {
        V1ExecutionEnv {
            root,
            event,
            bin: root,
        }
    }

    #[test]
    fn v1_closes_stdin_and_filters_legacy_environment() {
        let dir = tempfile::tempdir().unwrap();
        let out = run_v1(
            "test -z \"$(cat)\" && test \"$IRONLINT_EVENT\" = accept && test -n \"$IRONLINT_ROOT\" && test -n \"$IRONLINT_BIN\" && test -z \"$IRONLINT_FILE\" && test -z \"$IRONLINT_TMPFILE\" && test -z \"$IRONLINT_PROPOSED_MANIFEST\" && test -z \"$SECRET_TOKEN\"",
            &env(dir.path(), "accept"),
            Duration::from_secs(2),
            Duration::from_secs(2),
        );
        assert_eq!(out.outcome, V1ExecutionOutcome::Pass);
    }

    #[test]
    fn v1_environment_has_only_safe_and_reserved_variables() {
        let dir = tempfile::tempdir().unwrap();
        let vars = build_v1_env(
            &env(dir.path(), "change"),
            &[
                (OsString::from("PATH"), OsString::from("/safe")),
                (OsString::from("LC_ALL"), OsString::from("C")),
                (OsString::from("IRONLINT_FILE"), OsString::from("legacy")),
                (OsString::from("SECRET_TOKEN"), OsString::from("secret")),
            ],
        );
        let names: Vec<_> = vars.iter().map(|(name, _)| name).collect();
        assert!(names.contains(&&OsString::from("PATH")));
        assert!(names.contains(&&OsString::from("LC_ALL")));
        assert!(names.contains(&&OsString::from("IRONLINT_ROOT")));
        assert!(names.contains(&&OsString::from("IRONLINT_EVENT")));
        assert!(names.contains(&&OsString::from("IRONLINT_BIN")));
        assert!(!names.contains(&&OsString::from("IRONLINT_FILE")));
        assert!(!names.contains(&&OsString::from("SECRET_TOKEN")));
    }

    #[test]
    fn output_is_capped_but_drained_on_both_streams() {
        let dir = tempfile::tempdir().unwrap();
        let out = run_v1(
            "head -c 100000 /dev/zero; head -c 100000 /dev/zero >&2; exit 1",
            &env(dir.path(), "accept"),
            Duration::from_secs(2),
            Duration::from_secs(2),
        );
        assert_eq!(out.outcome, V1ExecutionOutcome::Violation);
        assert_eq!(out.exit_code, Some(1));
        assert_eq!(out.stdout.len(), OUTPUT_CAP);
        assert_eq!(out.stderr.len(), OUTPUT_CAP);
        assert!(out.stdout_truncated);
        assert!(out.stderr_truncated);
    }

    #[test]
    fn classifies_unavailable_and_signal_execution() {
        let dir = tempfile::tempdir().unwrap();
        let missing = run_v1(
            "definitely-not-a-real-command-ironlint-v1",
            &env(dir.path(), "accept"),
            Duration::from_secs(2),
            Duration::from_secs(2),
        );
        assert_eq!(
            missing.outcome,
            V1ExecutionOutcome::Error(V1ExecutionError::NotFound)
        );

        #[cfg(unix)]
        {
            let signal = run_v1(
                "kill -TERM $$",
                &env(dir.path(), "accept"),
                Duration::from_secs(2),
                Duration::from_secs(2),
            );
            assert_eq!(
                signal.outcome,
                V1ExecutionOutcome::Error(V1ExecutionError::Signal(15))
            );
            assert_eq!(signal.exit_code, None);
        }
    }

    #[test]
    fn timeout_cleans_up_and_returns_within_budget() {
        let dir = tempfile::tempdir().unwrap();
        let start = Instant::now();
        let out = run_v1(
            "while :; do :; done",
            &env(dir.path(), "accept"),
            Duration::from_millis(100),
            Duration::from_millis(100),
        );
        assert_eq!(
            out.outcome,
            V1ExecutionOutcome::Error(V1ExecutionError::Timeout)
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn impossible_deadline_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let out = run_v1(
            "true",
            &env(dir.path(), "accept"),
            Duration::MAX,
            Duration::MAX,
        );
        assert_eq!(
            out.outcome,
            V1ExecutionOutcome::Error(V1ExecutionError::DeadlineOverflow)
        );
    }

    #[cfg(unix)]
    #[test]
    fn descendant_holding_pipes_does_not_extend_completed_command() {
        let dir = tempfile::tempdir().unwrap();
        let start = Instant::now();
        let out = run_v1(
            "sleep 1 & exit 0",
            &env(dir.path(), "accept"),
            Duration::from_secs(2),
            Duration::from_secs(2),
        );
        assert_eq!(out.outcome, V1ExecutionOutcome::Pass);
        assert!(start.elapsed() < Duration::from_millis(900));
    }

    #[cfg(unix)]
    #[test]
    fn escaped_descendant_holding_pipes_does_not_extend_completed_command() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("escaped.pid");
        let script = "use POSIX; my $m=$ARGV[0]; my $pid=fork(); if($pid==0){ POSIX::setsid(); open(F,\">\",$m); print F $$; close F; exec(\"/bin/sleep\",\"2\"); } while(! -e $m){ select(undef,undef,undef,0.01); }";
        let start = Instant::now();
        let out = run_v1(
            &format!("/usr/bin/perl -e '{script}' -- '{}'", marker.display()),
            &env(dir.path(), "accept"),
            Duration::from_secs(2),
            Duration::from_secs(2),
        );
        assert_eq!(out.outcome, V1ExecutionOutcome::Pass);
        assert!(start.elapsed() < Duration::from_millis(500));
    }

    #[cfg(unix)]
    #[test]
    fn escaped_descendant_cannot_keep_drain_threads_alive() {
        let dir = tempfile::tempdir().unwrap();
        let start = Instant::now();
        let out = run_v1(
            "setsid sleep 2 & while :; do :; done",
            &env(dir.path(), "accept"),
            Duration::from_millis(50),
            Duration::from_millis(50),
        );
        assert_eq!(
            out.outcome,
            V1ExecutionOutcome::Error(V1ExecutionError::Timeout)
        );
        assert!(start.elapsed() < Duration::from_millis(500));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_reaps_direct_child_that_leaves_its_process_group() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("escaped.pid");
        let start = Instant::now();
        let out = run_v1(
            &format!(
                "exec ruby -e 'Process.setpgid(0, Process.getpgid(Process.ppid)); File.write(ARGV.fetch(0), Process.pid.to_s); loop {{}}' '{}'",
                marker.display()
            ),
            &env(dir.path(), "accept"),
            Duration::from_millis(500),
            Duration::from_millis(500),
        );
        assert_eq!(
            out.outcome,
            V1ExecutionOutcome::Error(V1ExecutionError::Timeout)
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        let pid: i32 = std::fs::read_to_string(marker)
            .unwrap_or_else(|error| {
                panic!(
                    "escaped child should write its pid before the timeout: {error}; stderr: {}",
                    String::from_utf8_lossy(&out.stderr)
                )
            })
            .parse()
            .expect("marker should contain an integer pid");
        assert_eq!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
            Err(nix::errno::Errno::ESRCH),
            "escaped direct child {pid} survived cleanup"
        );
    }
}
