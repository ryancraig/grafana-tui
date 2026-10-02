use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

/// Runs grafatui against a private config file so the user's own config never leaks in.
fn run_with_config(name: &str, config: &str, args: &[&str]) -> Output {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path: PathBuf = std::env::temp_dir().join(format!("grafatui-{name}-{stamp}.toml"));
    fs::write(&path, config).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
        .arg("--config")
        .arg(&path)
        .args(args)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    output
}

#[test]
fn list_themes_prints_every_flavor_and_marks_the_default() {
    let output = run_with_config("list-default", "", &["--list-themes"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let names: Vec<_> = stdout.lines().collect();
    assert_eq!(names.first(), Some(&"tokyo-night (current)"));
    for flavor in [
        "tokyo-night-storm",
        "tokyo-night-moon",
        "tokyo-night-day",
        "catppuccin-mocha",
        "catppuccin-macchiato",
        "catppuccin-frappe",
        "catppuccin-latte",
        "gruvbox-dark-hard",
        "gruvbox-light-soft",
        "terminal",
    ] {
        assert!(names.contains(&flavor), "missing {flavor}");
    }
}

#[test]
fn list_themes_marks_the_configured_theme_through_its_alias() {
    let output = run_with_config("list-alias", r#"theme = "catppuccin""#, &["--list-themes"]);

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout
            .lines()
            .any(|line| line == "catppuccin-mocha (current)")
    );
    assert_eq!(stdout.matches("(current)").count(), 1);
}

#[test]
fn unknown_theme_fails_before_starting_the_tui() {
    let output = run_with_config("unknown", "", &["--theme", "nope"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unknown theme `nope`"), "{stderr}");
    assert!(stderr.contains("catppuccin-latte"), "{stderr}");
}

#[test]
fn custom_themes_are_listed_and_selectable() {
    let config = r##"
        theme = "mine"

        [themes.mine]
        extends = "gruvbox-light"
        title = "#ff0000"

        [themes.tokyo-night]
        extends = "tokyo-night"
        grid = "dark-gray"
    "##;
    let output = run_with_config("custom-list", config, &["--list-themes"]);

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let names: Vec<_> = stdout.lines().collect();
    assert_eq!(names.last(), Some(&"mine (current)"));
    assert_eq!(
        names
            .iter()
            .filter(|name| name.starts_with("tokyo-night"))
            .count(),
        4
    );
}

#[test]
fn invalid_custom_theme_names_the_theme_and_key() {
    let config = r##"
        [themes.broken]
        palette = ["#ff0000", "nope"]
    "##;
    let output = run_with_config("custom-invalid", config, &["--list-themes"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("theme `broken`"), "{stderr}");
    assert!(stderr.contains("invalid color `nope`"), "{stderr}");
}

#[test]
fn validate_ignores_theme_configuration() {
    let dashboard = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("dashboards")
        .join("simple_test.json");
    let config = r##"
        theme = "nope"

        [themes.broken]
        palette = []
    "##;
    let output = run_with_config(
        "validate-themes",
        config,
        &["--validate", "--grafana-json", dashboard.to_str().unwrap()],
    );

    assert!(output.status.success(), "{output:?}");
}
