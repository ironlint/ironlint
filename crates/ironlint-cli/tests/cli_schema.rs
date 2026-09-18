use assert_cmd::Command;

fn schema_stdout() -> String {
    let output = Command::cargo_bin("ironlint")
        .unwrap()
        .arg("schema")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(output).expect("schema output is utf8")
}

#[test]
fn schema_prints_v1_authoring_guide() {
    let guide = schema_stdout();
    assert!(guide.contains("version: 1"));
    assert!(guide.contains("IRONLINT_ROOT"));
    assert!(guide.contains("accept"));
    assert!(guide.contains("change"));
    assert!(guide.contains("with stdin closed"));
    assert!(!guide.contains("IRONLINT_TMPFILE"));
    assert!(!guide.contains("extends:"));
    assert!(!guide.starts_with("---"));
}
