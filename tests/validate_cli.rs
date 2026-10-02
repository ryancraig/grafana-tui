use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn write_dashboard(name: &str, json: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("grafatui-{name}-{stamp}.json"));
    fs::write(&path, json).unwrap();
    path
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("grafana")
        .join(name)
}

fn example_dashboard(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("dashboards")
        .join(name)
}

fn demo_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("demo")
        .join(name)
}

#[test]
fn validate_strict_exits_nonzero_when_warnings_exist() {
    let path = write_dashboard(
        "strict",
        r#"{
            "title": "Warnings",
            "panels": [
                { "type": "text", "title": "Notes" }
            ]
        }"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
        .args(["--validate", "--strict", "--grafana-json"])
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("warning[grafana.import.skipped_panel]"));
    assert!(stderr.contains("validation failed with 1 warning(s)"));
}

#[test]
fn validate_strict_accepts_classic_transformations_without_warnings() {
    let path = write_dashboard(
        "classic-transformations",
        r#"{
            "title": "Classic transformations",
            "panels": [{
                "type": "timeseries",
                "title": "CPU",
                "targets": [{"expr": "up"}],
                "transformations": [{"id": "reduce"}]
            }]
        }"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
        .args([
            "--validate",
            "--strict",
            "--format",
            "json",
            "--grafana-json",
        ])
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["panel_count"], 1);
    assert_eq!(summary["diagnostics"], serde_json::json!([]));
}

#[test]
fn validate_json_outputs_machine_readable_summary() {
    let path = write_dashboard(
        "json",
        r#"{
            "title": "JSON Warnings",
            "panels": [
                { "type": "text", "title": "Notes" },
                {
                    "type": "timeseries",
                    "title": "CPU",
                    "targets": [
                        { "expr": "helper_query", "hide": true },
                        { "expr": "visible_query" }
                    ]
                }
            ]
        }"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
        .args(["--validate", "--format", "json", "--grafana-json"])
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let summary: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(summary["title"], "JSON Warnings");
    assert_eq!(summary["panel_count"], 1);
    assert_eq!(summary["diagnostics"][0]["code"], "skipped_panel");
    assert_eq!(summary["diagnostics"].as_array().unwrap().len(), 1);
}

#[test]
fn validate_accepts_supported_v2_resource_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
        .args(["--validate", "--format", "json", "--grafana-json"])
        .arg(fixture("v2_compatibility.json"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["title"], "Compatibility");
    assert_eq!(summary["panel_count"], 1);
    assert_eq!(summary["diagnostics"], serde_json::json!([]));
}

#[test]
fn validate_accepts_grafana13_server_serialized_exports() {
    for name in [
        "v2_grafana13_export.json",
        "v2_grafana13_external_export.json",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
            .args(["--validate", "--format", "json", "--grafana-json"])
            .arg(fixture(name))
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(summary["title"], "Grafatui native V2", "{name}");
        assert_eq!(summary["panel_count"], 2, "{name}");
        assert_eq!(summary["diagnostics"][0]["code"], "skipped_panel", "{name}");
    }
}

#[test]
fn validate_accepts_grafana13_yaml_export() {
    let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
        .args(["--validate", "--format", "json", "--grafana-dashboard"])
        .arg(fixture("v2_grafana13_export.yaml"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["title"], "Grafatui native V2");
    assert_eq!(summary["panel_count"], 2);
}

#[test]
fn validate_strict_accepts_auto_grid_example_and_grafana13_fixture() {
    for path in [
        example_dashboard("grafana_v2_autogrid.json"),
        fixture("v2_grafana13_autogrid.json"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
            .args(["--validate", "--strict", "--grafana-json"])
            .arg(&path)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn validate_strict_accepts_repeats_example_and_grafana13_fixture() {
    for path in [
        example_dashboard("grafana_v2_repeats.yaml"),
        fixture("v2_grafana13_repeats.json"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
            .args(["--validate", "--strict", "--grafana-json"])
            .arg(&path)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn validate_strict_accepts_conditional_example_and_grafana13_fixture() {
    for path in [
        example_dashboard("grafana_v2_conditional.json"),
        fixture("v2_grafana13_conditional.json"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
            .args(["--validate", "--strict", "--grafana-json"])
            .arg(&path)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn validate_strict_accepts_sections_example_and_grafana13_fixture() {
    for path in [
        example_dashboard("grafana_v2_sections.json"),
        fixture("v2_grafana13_sections.json"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
            .args(["--validate", "--strict", "--grafana-json"])
            .arg(&path)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn validate_accepts_live_grafana_v2_compatibility_example() {
    let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
        .args(["--validate", "--format", "json", "--grafana-json"])
        .arg(example_dashboard("grafana_v2_compatibility.json"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["title"], "Grafana V2 Compatibility");
    assert_eq!(summary["panel_count"], 2);
    assert_eq!(summary["diagnostics"], serde_json::json!([]));
}

#[test]
fn validate_accepts_live_grafana_v2_rows_example() {
    let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
        .args(["--validate", "--format", "json", "--grafana-json"])
        .arg(example_dashboard("grafana_v2_rows.json"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["title"], "Grafana V2 Rows");
    assert_eq!(summary["panel_count"], 3);
}

#[test]
fn validate_accepts_v2_rows_layout() {
    let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
        .args(["--validate", "--format", "json", "--grafana-json"])
        .arg(fixture("v2_rows_layout.json"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["title"], "Rows layout");
    assert_eq!(summary["panel_count"], 3);
}

#[test]
fn validate_accepts_v2_tabs_layout() {
    let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
        .args(["--validate", "--format", "json", "--grafana-json"])
        .arg(fixture("v2_tabs_layout.json"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["title"], "Tabs layout");
    assert_eq!(summary["panel_count"], 2);
}

#[test]
fn validate_accepts_live_grafana_v2_tabs_example() {
    let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
        .args(["--validate", "--format", "json", "--grafana-json"])
        .arg(example_dashboard("grafana_v2_tabs.json"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["title"], "Grafana V2 Tabs");
    assert_eq!(summary["panel_count"], 2);
}

#[test]
fn validate_accepts_nested_v2_tabs_layout() {
    let mut value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(fixture("v2_rows_layout.json")).unwrap()).unwrap();
    value["spec"]["layout"]["spec"]["rows"][0]["spec"]["layout"] = serde_json::json!({
        "kind": "TabsLayout",
        "spec": {"tabs": [{
            "kind": "TabsLayoutTab",
            "spec": {
                "title": "Empty",
                "layout": {"kind": "GridLayout", "spec": {"items": []}}
            }
        }]}
    });
    let path = write_dashboard("v2-nested-tabs", &value.to_string());

    let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
        .args(["--validate", "--grafana-json"])
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn validate_strict_rejects_v2_unsupported_datasource_warning() {
    let mut value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(fixture("v2_compatibility.json")).unwrap())
            .unwrap();
    value["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["queries"][1]["spec"]["query"]["group"] =
        "loki".into();
    let path = write_dashboard("v2-strict", &value.to_string());

    let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
        .args(["--validate", "--strict", "--grafana-json"])
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("warning[grafana.import.unsupported_datasource]"));
    assert!(stderr.contains("validation failed with 1 warning(s)"));
}

#[test]
fn validate_strict_accepts_every_hashistack_rdw_dashboard() {
    let mut paths: Vec<PathBuf> = fs::read_dir(demo_dir("hashistack-rdw"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 8, "{paths:?}");

    for path in paths {
        let output = Command::new(env!("CARGO_BIN_EXE_grafatui"))
            .args([
                "--validate",
                "--strict",
                "--format",
                "json",
                "--grafana-json",
            ])
            .arg(&path)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            summary["title"]
                .as_str()
                .unwrap()
                .starts_with("HashiStack / "),
            "{}",
            path.display()
        );
        assert!(
            summary["panel_count"].as_u64().unwrap() >= 20,
            "{}",
            path.display()
        );
        assert_eq!(
            summary["diagnostics"],
            serde_json::json!([]),
            "{}",
            path.display()
        );
    }
}
