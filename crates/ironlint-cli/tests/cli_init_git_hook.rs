use assert_cmd::Command;
use std::fs;
use std::path::Path;

const MARKER_START: &str = "# >>> ironlint pre-commit floor >>>";
const MARKER_END: &str = "# <<< ironlint pre-commit floor <<<";

fn git_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .unwrap();
    assert!(status.success());
    dir
}

fn hook_file(dir: &Path) -> std::path::PathBuf {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(dir)
        .output()
        .unwrap();
    let common = String::from_utf8(output.stdout).unwrap();
    let common = Path::new(common.trim());
    let common = if common.is_absolute() {
        common.to_path_buf()
    } else {
        dir.join(common)
    };
    common.join("hooks/pre-commit")
}

fn init(dir: &Path, args: &[&str]) {
    let xdg = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("ironlint")
        .unwrap()
        .current_dir(dir)
        .env("XDG_CONFIG_HOME", xdg.path())
        .env("HOME", home.path())
        .arg("init")
        .args(args)
        .assert()
        .success();
}

#[test]
fn default_init_does_not_install_git_hook() {
    let dir = git_repo();
    init(dir.path(), &["--yes"]);
    assert!(!hook_file(dir.path()).exists());
}

#[test]
fn git_hook_flag_installs_v1_acceptance_hook() {
    let dir = git_repo();
    init(dir.path(), &["--yes", "--git-hook"]);
    let hook = hook_file(dir.path());
    let body = fs::read_to_string(&hook).unwrap();
    assert!(body.contains(MARKER_START) && body.contains(MARKER_END));
    assert!(body.contains("check --event accept --root"));
    assert!(!body.contains("check --diff"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(fs::metadata(hook).unwrap().permissions().mode() & 0o111, 0);
    }
}

#[test]
fn repeated_git_hook_install_is_byte_identical() {
    let dir = git_repo();
    init(dir.path(), &["--yes", "--git-hook"]);
    let hook = hook_file(dir.path());
    let before = fs::read(&hook).unwrap();
    init(dir.path(), &["--yes", "--git-hook"]);
    assert_eq!(fs::read(hook).unwrap(), before);
}

#[test]
fn uninstall_removes_owned_git_hook_without_extra_flag() {
    let dir = git_repo();
    init(dir.path(), &["--yes", "--git-hook"]);
    let hook = hook_file(dir.path());
    assert!(hook.exists());
    init(dir.path(), &["--yes", "--uninstall"]);
    assert!(!hook.exists());
}

#[test]
fn uninstall_preserves_custom_hook_content() {
    let dir = git_repo();
    let hook = hook_file(dir.path());
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    fs::write(&hook, "#!/bin/sh\necho custom\n").unwrap();
    init(dir.path(), &["--yes", "--git-hook"]);
    init(dir.path(), &["--yes", "--uninstall"]);
    assert_eq!(
        fs::read_to_string(hook).unwrap(),
        "#!/bin/sh\necho custom\n\n"
    );
}
