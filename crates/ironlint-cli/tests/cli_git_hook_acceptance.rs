use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;

struct Fixture {
    _base: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    xdg: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let base = tempfile::tempdir().unwrap();
        let fixture = Self {
            root: base.path().join("repo"),
            home: base.path().join("home"),
            xdg: base.path().join("config"),
            _base: base,
        };
        for dir in [&fixture.root, &fixture.home, &fixture.xdg] {
            fs::create_dir(dir).unwrap();
        }
        fixture.git(&["init", "-q"]).assert().success();
        fs::write(
            fixture.root.join(".ironlint.yml"),
            "version: 1\nchecks:\n  no_bad:\n    run: grep -qx clean rule.txt\n    on: [accept]\n",
        )
        .unwrap();
        fs::write(fixture.root.join("rule.txt"), "clean\n").unwrap();
        fixture
            .cli(&["init", "--yes", "--git-hook"])
            .assert()
            .success();
        fixture.cli(&["trust"]).assert().success();
        fixture
            .git(&["add", ".ironlint.yml", "rule.txt"])
            .assert()
            .success();
        fixture
    }

    fn git(&self, args: &[&str]) -> Command {
        let mut command = Command::new("git");
        command
            .current_dir(&self.root)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("IRONLINT_BIN", assert_cmd::cargo::cargo_bin("ironlint"))
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
            ])
            .args(args);
        command
    }

    fn cli(&self, args: &[&str]) -> Command {
        let mut command = Command::cargo_bin("ironlint").unwrap();
        command
            .current_dir(&self.root)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg)
            .args(args);
        command
    }
}

#[test]
fn hook_rejects_staged_violation_hidden_by_unstaged_repair() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("rule.txt"), "bad\n").unwrap();
    fixture.git(&["add", "rule.txt"]).assert().success();
    fs::write(fixture.root.join("rule.txt"), "clean\n").unwrap();
    fixture
        .git(&["commit", "-qm", "must fail"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("staged and working"));
    fixture
        .git(&["rev-parse", "--verify", "HEAD"])
        .assert()
        .failure();
    fixture.git(&["show", ":rule.txt"]).assert().stdout("bad\n");
}

#[test]
fn hook_requires_policy_in_both_index_and_working_tree() {
    for remove_from_index in [false, true] {
        let fixture = Fixture::new();
        if remove_from_index {
            fixture
                .git(&["rm", "--cached", ".ironlint.yml"])
                .assert()
                .success();
        } else {
            fs::remove_file(fixture.root.join(".ironlint.yml")).unwrap();
        }
        fixture
            .git(&["commit", "-qm", "must fail"])
            .assert()
            .failure();
        fixture
            .git(&["rev-parse", "--verify", "HEAD"])
            .assert()
            .failure();
    }
}

#[test]
fn hook_allows_matching_clean_content_and_rejects_matching_bad_content() {
    let fixture = Fixture::new();
    fixture.git(&["commit", "-qm", "clean"]).assert().success();
    fixture
        .git(&["show", "HEAD:rule.txt"])
        .assert()
        .stdout("clean\n");
    fs::write(fixture.root.join("rule.txt"), "bad\n").unwrap();
    fixture.git(&["add", "rule.txt"]).assert().success();
    fixture.git(&["commit", "-qm", "bad"]).assert().failure();
    fixture
        .git(&["show", "HEAD:rule.txt"])
        .assert()
        .stdout("clean\n");
}

#[test]
fn hook_compares_raw_bytes_despite_index_flags_and_clean_filters() {
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("rule.txt"), "bad\n").unwrap();
        fixture.git(&["add", "rule.txt"]).assert().success();
        fixture
            .git(&["update-index", flag, "rule.txt"])
            .assert()
            .success();
        fs::write(fixture.root.join("rule.txt"), "clean\n").unwrap();
        fixture
            .git(&["commit", "-qm", "hidden mismatch"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("staged and working"));
    }
    let fixture = Fixture::new();
    fs::write(fixture.root.join(".gitattributes"), "rule.txt filter=bad\n").unwrap();
    fixture
        .git(&["config", "filter.bad.clean", "printf 'bad\\n'"])
        .assert()
        .success();
    fixture
        .git(&["add", ".gitattributes", "rule.txt"])
        .assert()
        .success();
    fixture.git(&["show", ":rule.txt"]).assert().stdout("bad\n");
    fixture
        .git(&["commit", "-qm", "filtered mismatch"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("staged and working"));
}

#[test]
fn hook_rejects_changes_made_during_acceptance() {
    let fixture = Fixture::new();
    fs::write(
        fixture.root.join(".ironlint.yml"),
        "version: 1\nchecks:\n  repair:\n    run: printf 'bad\\n' > rule.txt\n    on: [accept]\n",
    )
    .unwrap();
    fixture.cli(&["trust"]).assert().success();
    fixture.git(&["add", ".ironlint.yml"]).assert().success();
    fixture
        .git(&["commit", "-qm", "changed during check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("changed during acceptance"));
}

#[test]
fn hook_rejects_staged_deletions_still_present_on_disk() {
    let fixture = Fixture::new();
    fixture.git(&["commit", "-qm", "clean"]).assert().success();
    fixture
        .git(&["rm", "--cached", "rule.txt"])
        .assert()
        .success();
    fixture
        .git(&["commit", "-qm", "delete checked file"])
        .assert()
        .failure();
}

#[test]
fn hook_allows_fully_staged_file_to_directory_replacement() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("resource"), "old").unwrap();
    fixture.git(&["add", "resource"]).assert().success();
    fixture.git(&["commit", "-qm", "file"]).assert().success();
    fs::remove_file(fixture.root.join("resource")).unwrap();
    fs::create_dir(fixture.root.join("resource")).unwrap();
    fs::write(fixture.root.join("resource/new.txt"), "new").unwrap();
    fixture.git(&["add", "-A"]).assert().success();
    fixture
        .git(&["commit", "-qm", "directory"])
        .assert()
        .success();
}

#[cfg(unix)]
#[test]
fn hook_compares_symlink_targets_without_rejecting_matching_links() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    symlink("rule.txt", fixture.root.join("-link")).unwrap();
    symlink("target-with-newline\n", fixture.root.join("dangling")).unwrap();
    fixture
        .git(&["add", "--", "-link", "dangling"])
        .assert()
        .success();
    fixture
        .git(&["commit", "-qm", "matching links"])
        .assert()
        .success();
    fs::remove_file(fixture.root.join("-link")).unwrap();
    symlink(".ironlint.yml", fixture.root.join("-link")).unwrap();
    fixture
        .git(&["commit", "--allow-empty", "-qm", "unstaged link change"])
        .assert()
        .failure();
}

#[test]
fn hook_preserves_unusual_names_and_accepts_a_fully_staged_repair() {
    let fixture = Fixture::new();
    for name in [
        "space name",
        "tab\tname",
        "newline\nname",
        "-leading",
        "quote'$(false)",
    ] {
        fs::write(fixture.root.join(name), "contents").unwrap();
        fixture.git(&["add", "--", name]).assert().success();
    }
    fixture.git(&["commit", "-qm", "clean"]).assert().success();
    fs::write(fixture.root.join("rule.txt"), "bad\n").unwrap();
    fixture.git(&["add", "rule.txt"]).assert().success();
    fs::write(fixture.root.join("rule.txt"), "clean\n").unwrap();
    fixture
        .git(&["commit", "-qm", "unstaged repair"])
        .assert()
        .failure();
    fixture.git(&["add", "rule.txt"]).assert().success();
    fixture
        .git(&["commit", "--allow-empty", "-qm", "staged repair"])
        .assert()
        .success();
}
