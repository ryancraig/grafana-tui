use std::process::Command;

#[test]
fn help_lists_annotations_file() {
    let output = Command::new(env!("CARGO_BIN_EXE_grafana-tui"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("--annotations-file <FILE>"));
}

#[test]
fn validate_does_not_read_annotations_file() {
    let dashboard = std::env::temp_dir().join(format!(
        "grafana-tui-annotations-validate-{}.json",
        std::process::id()
    ));
    std::fs::write(&dashboard, r#"{"title":"empty","panels":[]}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_grafana-tui"))
        .args([
            "--validate",
            "--grafana-json",
            dashboard.to_str().unwrap(),
            "--annotations-file",
            "does-not-exist.jsonl",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(dashboard).unwrap();

    assert!(output.status.success());
}
