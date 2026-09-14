use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

fn schema_stdout() -> String {
    let out = Command::cargo_bin("ironlint")
        .unwrap()
        .arg("schema")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(out).expect("schema output is utf8")
}

#[test]
fn schema_prints_the_authoring_guide() {
    let s = schema_stdout();
    assert!(
        s.contains("$IRONLINT_FILE"),
        "guide must mention $IRONLINT_FILE:\n{s}"
    );
    assert!(
        s.contains("nonzero"),
        "guide must mention the nonzero-blocks contract:\n{s}"
    );
    assert!(
        s.contains("$IRONLINT_TMPFILE"),
        "guide must mention $IRONLINT_TMPFILE:\n{s}"
    );
}

#[test]
fn schema_teaches_lifecycle_placement() {
    // W2-R3 (docs/architecture.md): the placement heuristic
    // section and its $IRONLINT_FILES rustfmt example are part of the
    // canonical guide.
    let s = schema_stdout();
    assert!(
        s.contains("Lifecycle placement"),
        "guide must carry the placement heuristic:\n{s}"
    );
    assert!(
        s.contains("xargs -0 rustfmt --check"),
        "guide must show the re-scoped $IRONLINT_FILES rustfmt example:\n{s}"
    );
}

#[test]
fn schema_output_has_no_yaml_frontmatter() {
    let s = schema_stdout();
    assert!(
        !s.starts_with("---"),
        "frontmatter must be stripped from `ironlint schema` output, got:\n{s}"
    );
}

#[test]
fn schema_v1_tree_example_is_executable_against_tree() {
    let guide = schema_stdout();
    let run = guide
        .lines()
        .find_map(|line| line.trim().strip_prefix("run: "))
        .filter(|run| run.contains("$IRONLINT_ROOT/src/example.txt"))
        .expect("v1 guide must include a tree-reading example");

    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/example.txt"), "DEBUG\n").unwrap();
    let cfg = dir.path().join("policy.yml");
    fs::write(
        &cfg,
        format!("version: 1\nchecks:\n  no-debug:\n    run: {run}\n"),
    )
    .unwrap();
    let xdg = tempdir().unwrap();

    Command::cargo_bin("ironlint")
        .unwrap()
        .args(["trust", "--config", cfg.to_str().unwrap()])
        .env("XDG_CONFIG_HOME", xdg.path())
        .assert()
        .success();

    Command::cargo_bin("ironlint")
        .unwrap()
        .args([
            "check",
            "--config",
            cfg.to_str().unwrap(),
            "--root",
            dir.path().to_str().unwrap(),
        ])
        .env("XDG_CONFIG_HOME", xdg.path())
        .assert()
        .code(2);
}
