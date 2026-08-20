use assert_cmd::Command;

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
    // W2-R3 (specs/2026-08-17-...-design.md): the placement heuristic
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
