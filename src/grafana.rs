/*
 * Copyright 2026 Federico D'Ambrosio
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

mod classic;
mod import;
mod model;
mod v2;

/// Result of importing a Grafana dashboard.
#[derive(Debug, Clone, Default)]
pub(crate) struct DashboardImport {
    /// Dashboard title.
    pub(crate) title: String,
    /// List of panels extracted.
    pub(crate) queries: Vec<QueryPanel>,
    /// Recursive layout of panels and rows.
    pub(crate) layout: crate::dashboard::DashboardLayout,
    /// Variables extracted from `templating.list`, formatted for interpolation.
    pub(crate) vars: HashMap<String, String>,
    /// Raw selected values per variable, before formatting into `vars`; `All`
    /// selects every known option. Repeats iterate these and titles show them.
    pub(crate) var_values: HashMap<String, Vec<String>>,
    /// Multi-value and include-all variables, whose values are regex-escaped.
    pub(crate) regex_vars: HashSet<String>,
    /// Variables with `All` selected.
    pub(crate) all_vars: HashSet<String>,
    /// Names of every variable the dashboard defines.
    pub(crate) variable_names: HashSet<String>,
    /// Conditional rendering of rows, tabs, and auto grid items in `layout`.
    pub(crate) conditions: crate::conditions::Conditions,
    /// Variables that rows and tabs in `layout` define for their contents.
    pub(crate) sections: HashMap<crate::dashboard::SectionId, Vec<SectionVariable>>,
    /// Repeat settings for panels, rows, and tabs in `layout`.
    pub(crate) repeats: crate::dashboard::Repeats,
    /// Dynamic query variables extracted from `templating.list`.
    pub(crate) query_vars: Vec<TemplateQueryVar>,
    /// Number of panels that were skipped (unsupported types).
    pub(crate) skipped_panels: usize,
    /// Dashboard-level refresh interval in milliseconds, if provided.
    pub(crate) refresh_rate_ms: Option<u64>,
    /// Warnings produced while importing the dashboard.
    pub(crate) diagnostics: Vec<ImportDiagnostic>,
}

/// A warning produced while importing a Grafana dashboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ImportDiagnostic {
    /// Stable diagnostic code.
    pub(crate) code: String,
    /// JSON-ish source path for the warning.
    pub(crate) path: String,
    /// Human-readable diagnostic message.
    pub(crate) message: String,
}

impl ImportDiagnostic {
    fn new(code: &str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            path: path.into(),
            message: message.into(),
        }
    }
}

/// A Prometheus-backed Grafana template variable.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TemplateQueryVar {
    /// Variable name used in PromQL expressions.
    pub(crate) name: String,
    /// Prometheus variable query expression.
    pub(crate) query: String,
    /// Optional Grafana regex extractor.
    pub(crate) regex: Option<String>,
    /// JSON-ish source path for the variable query.
    pub(crate) query_path: String,
    /// Whether `All` is selected, so every resolved value is used.
    pub(crate) select_all: bool,
    /// Replaces the resolved values when `All` is selected, if set.
    pub(crate) all_value: Option<String>,
    /// Whether values are regex-escaped, as for multi-value or include-all variables.
    pub(crate) regex_values: bool,
    /// When the query runs again after the dashboard loads.
    pub(crate) refresh: VariableRefresh,
}

/// When a query variable resolves again after the dashboard loads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum VariableRefresh {
    /// Only on load, and when a variable its query references changes.
    #[default]
    OnLoad,
    /// Also whenever the time range changes.
    OnTimeRangeChange,
}

/// A variable that a V2 row or tab defines for itself and its contents,
/// shadowing a dashboard variable of the same name.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SectionVariable {
    pub(crate) name: String,
    /// Raw selected values; `All` selects every known option.
    pub(crate) values: Vec<String>,
    /// Whether values are regex-escaped, as for multi-value or include-all variables.
    pub(crate) regex: bool,
    /// Whether `All` is selected.
    pub(crate) all: bool,
    /// Replaces the values in queries while `All` is selected, if set.
    pub(crate) all_value: Option<String>,
    /// Prometheus query that resolves the values, for query variables.
    pub(crate) query: Option<TemplateQueryVar>,
}

/// A single panel extracted from Grafana.
#[derive(Debug, Clone)]
pub(crate) struct QueryPanel {
    pub(crate) title: String,
    pub(crate) exprs: Vec<String>,
    pub(crate) expr_paths: Vec<String>,      // Parallel to exprs
    pub(crate) legends: Vec<Option<String>>, // Parallel to exprs
    pub(crate) query_modes: Vec<crate::app::QueryMode>, // Parallel to exprs
    pub(crate) grid: Option<GridPos>,
    pub(crate) panel_type: crate::app::PanelType,
    pub(crate) thresholds: Option<crate::app::Thresholds>,
    pub(crate) min: Option<f64>,
    pub(crate) max: Option<f64>,
    pub(crate) autogrid: Option<bool>,
    pub(crate) display: crate::ui::DisplayFormat,
    pub(crate) options: crate::app::PanelOptions,
    pub(crate) resolution: crate::app::QueryResolution,
}

/// Grid position extracted from Grafana.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GridPos {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) w: i32,
    pub(crate) h: i32,
}

/// Serialization of a dashboard file, chosen from its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentFormat {
    Json,
    Yaml,
    /// Unknown extension: try JSON first, then YAML.
    Detect,
}

impl DocumentFormat {
    fn from_path(path: &std::path::Path) -> Self {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("json") => Self::Json,
            Some("yaml" | "yml") => Self::Yaml,
            _ => Self::Detect,
        }
    }
}

pub(crate) fn load_grafana_dashboard(path: &std::path::Path) -> Result<DashboardImport> {
    let data = std::fs::read_to_string(path)
        .with_context(|| format!("reading grafana dashboard: {}", path.display()))?;
    import_document(&data, DocumentFormat::from_path(path))
}

#[cfg(test)]
fn parse_grafana_dashboard(data: &str) -> Result<DashboardImport> {
    import_document(data, DocumentFormat::Detect)
}

/// Several dashboards imported together, one tab each in a tab group.
#[derive(Debug, Clone)]
pub(crate) struct DashboardSet {
    /// Every dashboard's panels and variables, under one tab group whose tabs
    /// are the dashboards, in order. Dashboard variables are the tabs' variables.
    pub(crate) combined: DashboardImport,
    /// The tab group holding one tab per dashboard.
    pub(crate) root: crate::dashboard::TabGroupId,
    /// Each dashboard imported alone, in order: its title, refresh rate,
    /// variables and diagnostics, exactly as a single dashboard.
    pub(crate) files: Vec<DashboardFile>,
}

/// One dashboard of a [`DashboardSet`].
#[derive(Debug, Clone)]
pub(crate) struct DashboardFile {
    pub(crate) path: std::path::PathBuf,
    pub(crate) single: DashboardImport,
}

/// Imports several dashboard files as the tabs of one tab group.
pub(crate) fn load_grafana_dashboards(paths: &[std::path::PathBuf]) -> Result<DashboardSet> {
    let documents = paths
        .iter()
        .map(|path| {
            let data = std::fs::read_to_string(path)
                .with_context(|| format!("reading grafana dashboard: {}", path.display()))?;
            let value = parse_document(&data, DocumentFormat::from_path(path))
                .with_context(|| path.display().to_string())?;
            Ok((path.clone(), value))
        })
        .collect::<Result<Vec<_>>>()?;
    import_documents(documents)
}

#[cfg(test)]
pub(crate) fn parse_grafana_dashboards(documents: &[&str]) -> Result<DashboardSet> {
    let documents = documents
        .iter()
        .enumerate()
        .map(|(index, data)| {
            Ok((
                std::path::PathBuf::from(format!("dashboard-{index}.json")),
                parse_document(data, DocumentFormat::Detect)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    import_documents(documents)
}

fn import_documents(documents: Vec<(std::path::PathBuf, Value)>) -> Result<DashboardSet> {
    let mut files = Vec::with_capacity(documents.len());
    let mut models = Vec::with_capacity(documents.len());
    for (path, value) in documents {
        let context = || path.display().to_string();
        let single = import::finish(detect_and_adapt(value.clone()).with_context(context)?)
            .with_context(context)?;
        models.push(detect_and_adapt(value).with_context(context)?);
        files.push(DashboardFile { path, single });
    }
    let (combined, root) = import::finish_many(models)?;
    Ok(DashboardSet {
        combined,
        root,
        files,
    })
}

fn import_document(data: &str, format: DocumentFormat) -> Result<DashboardImport> {
    import::finish(detect_and_adapt(parse_document(data, format)?)?)
}

/// Parses a dashboard document into JSON values.
///
/// Grafana exports Classic dashboards as JSON and V2 resources as JSON or YAML.
/// Both formats feed the same importer, so YAML is converted to `serde_json::Value`.
fn parse_document(data: &str, format: DocumentFormat) -> Result<Value> {
    match format {
        DocumentFormat::Json => {
            serde_json::from_str(data).context("parsing Grafana dashboard JSON")
        }
        DocumentFormat::Yaml => parse_yaml(data),
        DocumentFormat::Detect => serde_json::from_str(data).or_else(|json_error| {
            parse_yaml(data).map_err(|yaml_error| {
                anyhow::anyhow!(
                    "parsing Grafana dashboard: not valid JSON ({json_error}) or YAML ({yaml_error:#})"
                )
            })
        }),
    }
}

fn parse_yaml(data: &str) -> Result<Value> {
    let value: Value = serde_saphyr::from_str(data).context("parsing Grafana dashboard YAML")?;
    anyhow::ensure!(
        value.is_object(),
        "parsing Grafana dashboard YAML: expected a mapping at the document root"
    );
    Ok(value)
}

fn detect_and_adapt(value: Value) -> Result<model::Dashboard> {
    match value.get("apiVersion") {
        None => {
            if value.as_object().is_some_and(|object| {
                ["kind", "spec", "metadata", "status"]
                    .iter()
                    .any(|marker| object.contains_key(*marker))
            }) {
                anyhow::bail!(
                    "invalid Grafana dashboard resource at apiVersion: missing required field `apiVersion`"
                );
            }
            classic::adapt(value)
        }
        Some(Value::String(version)) if version == v2::V2_API_VERSION => v2::adapt(value),
        Some(Value::String(version)) => anyhow::bail!(
            "unsupported Grafana dashboard resource apiVersion `{version}` at apiVersion; supported resource version is `{}`",
            v2::V2_API_VERSION
        ),
        Some(_) => anyhow::bail!(
            "invalid Grafana dashboard resource apiVersion at apiVersion: expected a string"
        ),
    }
}

pub(crate) fn variable_diagnostics(
    dashboard: &DashboardImport,
    vars: &HashMap<String, String>,
) -> Vec<ImportDiagnostic> {
    let mut known_vars: HashSet<String> = vars.keys().cloned().collect();
    known_vars.extend(dashboard.query_vars.iter().map(|var| var.name.clone()));
    // Section variables only apply inside their row or tab, but panels are not
    // traced back to sections here, so any section's names count as known.
    let section_vars = dashboard.sections.values().flatten();
    known_vars.extend(section_vars.clone().map(|var| var.name.clone()));
    let section_queries = section_vars.filter_map(|var| var.query.as_ref());

    let mut diagnostics = Vec::new();
    let mut seen = HashSet::new();
    for panel in &dashboard.queries {
        for (expr, path) in panel.exprs.iter().zip(panel.expr_paths.iter()) {
            collect_variable_diagnostics(expr, path, &known_vars, &mut diagnostics, &mut seen);
        }
    }
    for query_var in dashboard.query_vars.iter().chain(section_queries) {
        collect_variable_diagnostics(
            &query_var.query,
            &query_var.query_path,
            &known_vars,
            &mut diagnostics,
            &mut seen,
        );
    }

    diagnostics
}

fn collect_variable_diagnostics(
    expr: &str,
    path: &str,
    known_vars: &HashSet<String>,
    diagnostics: &mut Vec<ImportDiagnostic>,
    seen: &mut HashSet<(String, String, String)>,
) {
    let chars: Vec<(usize, char)> = expr.char_indices().collect();
    let mut idx = 0;
    while idx < chars.len() {
        if chars[idx].1 != '$' {
            idx += 1;
            continue;
        }

        if idx + 1 >= chars.len() {
            idx += 1;
            continue;
        }

        if chars[idx + 1].1 == '{' {
            let start = chars[idx].0;
            let inner_start = chars[idx + 1].0 + 1;
            let mut end_idx = idx + 2;
            while end_idx < chars.len() && chars[end_idx].1 != '}' {
                end_idx += 1;
            }
            if end_idx >= chars.len() {
                idx += 1;
                continue;
            }

            let end = chars[end_idx].0;
            let token_end = end + 1;
            let inner = &expr[inner_start..end];
            let token = &expr[start..token_end];
            let (name, modifier) = inner.split_once(':').unwrap_or((inner, ""));
            if !modifier.is_empty() {
                push_variable_diagnostic(
                    diagnostics,
                    seen,
                    ImportDiagnostic::new(
                        "unsupported_variable_modifier",
                        path,
                        format!(
                            "unsupported Grafana variable modifier `{token}`; Grafatui expands only unmodified variables"
                        ),
                    ),
                );
            }
            if is_valid_variable_name(name)
                && !is_builtin_variable(name)
                && !known_vars.contains(name)
            {
                push_variable_diagnostic(
                    diagnostics,
                    seen,
                    ImportDiagnostic::new(
                        "unresolved_variable",
                        path,
                        format!(
                            "unresolved variable `{token}`; provide it with --var or dashboard templating"
                        ),
                    ),
                );
            }
            idx = end_idx + 1;
            continue;
        }

        let name_start = chars[idx + 1].0;
        let mut end_idx = idx + 1;
        while end_idx < chars.len() && is_variable_name_char(chars[end_idx].1) {
            end_idx += 1;
        }
        if end_idx == idx + 1 {
            idx += 1;
            continue;
        }

        let name_end = chars
            .get(end_idx)
            .map(|(byte_idx, _)| *byte_idx)
            .unwrap_or(expr.len());
        let name = &expr[name_start..name_end];
        if is_valid_variable_name(name) && !is_builtin_variable(name) && !known_vars.contains(name)
        {
            let token = &expr[chars[idx].0..name_end];
            push_variable_diagnostic(
                diagnostics,
                seen,
                ImportDiagnostic::new(
                    "unresolved_variable",
                    path,
                    format!(
                        "unresolved variable `{token}`; provide it with --var or dashboard templating"
                    ),
                ),
            );
        }
        idx = end_idx;
    }
}

fn push_variable_diagnostic(
    diagnostics: &mut Vec<ImportDiagnostic>,
    seen: &mut HashSet<(String, String, String)>,
    diagnostic: ImportDiagnostic,
) {
    let key = (
        diagnostic.code.clone(),
        diagnostic.path.clone(),
        diagnostic.message.clone(),
    );
    if seen.insert(key) {
        diagnostics.push(diagnostic);
    }
}

fn is_builtin_variable(name: &str) -> bool {
    matches!(
        name,
        "__interval"
            | "__interval_ms"
            | "__range"
            | "__range_s"
            | "__range_ms"
            | "__rate_interval"
            | "__rate_interval_ms"
    )
}

fn is_valid_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(is_variable_name_char)
}

fn is_variable_name_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_model_panel(kind: &str, expr: Option<&str>) -> model::LayoutNode {
        model::LayoutNode::Panel(model::Panel {
            repeat: None,
            condition: None,
            kind: kind.into(),
            title: kind.into(),
            source_path: format!("layout.{kind}"),
            targets: expr
                .into_iter()
                .map(|expr| model::Target {
                    expr: Some(expr.into()),
                    expr_path: format!("layout.{kind}.expr"),
                    legend_format: None,
                    instant: None,
                    hidden: false,
                    min_interval: None,
                })
                .collect(),
            count_as_skipped_if_empty: false,
            grid: None,
            field_defaults: None,
            reduce_options_path: None,
            transformations_path: None,
            min_interval: None,
            max_data_points: None,
        })
    }

    #[test]
    fn normalized_layout_references_only_imported_panel_indices() {
        let dashboard = model::Dashboard {
            title: "Rows".into(),
            layout: vec![model::LayoutNode::Row(model::Row {
                repeat: None,
                condition: None,
                variables: Vec::new(),
                title: "Group".into(),
                collapsed: false,
                hidden_header: false,
                source_path: "layout.row".into(),
                children: vec![
                    test_model_panel("piechart", None),
                    test_model_panel("timeseries", Some("up")),
                ],
            })],
            ..model::Dashboard::default()
        };
        let imported = import::finish(dashboard).unwrap();
        assert_eq!(imported.queries.len(), 1);
        assert_eq!(imported.layout.visible_panel_indices(), vec![0]);
        assert_eq!(imported.skipped_panels, 1);
    }

    fn minimal_v2_with_layout(layout: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "apiVersion": "dashboard.grafana.app/v2",
            "kind": "Dashboard",
            "metadata": {"name": "test"},
            "spec": {
                "title": "Test",
                "elements": {},
                "layout": layout,
                "variables": [],
                "timeSettings": {"from": "now-6h", "to": "now", "autoRefresh": ""}
            },
            "status": {}
        })
    }

    fn valid_v2_resource() -> serde_json::Value {
        serde_json::json!({
            "apiVersion": "dashboard.grafana.app/v2",
            "kind": "Dashboard",
            "spec": {
                "title": "Test",
                "elements": {
                    "panel-1": {
                        "kind": "Panel",
                        "spec": {
                            "id": 1,
                            "title": "Panel",
                            "links": [],
                            "data": {"kind": "QueryGroup", "spec": {"queries": [], "transformations": [], "queryOptions": {}}},
                            "vizConfig": {"kind": "VizConfig", "group": "timeseries", "version": "v0", "spec": {"fieldConfig": {"defaults": {}}, "options": {}}}
                        }
                    }
                },
                "layout": {"kind": "GridLayout", "spec": {"items": [{
                    "kind": "GridLayoutItem",
                    "spec": {"x": 0, "y": 0, "width": 1, "height": 1, "element": {"kind": "ElementReference", "name": "panel-1"}}
                }]}},
                "variables": [],
                "timeSettings": {"from": "now-6h", "to": "now", "autoRefresh": ""}
            }
        })
    }

    fn v2_row_resource_with_field(field: &str, value: serde_json::Value) -> serde_json::Value {
        let mut json = valid_v2_resource();
        let mut row_spec = serde_json::json!({
            "title": "Row",
            "layout": {"kind": "GridLayout", "spec": {"items": []}}
        });
        row_spec
            .as_object_mut()
            .unwrap()
            .insert(field.into(), value);
        json["spec"]["layout"] = serde_json::json!({
            "kind": "RowsLayout",
            "spec": {
                "rows": [{"kind": "RowsLayoutRow", "spec": row_spec}]
            }
        });
        json
    }

    fn make_v2_panel_importable(json: &mut serde_json::Value) {
        json["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["queries"] = serde_json::json!([{
            "kind": "PanelQuery",
            "spec": {
                "hidden": false,
                "refId": "A",
                "query": {
                    "kind": "DataQuery",
                    "group": "prometheus",
                    "spec": {"expr": "up"}
                }
            }
        }]);
    }

    #[test]
    fn v2_fixed_grid_panel_matches_classic_semantics() {
        let classic = parse_grafana_dashboard(include_str!(
            "../tests/fixtures/grafana/classic_compatibility.json"
        ))
        .unwrap();
        let v2 = parse_grafana_dashboard(include_str!(
            "../tests/fixtures/grafana/v2_compatibility.json"
        ))
        .unwrap();

        assert_eq!(v2.title, classic.title);
        assert_eq!(v2.refresh_rate_ms, classic.refresh_rate_ms);
        assert_eq!(v2.vars, classic.vars);
        assert_eq!(v2.query_vars.len(), 1);
        assert_eq!(v2.query_vars[0].name, classic.query_vars[0].name);
        assert_eq!(v2.query_vars[0].query, classic.query_vars[0].query);
        assert_eq!(v2.query_vars[0].regex, classic.query_vars[0].regex);
        assert_eq!(
            v2.query_vars[0].query_path,
            "spec.variables[0].spec.query.spec.query"
        );
        assert_eq!(v2.queries.len(), 1);
        let (actual, expected) = (&v2.queries[0], &classic.queries[0]);
        assert_eq!(actual.title, expected.title);
        assert_eq!(actual.exprs, expected.exprs);
        assert_eq!(actual.legends, expected.legends);
        assert_eq!(actual.query_modes, expected.query_modes);
        assert_eq!(actual.panel_type, expected.panel_type);
        assert_eq!(actual.display, expected.display);
        assert_eq!(actual.options, expected.options);
        let (actual_thresholds, expected_thresholds) = (
            actual.thresholds.as_ref().unwrap(),
            expected.thresholds.as_ref().unwrap(),
        );
        assert_eq!(actual_thresholds.mode, expected_thresholds.mode);
        assert_eq!(actual_thresholds.style, expected_thresholds.style);
        assert_eq!(
            actual_thresholds.steps.len(),
            expected_thresholds.steps.len()
        );
        for (actual_step, expected_step) in actual_thresholds
            .steps
            .iter()
            .zip(&expected_thresholds.steps)
        {
            assert_eq!(actual_step.value, expected_step.value);
            assert_eq!(actual_step.color, expected_step.color);
        }
        assert_eq!((actual.min, actual.max), (expected.min, expected.max));
        assert_eq!(actual.autogrid, expected.autogrid);
        assert_eq!(
            actual.grid.map(|grid| (grid.x, grid.y, grid.w, grid.h)),
            expected.grid.map(|grid| (grid.x, grid.y, grid.w, grid.h))
        );
        assert_eq!(
            actual.expr_paths,
            ["spec.elements[\"panel-1\"].spec.data.spec.queries[1].spec.query.spec.expr"]
        );
    }

    #[test]
    fn rejects_unsupported_resource_versions() {
        for version in [
            "dashboard.grafana.app/v1",
            "dashboard.grafana.app/v2alpha1",
            "dashboard.grafana.app/v2beta1",
        ] {
            let json = format!(r#"{{"apiVersion":"{version}","kind":"Dashboard","spec":{{}}}}"#);
            let error = parse_grafana_dashboard(&json).unwrap_err().to_string();
            assert!(error.contains(version));
            assert!(error.contains("apiVersion"));
        }
    }

    #[test]
    fn rejects_resource_envelope_without_api_version() {
        let error = parse_grafana_dashboard(
            r#"{"kind":"Dashboard","spec":{"title":"Incomplete resource"}}"#,
        )
        .expect_err("resource-shaped JSON without apiVersion must be rejected")
        .to_string();

        assert!(
            error.contains("apiVersion"),
            "expected missing apiVersion error, got `{error}`"
        );
    }

    #[test]
    fn rejects_unsupported_v2_layouts() {
        let json = minimal_v2_with_layout(serde_json::json!({"kind": "FutureLayout", "spec": {}}));
        let error = parse_grafana_dashboard(&json.to_string())
            .unwrap_err()
            .to_string();
        assert!(error.contains("FutureLayout"));
        assert!(error.contains("spec.layout.kind"));
    }

    fn auto_grid_item(name: &str) -> serde_json::Value {
        serde_json::json!({
            "kind": "AutoGridLayoutItem",
            "spec": {"element": {"kind": "ElementReference", "name": name}}
        })
    }

    /// Parses a dashboard whose root layout is an auto grid over `valid_v2_resource`'s
    /// panel, with `spec` merged into the auto grid spec.
    fn v2_auto_grid(spec: serde_json::Value) -> Result<DashboardImport> {
        let mut json = valid_v2_resource();
        make_v2_panel_importable(&mut json);
        let mut grid_spec = serde_json::json!({"items": [auto_grid_item("panel-1")]});
        grid_spec
            .as_object_mut()
            .unwrap()
            .extend(spec.as_object().unwrap().clone());
        json["spec"]["layout"] = serde_json::json!({"kind": "AutoGridLayout", "spec": grid_spec});
        parse_grafana_dashboard(&json.to_string())
    }

    fn only_auto_grid(dashboard: &DashboardImport) -> &crate::dashboard::DashboardAutoGrid {
        match dashboard.layout.items.as_slice() {
            [crate::dashboard::DashboardLayoutItem::AutoGrid(grid)] => grid,
            items => panic!("expected a single auto grid, got {items:?}"),
        }
    }

    /// `v2_grafana13_autogrid.json` was authored through Grafana 13.2.3's V2 API: two
    /// rows, each holding an auto grid, one with named and one with custom sizes.
    #[test]
    fn v2_grafana13_auto_grids_import_sizes_in_terminal_units() {
        let dashboard = parse_grafana_dashboard(include_str!(
            "../tests/fixtures/grafana/v2_grafana13_autogrid.json"
        ))
        .unwrap();

        let grids: Vec<_> = dashboard
            .layout
            .items
            .iter()
            .map(|item| match item {
                crate::dashboard::DashboardLayoutItem::Row(row) => match row.children.as_slice() {
                    [crate::dashboard::DashboardLayoutItem::AutoGrid(grid)] => {
                        (row.title.as_str(), grid.clone())
                    }
                    children => panic!("expected an auto grid, got {children:?}"),
                },
                item => panic!("expected a row, got {item:?}"),
            })
            .collect();
        assert_eq!(
            grids,
            [
                (
                    "Overview",
                    crate::dashboard::DashboardAutoGrid {
                        panels: vec![0, 1, 2, 3],
                        max_columns: 3,
                        // standard: 448px at 8px per cell
                        min_column_width: 56,
                        // short: 168px is closest to 5 grid units (5 * 30 + 4 * 8 = 182px)
                        row_height: 5,
                    }
                ),
                (
                    "Runtime",
                    crate::dashboard::DashboardAutoGrid {
                        panels: vec![4, 5],
                        max_columns: 4,
                        min_column_width: 30,
                        row_height: 11,
                    }
                ),
            ]
        );
        assert!(dashboard.queries.iter().all(|query| query.grid.is_none()));
        assert!(dashboard.diagnostics.is_empty());
    }

    #[test]
    fn v2_auto_grid_defaults_match_grafana() {
        let dashboard = v2_auto_grid(serde_json::json!({})).unwrap();
        let grid = only_auto_grid(&dashboard);

        assert_eq!(grid.panels, [0]);
        assert_eq!(grid.max_columns, 3);
        assert_eq!(grid.min_column_width, 56);
        assert_eq!(grid.row_height, 9);
    }

    #[test]
    fn v2_auto_grid_named_and_custom_sizes() {
        for (spec, min_column_width, row_height) in [
            (
                serde_json::json!({"columnWidthMode": "narrow", "rowHeightMode": "tall"}),
                24,
                14,
            ),
            (
                serde_json::json!({"columnWidthMode": "wide", "rowHeightMode": "short"}),
                96,
                5,
            ),
            (
                serde_json::json!({
                    "columnWidthMode": "custom",
                    "columnWidth": 100,
                    "rowHeightMode": "custom",
                    "rowHeight": 30
                }),
                13,
                1,
            ),
            // Grafana treats `custom` without a size, and unknown modes, as `standard`.
            (
                serde_json::json!({"columnWidthMode": "custom", "rowHeightMode": "huge"}),
                56,
                9,
            ),
            (
                serde_json::json!({"columnWidthMode": null, "rowHeightMode": null}),
                56,
                9,
            ),
        ] {
            let dashboard = v2_auto_grid(spec.clone()).unwrap();
            let grid = only_auto_grid(&dashboard);
            assert_eq!(
                (grid.min_column_width, grid.row_height),
                (min_column_width, row_height),
                "{spec}"
            );
        }
    }

    #[test]
    fn v2_auto_grid_max_column_count_is_clamped() {
        for (count, expected) in [(0, 1), (2, 2), (99, 24)] {
            let dashboard = v2_auto_grid(serde_json::json!({"maxColumnCount": count})).unwrap();
            assert_eq!(only_auto_grid(&dashboard).max_columns, expected, "{count}");
        }
    }

    #[test]
    fn v2_auto_grid_skips_unsupported_panels_and_empty_grids() {
        let mut json = valid_v2_resource();
        json["spec"]["elements"]["panel-1"]["spec"]["vizConfig"]["group"] = "text".into();
        json["spec"]["layout"] = serde_json::json!({
            "kind": "AutoGridLayout",
            "spec": {"items": [auto_grid_item("panel-1")]}
        });

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert!(dashboard.layout.items.is_empty());
        assert_eq!(dashboard.diagnostics.len(), 1);
        assert_eq!(dashboard.diagnostics[0].code, "skipped_panel");
    }

    #[test]
    fn v2_auto_grid_rejects_malformed_items_at_native_paths() {
        for (spec, expected) in [
            (
                serde_json::json!({"items": [{"kind": "GridLayoutItem", "spec": {}}]}),
                "spec.layout.spec.items[0].kind",
            ),
            (
                serde_json::json!({"items": [auto_grid_item("missing")]}),
                "spec.layout.spec.items[0].spec.element.name",
            ),
            (
                serde_json::json!({"maxColumnCount": "3"}),
                "spec.layout.spec.maxColumnCount",
            ),
            (
                serde_json::json!({"columnWidth": "wide"}),
                "spec.layout.spec.columnWidth",
            ),
        ] {
            let error = v2_auto_grid(spec.clone()).unwrap_err().to_string();
            assert!(error.contains(expected), "{spec}: {error}");
        }
    }

    /// A V2 dashboard with one panel querying `expr`, refreshing every
    /// `auto_refresh`, and an `instance` variable resolved by `instance_query`.
    fn v2_dashboard(title: &str, auto_refresh: &str, expr: &str, instance_query: &str) -> String {
        let mut json = valid_v2_resource();
        make_v2_panel_importable(&mut json);
        json["spec"]["title"] = serde_json::json!(title);
        json["spec"]["timeSettings"]["autoRefresh"] = serde_json::json!(auto_refresh);
        json["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["queries"][0]["spec"]["query"]
            ["spec"]["expr"] = serde_json::json!(expr);
        json["spec"]["variables"] = serde_json::json!([{
            "kind": "QueryVariable",
            "spec": {
                "name": "instance",
                "query": {"kind": "DataQuery", "group": "prometheus", "version": "v0",
                          "spec": {"query": instance_query}}
            }
        }]);
        json.to_string()
    }

    #[test]
    fn several_dashboards_import_as_one_tab_each() {
        let set = parse_grafana_dashboards(&[
            &v2_dashboard(
                "Nodes",
                "30s",
                "up{job=\"node\"}",
                "label_values(node_uname_info, instance)",
            ),
            &v2_dashboard(
                "Consul",
                "1m",
                "up{job=\"consul\"}",
                "label_values(consul_up, instance)",
            ),
        ])
        .unwrap();

        let root = set.root;
        let group = set.combined.layout.tabs(root).unwrap();
        assert_eq!(group.active, Some(0));
        let titles: Vec<&str> = group.tabs.iter().map(|tab| tab.title.as_str()).collect();
        assert_eq!(titles, ["Nodes", "Consul"]);
        // Panels are numbered across dashboards, so their indices do not collide.
        let exprs: Vec<&str> = set
            .combined
            .queries
            .iter()
            .map(|panel| panel.exprs[0].as_str())
            .collect();
        assert_eq!(exprs, ["up{job=\"node\"}", "up{job=\"consul\"}"]);
        assert_eq!(set.combined.layout.visible_panel_indices(), [0]);

        // Both define `instance`, each as its own tab's variable.
        assert!(set.combined.query_vars.is_empty());
        let instance_query = |tab| {
            set.combined.sections[&crate::dashboard::SectionId::Tab(root, tab)][0]
                .query
                .as_ref()
                .unwrap()
                .query
                .clone()
        };
        assert_eq!(instance_query(0), "label_values(node_uname_info, instance)");
        assert_eq!(instance_query(1), "label_values(consul_up, instance)");

        // Each file also imports alone, as a single dashboard would.
        let refresh: Vec<_> = set
            .files
            .iter()
            .map(|file| (file.single.title.as_str(), file.single.refresh_rate_ms))
            .collect();
        assert_eq!(refresh, [("Nodes", Some(30_000)), ("Consul", Some(60_000))]);
        assert_eq!(set.files[1].single.query_vars.len(), 1);
    }

    #[test]
    fn a_dashboard_with_tabs_nests_them_inside_its_own_tab() {
        let set = parse_grafana_dashboards(&[
            &v2_dashboard("Nodes", "30s", "up", "label_values(up, instance)"),
            include_str!("../tests/fixtures/grafana/v2_tabs_layout.json"),
        ])
        .unwrap();

        let group = set.combined.layout.tabs(set.root).unwrap();
        assert_eq!(group.tabs[1].title, "Tabs layout");
        let crate::dashboard::DashboardLayoutItem::Tabs(inner) = &group.tabs[1].children[0] else {
            panic!("expected the file's own tab group");
        };
        assert_ne!(inner.id, set.root);
        assert_eq!(set.combined.queries.len(), 3);
    }

    #[test]
    fn repeats_only_see_their_own_dashboards_variables() {
        // The second dashboard repeats over `instance`, which only the first defines.
        let mut repeating = valid_v2_resource();
        make_v2_panel_importable(&mut repeating);
        repeating["spec"]["layout"]["spec"]["items"][0]["spec"]["repeat"] =
            serde_json::json!({"mode": "variable", "value": "instance"});

        let set = parse_grafana_dashboards(&[
            &v2_dashboard("Nodes", "30s", "up", "label_values(up, instance)"),
            &repeating.to_string(),
        ])
        .unwrap();

        assert!(set.combined.repeats.is_empty());
        assert_eq!(
            set.files[1].single.diagnostics[0].code,
            "unknown_repeat_variable"
        );
    }

    #[test]
    fn hashistack_dashboards_load_together() {
        let mut paths: Vec<_> = std::fs::read_dir("examples/demo/hashistack-rdw")
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .collect();
        paths.sort();

        let set = load_grafana_dashboards(&paths).unwrap();

        assert_eq!(set.files.len(), 8);
        let group = set.combined.layout.tabs(set.root).unwrap();
        assert_eq!(group.tabs.len(), 8);
        for (index, file) in set.files.iter().enumerate() {
            assert_eq!(group.tabs[index].title, file.single.title);
            assert_eq!(
                file.single.refresh_rate_ms,
                Some(30_000),
                "{}",
                file.path.display()
            );
            let names: Vec<&str> = set.combined.sections
                [&crate::dashboard::SectionId::Tab(set.root, index)]
                .iter()
                .map(|variable| variable.name.as_str())
                .collect();
            assert!(names.contains(&"datasource"), "{names:?}");
        }
        let panels: usize = set.files.iter().map(|file| file.single.queries.len()).sum();
        assert_eq!(set.combined.queries.len(), panels);
    }

    #[test]
    fn v2_tabs_preserves_empty_tab_and_first_selection() {
        let value = minimal_v2_with_layout(serde_json::json!({
            "kind": "TabsLayout",
            "spec": {"tabs": [{
                "kind": "TabsLayoutTab",
                "spec": {
                    "title": "Empty",
                    "layout": {"kind": "GridLayout", "spec": {"items": []}}
                }
            }]}
        }));

        let imported = parse_grafana_dashboard(&value.to_string()).unwrap();
        let group = imported
            .layout
            .tabs(crate::dashboard::TabGroupId::new(0))
            .unwrap();
        assert_eq!(group.active, Some(0));
        assert_eq!(group.tabs[0].title, "Empty");
        assert!(group.tabs[0].children.is_empty());
    }

    #[test]
    fn v2_tabs_fixture_imports_inactive_and_nested_panels() {
        let mut dashboard = parse_grafana_dashboard(include_str!(
            "../tests/fixtures/grafana/v2_tabs_layout.json"
        ))
        .unwrap();

        assert_eq!(dashboard.queries.len(), 2);
        assert_eq!(dashboard.layout.visible_panel_indices(), vec![0]);
        dashboard
            .layout
            .set_active_tab(crate::dashboard::TabGroupId::new(0), 1)
            .unwrap();
        assert_eq!(dashboard.layout.visible_panel_indices(), vec![1]);
        assert!(
            dashboard
                .layout
                .tabs(crate::dashboard::TabGroupId::new(1))
                .is_some()
        );
    }

    /// `v2_grafana13_sections.json` was authored through Grafana 13.2.3's V2 API:
    /// a row shadowing the dashboard's `quantile`, and a row whose `handler`
    /// query variable drives a repeat.
    #[test]
    fn v2_grafana13_section_variables_fixture_imports_per_row() {
        use crate::dashboard::{RowId, SectionId};

        let dashboard = parse_grafana_dashboard(include_str!(
            "../tests/fixtures/grafana/v2_grafana13_sections.json"
        ))
        .unwrap();

        assert!(
            dashboard.diagnostics.is_empty(),
            "{:?}",
            dashboard.diagnostics
        );
        assert_eq!(
            dashboard.vars.get("quantile").map(String::as_str),
            Some("0.9")
        );
        let section = |row| &dashboard.sections[&SectionId::Row(RowId::new(row))];
        assert!(
            !dashboard
                .sections
                .contains_key(&SectionId::Row(RowId::new(0)))
        );
        assert_eq!(section(1)[0].name, "quantile");
        assert_eq!(section(1)[0].values, ["0.99"]);
        let handler = &section(2)[0];
        assert!(handler.all && handler.regex);
        assert_eq!(
            handler.query.as_ref().map(|query| query.query.as_str()),
            Some("label_values(prometheus_http_requests_total, handler)")
        );
        assert_eq!(
            dashboard
                .repeats
                .panels
                .get(&2)
                .map(|repeat| repeat.variable.as_str()),
            Some("handler")
        );
    }

    #[test]
    fn v2_tabs_reject_malformed_section_variables_at_native_paths() {
        let spec = serde_json::json!({
            "title": "Tab",
            "layout": {"kind": "GridLayout", "spec": {"items": []}},
            "variables": [{"kind": "TextVariable"}]
        });
        let dashboard = minimal_v2_with_layout(serde_json::json!({
            "kind": "TabsLayout",
            "spec": {"tabs": [{"kind": "TabsLayoutTab", "spec": spec}]}
        }));
        let error = parse_grafana_dashboard(&dashboard.to_string())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("spec.layout.spec.tabs[0].spec.variables[0].spec"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn v2_rows_import_expanded_collapsed_and_nested_content() {
        let dashboard = parse_grafana_dashboard(include_str!(
            "../tests/fixtures/grafana/v2_rows_layout.json"
        ))
        .unwrap();

        assert_eq!(dashboard.queries.len(), 3);
        assert_eq!(dashboard.layout.visible_panel_indices(), vec![0]);
    }

    #[test]
    fn classic_rows_group_expanded_siblings_and_collapsed_nested_panels() {
        let dashboard =
            parse_grafana_dashboard(include_str!("../tests/fixtures/grafana/classic_rows.json"))
                .unwrap();

        assert_eq!(dashboard.queries.len(), 2);
        assert_eq!(dashboard.layout.visible_panel_indices(), vec![0]);
        let visible = dashboard.layout.visible_items();
        assert_eq!(
            visible[0].id,
            crate::dashboard::DashboardItemId::Row(crate::dashboard::RowId::new(0))
        );
        assert_eq!(
            visible[2].id,
            crate::dashboard::DashboardItemId::Row(crate::dashboard::RowId::new(1))
        );
    }

    #[test]
    fn classic_rows_make_child_grid_y_relative_to_each_row() {
        let dashboard = parse_grafana_dashboard(
            r#"{
                "panels": [{
                    "type": "row", "title": "Outer", "collapsed": true,
                    "gridPos": {"x": 0, "y": 10, "w": 24, "h": 2},
                    "panels": [
                        {
                            "type": "timeseries", "title": "Direct child",
                            "gridPos": {"x": 0, "y": 13, "w": 24, "h": 8},
                            "targets": [{"expr": "up"}]
                        },
                        {
                            "type": "row", "title": "Nested", "collapsed": true,
                            "gridPos": {"x": 0, "y": 20, "w": 24, "h": 3},
                            "panels": [{
                                "type": "stat", "title": "Nested child",
                                "gridPos": {"x": 0, "y": 25, "w": 24, "h": 6},
                                "targets": [{"expr": "sum(up)"}]
                            }]
                        }
                    ]
                }]
            }"#,
        )
        .unwrap();

        assert_eq!(dashboard.queries[0].grid.unwrap().y, 1);
        assert_eq!(dashboard.queries[1].grid.unwrap().y, 2);
    }

    #[test]
    fn v2_rows_hidden_header_is_transparent_even_when_collapsed() {
        let mut json = valid_v2_resource();
        make_v2_panel_importable(&mut json);
        let grid = json["spec"]["layout"].clone();
        json["spec"]["layout"] = serde_json::json!({
            "kind": "RowsLayout",
            "spec": {"rows": [{
                "kind": "RowsLayoutRow",
                "spec": {
                    "title": "Hidden",
                    "collapse": true,
                    "hideHeader": true,
                    "layout": grid
                }
            }]}
        });

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert_eq!(dashboard.queries.len(), 1);
        assert_eq!(dashboard.layout.visible_panel_indices(), vec![0]);
        assert_eq!(dashboard.layout.visible_items().len(), 1);
    }

    #[test]
    fn v2_row_variables_import_as_section_variables() {
        let mut json = v2_row_resource_with_field(
            "variables",
            serde_json::json!([
                {"kind": "CustomVariable", "spec": {
                    "name": "host",
                    "query": "a,b",
                    "multi": true,
                    "includeAll": true,
                    "allValue": ".+",
                    "current": {"text": ["All"], "value": ["$__all"]}
                }},
                {"kind": "QueryVariable", "spec": {
                    "name": "pod",
                    "current": {"text": "p1", "value": "p1"},
                    "query": {"kind": "DataQuery", "group": "prometheus", "spec": {"query": "label_values(up{host=~\"$host\"}, pod)"}}
                }},
                {"kind": "AdhocVariable", "spec": {"name": "filters"}}
            ]),
        );
        json["spec"]["layout"]["spec"]["rows"][0]["spec"]["repeat"] =
            serde_json::json!({"mode": "variable", "value": "host"});

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        let section =
            &dashboard.sections[&crate::dashboard::SectionId::Row(crate::dashboard::RowId::new(0))];
        assert_eq!(section.len(), 2);
        assert_eq!(section[0].name, "host");
        assert_eq!(section[0].values, ["a", "b"]);
        assert!(section[0].all && section[0].regex);
        assert_eq!(section[0].all_value.as_deref(), Some(".+"));
        assert_eq!(section[1].name, "pod");
        assert_eq!(section[1].values, ["p1"]);
        let query = section[1].query.as_ref().unwrap();
        assert_eq!(query.query, "label_values(up{host=~\"$host\"}, pod)");
        assert_eq!(
            query.query_path,
            "spec.layout.spec.rows[0].spec.variables[1].spec.query.spec.query"
        );
        // Section variables stay out of the dashboard's own variables.
        assert!(dashboard.vars.is_empty() && dashboard.query_vars.is_empty());
        // The row may repeat over its own variable.
        assert_eq!(
            dashboard.repeats.rows.get(&crate::dashboard::RowId::new(0)),
            Some(&crate::dashboard::Repeat::new("host"))
        );
        assert_eq!(dashboard.diagnostics.len(), 1);
        assert_eq!(dashboard.diagnostics[0].code, "unsupported_variable");
        assert_eq!(
            dashboard.diagnostics[0].path,
            "spec.layout.spec.rows[0].spec.variables[2]"
        );
    }

    #[test]
    fn v2_rows_default_absent_title_to_empty_string() {
        let mut json = valid_v2_resource();
        let grid = json["spec"]["layout"].clone();
        json["spec"]["layout"] = serde_json::json!({
            "kind": "RowsLayout",
            "spec": {"rows": [{
                "kind": "RowsLayoutRow",
                "spec": {"layout": grid}
            }]}
        });

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert_eq!(
            dashboard
                .layout
                .row(crate::dashboard::RowId::new(0))
                .unwrap()
                .title,
            ""
        );
    }

    #[test]
    fn v2_rows_default_absent_collapse_and_hide_header_to_false() {
        let mut json = valid_v2_resource();
        make_v2_panel_importable(&mut json);
        let grid = json["spec"]["layout"].clone();
        json["spec"]["layout"] = serde_json::json!({
            "kind": "RowsLayout",
            "spec": {"rows": [{
                "kind": "RowsLayoutRow",
                "spec": {"title": "Defaults", "layout": grid}
            }]}
        });

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();
        let row = dashboard
            .layout
            .row(crate::dashboard::RowId::new(0))
            .unwrap();

        assert!(!row.collapsed);
        assert!(!row.hidden_header);
        assert_eq!(dashboard.layout.visible_panel_indices(), vec![0]);
        assert_eq!(
            dashboard
                .layout
                .visible_items()
                .into_iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            vec![
                crate::dashboard::DashboardItemId::Row(crate::dashboard::RowId::new(0)),
                crate::dashboard::DashboardItemId::Panel(0),
            ]
        );
    }

    #[test]
    fn v2_rows_accept_empty_deferred_fields() {
        for (field, value) in [
            ("variables", serde_json::json!([])),
            ("variables", serde_json::Value::Null),
            ("fillScreen", serde_json::json!(false)),
            ("fillScreen", serde_json::json!(true)),
            ("title", serde_json::Value::Null),
        ] {
            let dashboard =
                parse_grafana_dashboard(&v2_row_resource_with_field(field, value).to_string())
                    .unwrap();
            assert!(dashboard.layout.visible_panel_indices().is_empty());
        }
    }

    #[test]
    fn v2_rows_reject_malformed_rows_at_native_paths() {
        let cases = [
            (
                serde_json::json!({"kind": "RowsLayout", "spec": {"rows": {}}}),
                "spec.layout.spec.rows",
            ),
            (
                serde_json::json!({"kind": "RowsLayout", "spec": {"rows": [null]}}),
                "spec.layout.spec.rows[0]",
            ),
            (
                serde_json::json!({"kind": "RowsLayout", "spec": {"rows": [{"kind": "GridLayoutItem", "spec": {}}]}}),
                "spec.layout.spec.rows[0].kind",
            ),
            (
                serde_json::json!({"kind": "RowsLayout", "spec": {"rows": [{"kind": "RowsLayoutRow", "spec": []}]}}),
                "spec.layout.spec.rows[0].spec",
            ),
            (
                serde_json::json!({"kind": "RowsLayout", "spec": {"rows": [{"kind": "RowsLayoutRow", "spec": {"title": 7, "layout": {"kind": "GridLayout", "spec": {"items": []}}}}]}}),
                "spec.layout.spec.rows[0].spec.title",
            ),
            (
                serde_json::json!({"kind": "RowsLayout", "spec": {"rows": [{"kind": "RowsLayoutRow", "spec": {"title": "Row", "layout": []}}]}}),
                "spec.layout.spec.rows[0].spec.layout",
            ),
        ];

        for (layout, expected_path) in cases {
            let json = minimal_v2_with_layout(layout);
            let error = parse_grafana_dashboard(&json.to_string())
                .unwrap_err()
                .to_string();
            assert!(
                error.contains(expected_path),
                "expected {expected_path}, got {error}"
            );
        }
    }

    #[test]
    fn v2_rows_reject_non_boolean_options_at_native_paths() {
        for field in ["collapse", "hideHeader", "fillScreen"] {
            let error = parse_grafana_dashboard(
                &v2_row_resource_with_field(field, serde_json::json!("false")).to_string(),
            )
            .unwrap_err()
            .to_string();
            assert!(
                error.contains(&format!("spec.layout.spec.rows[0].spec.{field}")),
                "unexpected error for {field}: {error}"
            );
        }
    }

    #[test]
    fn v2_rows_reject_nested_unsupported_layouts_at_native_paths() {
        let json = v2_row_resource_with_field(
            "layout",
            serde_json::json!({"kind": "FutureLayout", "spec": {}}),
        );
        let error = parse_grafana_dashboard(&json.to_string())
            .unwrap_err()
            .to_string();
        assert!(error.contains("FutureLayout"));
        assert!(
            error.contains("spec.layout.spec.rows[0].spec.layout.kind"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_invalid_v2_resource_kind() {
        for kind in [None, Some("Folder")] {
            let mut json = valid_v2_resource();
            if let Some(kind) = kind {
                json["kind"] = serde_json::json!(kind);
            } else {
                json.as_object_mut().unwrap().remove("kind");
            }
            assert!(
                parse_grafana_dashboard(&json.to_string())
                    .unwrap_err()
                    .to_string()
                    .contains("kind")
            );
        }
    }

    #[test]
    fn rejects_missing_or_non_object_v2_spec() {
        for spec in [None, Some(serde_json::json!([]))] {
            let mut json = valid_v2_resource();
            if let Some(spec) = spec {
                json["spec"] = spec;
            } else {
                json.as_object_mut().unwrap().remove("spec");
            }
            assert!(
                parse_grafana_dashboard(&json.to_string())
                    .unwrap_err()
                    .to_string()
                    .contains("spec")
            );
        }
    }

    #[test]
    fn rejects_malformed_v2_time_settings_fields() {
        let mut non_object_settings = valid_v2_resource();
        non_object_settings["spec"]["timeSettings"] = serde_json::json!("30s");
        let error = parse_grafana_dashboard(&non_object_settings.to_string())
            .unwrap_err()
            .to_string();
        assert!(error.contains("spec.timeSettings"));

        let mut non_string_refresh = valid_v2_resource();
        non_string_refresh["spec"]["timeSettings"]["autoRefresh"] = serde_json::json!(30);
        let error = parse_grafana_dashboard(&non_string_refresh.to_string())
            .unwrap_err()
            .to_string();
        assert!(error.contains("spec.timeSettings.autoRefresh"));
    }

    #[test]
    fn rejects_invalid_v2_nested_kinds() {
        let cases = [
            (
                "data",
                "spec.elements[\"panel-1\"].spec.data.kind",
                "NotQueryGroup",
            ),
            (
                "query",
                "spec.elements[\"panel-1\"].spec.data.spec.queries[0].kind",
                "NotPanelQuery",
            ),
            (
                "data_query",
                "spec.elements[\"panel-1\"].spec.data.spec.queries[0].spec.query.kind",
                "NotDataQuery",
            ),
            (
                "viz",
                "spec.elements[\"panel-1\"].spec.vizConfig.kind",
                "NotVizConfig",
            ),
        ];

        for (case, expected_path, replacement) in cases {
            let mut value: serde_json::Value = serde_json::from_str(include_str!(
                "../tests/fixtures/grafana/v2_compatibility.json"
            ))
            .unwrap();
            match case {
                "data" => {
                    value["spec"]["elements"]["panel-1"]["spec"]["data"]["kind"] =
                        replacement.into()
                }
                "query" => {
                    value["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["queries"][0]["kind"] =
                        replacement.into()
                }
                "data_query" => {
                    value["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["queries"][0]["spec"]
                        ["query"]["kind"] = replacement.into()
                }
                "viz" => {
                    value["spec"]["elements"]["panel-1"]["spec"]["vizConfig"]["kind"] =
                        replacement.into()
                }
                _ => unreachable!(),
            }
            let error = parse_grafana_dashboard(&value.to_string())
                .unwrap_err()
                .to_string();
            assert!(error.contains(replacement));
            assert!(error.contains(expected_path));
        }
    }

    #[test]
    fn classic_variable_refresh_settings_decide_when_queries_run() {
        let variable = |name: &str, refresh: serde_json::Value, current: &str| {
            serde_json::json!({
                "name": name,
                "type": "query",
                "query": format!("label_values({name})"),
                "refresh": refresh,
                "current": {"text": current, "value": current},
            })
        };
        let json = serde_json::json!({
            "title": "Refresh",
            "panels": [],
            "templating": {"list": [
                variable("saved", serde_json::json!(0), "api"),
                variable("cleared", serde_json::json!(0), ""),
                variable("legacy", serde_json::json!(false), "api"),
                variable("loaded", serde_json::json!(1), "api"),
                variable("ranged", serde_json::json!(2), "api"),
                variable("unset", serde_json::Value::Null, "api"),
            ]},
        });

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        let refreshes: Vec<_> = dashboard
            .query_vars
            .iter()
            .map(|variable| (variable.name.as_str(), variable.refresh))
            .collect();
        // `Never` keeps a saved value, and resolves only a cleared one.
        assert_eq!(
            refreshes,
            [
                ("cleared", VariableRefresh::OnLoad),
                ("loaded", VariableRefresh::OnLoad),
                ("ranged", VariableRefresh::OnTimeRangeChange),
                ("unset", VariableRefresh::OnLoad),
            ]
        );
        assert_eq!(dashboard.vars["saved"], "api");
    }

    #[test]
    fn v2_variable_refresh_settings_decide_when_queries_run() {
        let mut json = valid_v2_resource();
        let variable = |name: &str, refresh: &str| {
            serde_json::json!({
                "kind": "QueryVariable",
                "spec": {
                    "name": name,
                    "current": {"text": "api", "value": "api"},
                    "refresh": refresh,
                    "query": {"kind": "DataQuery", "group": "prometheus", "version": "v0",
                        "spec": {"query": format!("label_values({name})")}}
                }
            })
        };
        json["spec"]["variables"] = serde_json::json!([
            variable("saved", "never"),
            variable("loaded", "onDashboardLoad"),
            variable("ranged", "onTimeRangeChanged"),
        ]);

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        let refreshes: Vec<_> = dashboard
            .query_vars
            .iter()
            .map(|variable| (variable.name.as_str(), variable.refresh))
            .collect();
        assert_eq!(
            refreshes,
            [
                ("loaded", VariableRefresh::OnLoad),
                ("ranged", VariableRefresh::OnTimeRangeChange),
            ]
        );
    }

    #[test]
    fn v2_query_variable_falls_back_to_definition_with_its_native_path() {
        let mut json = valid_v2_resource();
        json["spec"]["variables"] = serde_json::json!([{
            "kind": "QueryVariable",
            "spec": {
                "name": "instance",
                "current": {"text": "node-1", "value": "node-1"},
                "query": {"kind": "DataQuery", "group": "prometheus", "version": "v0", "spec": {"query": "  "}},
                "definition": "  label_values(up, instance)  "
            }
        }]);

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert_eq!(
            dashboard.vars.get("instance").map(String::as_str),
            Some("node-1")
        );
        assert_eq!(dashboard.query_vars.len(), 1);
        assert_eq!(dashboard.query_vars[0].query, "label_values(up, instance)");
        assert_eq!(
            dashboard.query_vars[0].query_path,
            "spec.variables[0].spec.definition"
        );
    }

    #[test]
    fn v2_query_variable_reads_legacy_string_queries_from_converted_dashboards() {
        let mut json = valid_v2_resource();
        json["spec"]["variables"] = serde_json::json!([{
            "kind": "QueryVariable",
            "spec": {
                "name": "instance",
                "current": {"text": ["All"], "value": ["$__all"]},
                "query": {"kind": "DataQuery", "group": "prometheus", "version": "v0", "spec": {
                    "__legacyStringValue": "label_values(up, instance)"
                }},
                "multi": true,
                "includeAll": true
            }
        }]);

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert_eq!(dashboard.query_vars.len(), 1);
        assert_eq!(dashboard.query_vars[0].query, "label_values(up, instance)");
        assert_eq!(
            dashboard.query_vars[0].query_path,
            "spec.variables[0].spec.query.spec.__legacyStringValue"
        );
    }

    #[test]
    fn rejects_malformed_v2_query_variable_wrappers_at_native_paths() {
        for (case, expected_path) in [
            ("missing_query", "spec.variables[0].spec.query"),
            ("wrong_query_type", "spec.variables[0].spec.query"),
            ("missing_kind", "spec.variables[0].spec.query.kind"),
            ("wrong_kind", "spec.variables[0].spec.query.kind"),
            ("wrong_kind_type", "spec.variables[0].spec.query.kind"),
            ("missing_group", "spec.variables[0].spec.query.group"),
            ("wrong_group_type", "spec.variables[0].spec.query.group"),
        ] {
            let mut json = valid_v2_resource();
            json["spec"]["variables"] = serde_json::json!([{
                "kind": "QueryVariable",
                "spec": {
                    "name": "instance",
                    "current": {"text": "node-1", "value": "node-1"},
                    "query": {
                        "kind": "DataQuery",
                        "group": "prometheus",
                        "spec": {"query": "label_values(up, instance)"}
                    },
                    "definition": "label_values(up, instance)"
                }
            }]);

            let variable_spec = json["spec"]["variables"][0]["spec"]
                .as_object_mut()
                .unwrap();
            match case {
                "missing_query" => {
                    variable_spec.remove("query");
                }
                "wrong_query_type" => {
                    variable_spec.insert("query".into(), serde_json::json!([]));
                }
                "missing_kind" => {
                    variable_spec["query"]
                        .as_object_mut()
                        .unwrap()
                        .remove("kind");
                }
                "wrong_kind" => variable_spec["query"]["kind"] = serde_json::json!("FutureQuery"),
                "wrong_kind_type" => variable_spec["query"]["kind"] = serde_json::json!(1),
                "missing_group" => {
                    variable_spec["query"]
                        .as_object_mut()
                        .unwrap()
                        .remove("group");
                }
                "wrong_group_type" => variable_spec["query"]["group"] = serde_json::json!(1),
                _ => unreachable!(),
            };

            let error = parse_grafana_dashboard(&json.to_string())
                .expect_err(case)
                .to_string();
            assert!(
                error.contains(expected_path),
                "{case}: expected `{expected_path}` in `{error}`"
            );
        }
    }

    #[test]
    fn rejects_missing_or_non_object_v2_query_variable_data_query_spec() {
        for case in ["missing_spec", "wrong_spec_type"] {
            let mut json = valid_v2_resource();
            json["spec"]["variables"] = serde_json::json!([{
                "kind": "QueryVariable",
                "spec": {
                    "name": "instance",
                    "current": {"text": "node-1", "value": "node-1"},
                    "query": {
                        "kind": "DataQuery",
                        "group": "prometheus",
                        "spec": {"query": "label_values(up, instance)"}
                    },
                    "definition": "label_values(up, instance)"
                }
            }]);
            let query = json["spec"]["variables"][0]["spec"]["query"]
                .as_object_mut()
                .unwrap();
            match case {
                "missing_spec" => {
                    query.remove("spec");
                }
                "wrong_spec_type" => {
                    query.insert("spec".into(), serde_json::json!([]));
                }
                _ => unreachable!(),
            }

            let error = parse_grafana_dashboard(&json.to_string())
                .expect_err(case)
                .to_string();
            assert!(
                error.contains("spec.variables[0].spec.query.spec"),
                "{case}: expected native DataQuery.spec path in `{error}`"
            );
        }
    }

    #[test]
    fn v2_supported_option_variables_import_object_current_values() {
        let mut json = valid_v2_resource();
        json["spec"]["variables"] = serde_json::json!([
            {"kind": "TextVariable", "spec": {"name": "text", "current": {"text": "one", "value": "one"}}},
            {"kind": "ConstantVariable", "spec": {"name": "constant", "current": {"text": "two", "value": "two"}}},
            {"kind": "DatasourceVariable", "spec": {"name": "datasource", "current": {"text": "three", "value": "three"}}},
            {"kind": "IntervalVariable", "spec": {"name": "interval", "current": {"text": "four", "value": "four"}}},
            {"kind": "CustomVariable", "spec": {"name": "custom", "current": {"text": "five", "value": "five"}}},
            {"kind": "GroupByVariable", "spec": {"name": "group_by", "current": {"text": "six", "value": "six"}}}
        ]);

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        for (name, value) in [
            ("text", "one"),
            ("constant", "two"),
            ("datasource", "three"),
            ("interval", "four"),
            ("custom", "five"),
            ("group_by", "six"),
        ] {
            assert_eq!(dashboard.vars.get(name).map(String::as_str), Some(value));
        }
    }

    #[test]
    fn v2_switch_variable_imports_direct_current_value() {
        let mut json = valid_v2_resource();
        json["spec"]["variables"] = serde_json::json!([{
            "kind": "SwitchVariable",
            "spec": {"name": "show_total", "current": "true"}
        }]);

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert_eq!(
            dashboard.vars.get("show_total").map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn v2_diagnostics_non_prometheus_query_variable_is_not_dynamic() {
        let mut json = valid_v2_resource();
        json["spec"]["variables"] = serde_json::json!([{
            "kind": "QueryVariable",
            "spec": {
                "name": "service",
                "current": {"text": "api", "value": "api"},
                "query": {"kind": "DataQuery", "group": "loki", "version": "v0", "spec": {"query": "label_values(service)"}}
            }
        }]);

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert_eq!(
            dashboard.vars.get("service").map(String::as_str),
            Some("api")
        );
        assert!(dashboard.query_vars.is_empty());
        assert_eq!(dashboard.diagnostics.len(), 1);
        assert_eq!(dashboard.diagnostics[0].code, "unsupported_datasource");
        assert_eq!(
            dashboard.diagnostics[0].path,
            "spec.variables[0].spec.query"
        );
    }

    #[test]
    fn v2_query_variable_uses_its_all_value_for_all_current_selection() {
        let mut json = valid_v2_resource();
        json["spec"]["variables"] = serde_json::json!([{
            "kind": "QueryVariable",
            "spec": {
                "name": "job",
                "current": {"text": "All", "value": "$__all"},
                "query": {"kind": "DataQuery", "group": "prometheus", "version": "v0", "spec": {"query": "label_values(up, job)"}},
                "allValue": "api|worker"
            }
        }]);

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert_eq!(
            dashboard.vars.get("job").map(String::as_str),
            Some("api|worker")
        );
        assert_eq!(dashboard.query_vars.len(), 1);
        assert!(dashboard.query_vars[0].select_all);
        assert_eq!(
            dashboard.query_vars[0].all_value.as_deref(),
            Some("api|worker")
        );
    }

    #[test]
    fn v2_custom_variable_all_selection_matches_classic_all_value_behavior() {
        let classic = parse_grafana_dashboard(
            r#"{
                "title": "Classic",
                "templating": {"list": [{
                    "name": "region",
                    "type": "custom",
                    "current": {"text": "All", "value": "$__all"},
                    "allValue": "eu-west-1|us-east-1"
                }]}
            }"#,
        )
        .unwrap();
        let mut json = valid_v2_resource();
        json["spec"]["variables"] = serde_json::json!([{
            "kind": "CustomVariable",
            "spec": {
                "name": "region",
                "current": {"text": "All", "value": "$__all"},
                "allValue": "eu-west-1|us-east-1"
            }
        }]);

        let v2 = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert_eq!(
            v2.vars.get("region").map(String::as_str),
            Some("eu-west-1|us-east-1")
        );
        assert_eq!(v2.vars.get("region"), classic.vars.get("region"));
    }

    /// Applies a named malformation to `v2_compatibility.json`'s only panel.
    fn v2_compatibility_with_panel_case(case: &str) -> serde_json::Value {
        let mut json: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/grafana/v2_compatibility.json"
        ))
        .unwrap();
        let panel = &mut json["spec"]["elements"]["panel-1"]["spec"];
        match case {
            "missing_title" => {
                panel.as_object_mut().unwrap().remove("title");
            }
            "wrong_title_type" => panel["title"] = serde_json::json!(1),
            "missing_transformations" => {
                panel["data"]["spec"]
                    .as_object_mut()
                    .unwrap()
                    .remove("transformations");
            }
            "wrong_transformations_type" => {
                panel["data"]["spec"]["transformations"] = serde_json::json!({})
            }
            "missing_hidden" => {
                panel["data"]["spec"]["queries"][0]["spec"]
                    .as_object_mut()
                    .unwrap()
                    .remove("hidden");
            }
            "wrong_hidden_type" => {
                panel["data"]["spec"]["queries"][0]["spec"]["hidden"] = serde_json::json!("false")
            }
            "missing_data_query_group" => {
                panel["data"]["spec"]["queries"][0]["spec"]["query"]
                    .as_object_mut()
                    .unwrap()
                    .remove("group");
            }
            "wrong_data_query_group_type" => {
                panel["data"]["spec"]["queries"][0]["spec"]["query"]["group"] = serde_json::json!(1)
            }
            "missing_data_query_spec" => {
                panel["data"]["spec"]["queries"][0]["spec"]["query"]
                    .as_object_mut()
                    .unwrap()
                    .remove("spec");
            }
            "wrong_data_query_spec_type" => {
                panel["data"]["spec"]["queries"][0]["spec"]["query"]["spec"] = serde_json::json!([])
            }
            "missing_viz_group" => {
                panel["vizConfig"].as_object_mut().unwrap().remove("group");
            }
            "wrong_viz_group_type" => panel["vizConfig"]["group"] = serde_json::json!(1),
            "missing_viz_spec" => {
                panel["vizConfig"].as_object_mut().unwrap().remove("spec");
            }
            "wrong_viz_spec_type" => panel["vizConfig"]["spec"] = serde_json::json!([]),
            "missing_field_config" => {
                panel["vizConfig"]["spec"]
                    .as_object_mut()
                    .unwrap()
                    .remove("fieldConfig");
            }
            "wrong_field_config_type" => {
                panel["vizConfig"]["spec"]["fieldConfig"] = serde_json::json!([])
            }
            "missing_defaults" => {
                panel["vizConfig"]["spec"]["fieldConfig"]
                    .as_object_mut()
                    .unwrap()
                    .remove("defaults");
            }
            "wrong_defaults_type" => {
                panel["vizConfig"]["spec"]["fieldConfig"]["defaults"] = serde_json::json!([])
            }
            "missing_options" => {
                panel["vizConfig"]["spec"]
                    .as_object_mut()
                    .unwrap()
                    .remove("options");
            }
            "wrong_options_type" => panel["vizConfig"]["spec"]["options"] = serde_json::json!([]),
            _ => unreachable!(),
        }
        json
    }

    #[test]
    fn rejects_malformed_required_v2_panel_fields_at_native_paths() {
        let cases = [
            ("wrong_title_type", "spec.elements[\"panel-1\"].spec.title"),
            (
                "wrong_transformations_type",
                "spec.elements[\"panel-1\"].spec.data.spec.transformations",
            ),
            (
                "wrong_hidden_type",
                "spec.elements[\"panel-1\"].spec.data.spec.queries[0].spec.hidden",
            ),
            (
                "missing_data_query_group",
                "spec.elements[\"panel-1\"].spec.data.spec.queries[0].spec.query.group",
            ),
            (
                "wrong_data_query_group_type",
                "spec.elements[\"panel-1\"].spec.data.spec.queries[0].spec.query.group",
            ),
            (
                "wrong_data_query_spec_type",
                "spec.elements[\"panel-1\"].spec.data.spec.queries[0].spec.query.spec",
            ),
            (
                "missing_viz_group",
                "spec.elements[\"panel-1\"].spec.vizConfig.group",
            ),
            (
                "wrong_viz_group_type",
                "spec.elements[\"panel-1\"].spec.vizConfig.group",
            ),
            (
                "wrong_viz_spec_type",
                "spec.elements[\"panel-1\"].spec.vizConfig.spec",
            ),
            (
                "wrong_field_config_type",
                "spec.elements[\"panel-1\"].spec.vizConfig.spec.fieldConfig",
            ),
            (
                "wrong_defaults_type",
                "spec.elements[\"panel-1\"].spec.vizConfig.spec.fieldConfig.defaults",
            ),
            (
                "wrong_options_type",
                "spec.elements[\"panel-1\"].spec.vizConfig.spec.options",
            ),
        ];

        for (case, expected_path) in cases {
            let json = v2_compatibility_with_panel_case(case);
            let error = parse_grafana_dashboard(&json.to_string())
                .expect_err(case)
                .to_string();
            assert!(
                error.contains(expected_path),
                "{case}: expected `{expected_path}` in `{error}`"
            );
        }
    }

    #[test]
    fn v2_panels_default_fields_that_real_exports_omit() {
        for case in [
            "missing_title",
            "missing_transformations",
            "missing_hidden",
            "missing_data_query_spec",
            "missing_viz_spec",
            "missing_field_config",
            "missing_defaults",
            "missing_options",
        ] {
            let json = v2_compatibility_with_panel_case(case);
            let dashboard = parse_grafana_dashboard(&json.to_string())
                .unwrap_or_else(|error| panic!("{case}: {error:#}"));
            assert_eq!(dashboard.layout.visible_panel_indices().len(), 1, "{case}");
        }
    }

    #[test]
    fn v2_panels_accept_absent_or_null_id_and_links() {
        for (field, value) in [
            ("id", None),
            ("id", Some(serde_json::Value::Null)),
            ("links", None),
            ("links", Some(serde_json::Value::Null)),
        ] {
            let mut json: serde_json::Value = serde_json::from_str(include_str!(
                "../tests/fixtures/grafana/v2_compatibility.json"
            ))
            .unwrap();
            let panel = json["spec"]["elements"]["panel-1"]["spec"]
                .as_object_mut()
                .unwrap();
            match value {
                Some(value) => {
                    panel.insert(field.to_string(), value);
                }
                None => {
                    panel.remove(field);
                }
            }
            parse_grafana_dashboard(&json.to_string())
                .unwrap_or_else(|error| panic!("{field}: {error:#}"));
        }
    }

    #[test]
    fn rejects_malformed_v2_panel_id_and_links_at_native_paths() {
        for (case, expected_path) in [
            ("wrong_id_type", "spec.elements[\"panel-1\"].spec.id"),
            ("wrong_links_type", "spec.elements[\"panel-1\"].spec.links"),
        ] {
            let mut json: serde_json::Value = serde_json::from_str(include_str!(
                "../tests/fixtures/grafana/v2_compatibility.json"
            ))
            .unwrap();
            let panel = &mut json["spec"]["elements"]["panel-1"]["spec"];
            match case {
                "wrong_id_type" => panel["id"] = serde_json::json!("1"),
                "wrong_links_type" => panel["links"] = serde_json::json!({}),
                _ => unreachable!(),
            }

            let error = parse_grafana_dashboard(&json.to_string())
                .expect_err(case)
                .to_string();
            assert!(
                error.contains(expected_path),
                "{case}: expected `{expected_path}` in `{error}`"
            );
        }
    }

    #[test]
    fn v2_diagnostics_unsupported_variable_kinds_are_skipped_with_native_paths() {
        let mut json = valid_v2_resource();
        json["spec"]["variables"] = serde_json::json!([
            {"kind": "AdhocVariable", "spec": {"name": "adhoc"}},
            {"kind": "FutureVariable", "spec": {"name": "future"}}
        ]);

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert!(dashboard.vars.is_empty());
        assert!(dashboard.query_vars.is_empty());
        assert_eq!(dashboard.diagnostics.len(), 2);
        assert_eq!(dashboard.diagnostics[0].code, "unsupported_variable");
        assert_eq!(dashboard.diagnostics[0].path, "spec.variables[0]");
        assert_eq!(dashboard.diagnostics[1].code, "unsupported_variable");
        assert_eq!(dashboard.diagnostics[1].path, "spec.variables[1]");
    }

    #[test]
    fn v2_diagnostics_unsupported_elements_are_skipped_before_deserialization() {
        for (kind, name) in [("LibraryPanel", "library-1"), ("FutureElement", "future-1")] {
            let mut json = valid_v2_resource();
            json["spec"]["elements"] = serde_json::json!({
                name: {"kind": kind, "spec": {"name": "not-a-panel"}}
            });
            json["spec"]["layout"]["spec"]["items"][0]["spec"]["element"]["name"] = name.into();

            let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

            assert!(dashboard.queries.is_empty());
            assert_eq!(dashboard.skipped_panels, 1);
            assert_eq!(dashboard.diagnostics.len(), 1);
            assert_eq!(dashboard.diagnostics[0].code, "unsupported_element");
            assert_eq!(
                dashboard.diagnostics[0].path,
                format!("spec.elements[{name:?}]")
            );
            assert_eq!(
                dashboard.diagnostics[0]
                    .message
                    .contains("Share dashboard with another instance"),
                kind == "LibraryPanel",
                "{kind}: {}",
                dashboard.diagnostics[0].message
            );
        }
    }

    #[test]
    fn v2_prometheus_compatible_datasource_groups_import_as_prometheus() {
        for group in [
            "grafana-amazonprometheus-datasource",
            "grafana-azureprometheus-datasource",
        ] {
            let mut json = valid_v2_resource();
            make_v2_panel_importable(&mut json);
            let query =
                &mut json["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["queries"][0];
            query["spec"]["query"]["group"] = group.into();

            let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

            assert_eq!(dashboard.queries.len(), 1, "{group}");
            assert!(dashboard.diagnostics.is_empty(), "{group}");
        }
    }

    /// `v2_grafana13_export.json` is a dashboard authored through Grafana 13.2.3's
    /// `dashboard.grafana.app/v2` API and read back unchanged apart from dropping
    /// `status`, so it carries the server's real serialization: unset slices are
    /// `null`, `vizConfig.version` is empty, and the query-less text panel's query
    /// group has an empty `kind`.
    #[test]
    fn v2_grafana13_export_imports_server_serialized_nulls() {
        let dashboard = parse_grafana_dashboard(include_str!(
            "../tests/fixtures/grafana/v2_grafana13_export.json"
        ))
        .unwrap();

        assert_eq!(dashboard.title, "Grafatui native V2");
        assert_eq!(dashboard.refresh_rate_ms, Some(30_000));
        let titles: Vec<_> = dashboard.queries.iter().map(|q| q.title.as_str()).collect();
        assert_eq!(titles, ["Scrape duration", "Targets up"]);
        assert_eq!(
            dashboard.queries[1].query_modes,
            [crate::app::QueryMode::Instant]
        );
        assert_eq!(
            dashboard.vars.get("job").map(String::as_str),
            Some("prometheus")
        );
        assert_eq!(
            dashboard.vars.get("quantile").map(String::as_str),
            Some("0.9")
        );
        assert_eq!(dashboard.query_vars.len(), 1);
        assert_eq!(dashboard.skipped_panels, 1);
        assert_eq!(dashboard.diagnostics.len(), 1);
        assert_eq!(dashboard.diagnostics[0].code, "skipped_panel");
        assert_eq!(dashboard.diagnostics[0].path, "spec.elements[\"panel-3\"]");
    }

    /// `v2_grafana13_external_export.json` applies Grafana's "Share dashboard with
    /// another instance" rules to the export above: query datasources are replaced
    /// by export labels and query variable selections are cleared.
    #[test]
    fn v2_grafana13_external_export_resolves_variables_dynamically() {
        let dashboard = parse_grafana_dashboard(include_str!(
            "../tests/fixtures/grafana/v2_grafana13_external_export.json"
        ))
        .unwrap();

        assert_eq!(dashboard.queries.len(), 2);
        assert!(!dashboard.vars.contains_key("job"));
        assert_eq!(
            dashboard.vars.get("quantile").map(String::as_str),
            Some("0.9")
        );
        assert_eq!(dashboard.query_vars.len(), 1);
        assert_eq!(dashboard.query_vars[0].name, "job");
        assert_eq!(dashboard.query_vars[0].query, "label_values(up, job)");
        assert_eq!(dashboard.diagnostics.len(), 1);
    }

    #[test]
    fn v2_diagnostics_mixed_datasources_retain_prometheus_expressions() {
        let mut json = valid_v2_resource();
        json["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["queries"] = serde_json::json!([
            {
                "kind": "PanelQuery",
                "spec": {
                    "hidden": false,
                    "refId": "A",
                    "query": {"kind": "DataQuery", "group": "loki", "spec": {"expr": "{app=\"api\"}"}}
                }
            },
            {
                "kind": "PanelQuery",
                "spec": {
                    "hidden": false,
                    "refId": "B",
                    "query": {"kind": "DataQuery", "group": "prometheus", "spec": {"expr": "up"}}
                }
            }
        ]);

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert_eq!(dashboard.queries.len(), 1);
        assert_eq!(dashboard.queries[0].exprs, ["up"]);
        assert_eq!(dashboard.diagnostics.len(), 1);
        assert_eq!(dashboard.diagnostics[0].code, "unsupported_datasource");
        assert_eq!(
            dashboard.diagnostics[0].path,
            "spec.elements[\"panel-1\"].spec.data.spec.queries[0].spec.query"
        );
    }

    #[test]
    fn v2_all_unsupported_datasource_targets_increment_skipped_panels_once() {
        let mut json = valid_v2_resource();
        json["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["queries"] = serde_json::json!([{
            "kind": "PanelQuery",
            "spec": {
                "hidden": false,
                "refId": "A",
                "query": {
                    "kind": "DataQuery",
                    "group": "loki",
                    "spec": {"expr": "{app=\"api\"}"}
                }
            }
        }]);

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert!(dashboard.queries.is_empty());
        assert_eq!(dashboard.skipped_panels, 1);
        assert_eq!(dashboard.diagnostics.len(), 1);
        assert_eq!(dashboard.diagnostics[0].code, "unsupported_datasource");
        assert_eq!(
            dashboard.diagnostics[0].path,
            "spec.elements[\"panel-1\"].spec.data.spec.queries[0].spec.query"
        );
    }

    #[test]
    fn v2_diagnostics_missing_visible_prometheus_expression() {
        let mut json = valid_v2_resource();
        json["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["queries"] = serde_json::json!([{
            "kind": "PanelQuery",
            "spec": {
                "hidden": false,
                "refId": "A",
                "query": {"kind": "DataQuery", "group": "prometheus", "spec": {"expr": "  "}}
            }
        }]);

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert!(dashboard.queries.is_empty());
        assert_eq!(dashboard.diagnostics.len(), 1);
        assert_eq!(dashboard.diagnostics[0].code, "missing_query_expression");
        assert_eq!(
            dashboard.diagnostics[0].path,
            "spec.elements[\"panel-1\"].spec.data.spec.queries[0].spec.query.spec.expr"
        );
    }

    #[test]
    fn v2_diagnostics_transformations_use_the_native_path() {
        let mut json = valid_v2_resource();
        json["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["transformations"] =
            serde_json::json!([{"kind": "reduce"}]);

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert_eq!(dashboard.diagnostics.len(), 1);
        assert_eq!(dashboard.diagnostics[0].code, "ignored_field");
        assert_eq!(
            dashboard.diagnostics[0].path,
            "spec.elements[\"panel-1\"].spec.data.spec.transformations"
        );
    }

    #[test]
    fn v2_diagnostics_unsupported_viz_groups_increment_skipped_panels() {
        let mut json = valid_v2_resource();
        json["spec"]["elements"]["panel-1"]["spec"]["vizConfig"]["group"] =
            serde_json::json!("piechart");

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert!(dashboard.queries.is_empty());
        assert_eq!(dashboard.skipped_panels, 1);
        assert_eq!(dashboard.diagnostics.len(), 1);
        assert_eq!(dashboard.diagnostics[0].code, "skipped_panel");
        assert_eq!(dashboard.diagnostics[0].path, "spec.elements[\"panel-1\"]");
    }

    #[test]
    fn default_reduce_options_are_not_reported() {
        let panel = |options: &str| {
            format!(
                r#"{{"title": "Reduce", "panels": [{{"type": "stat", "title": "Panel",
                    "targets": [{{"expr": "up"}}], "options": {{"reduceOptions": {options}}}}}]}}"#
            )
        };
        for default in [
            r#"{"values": false, "calcs": ["lastNotNull"], "fields": ""}"#,
            r#"{"calcs": ["last"]}"#,
            r#"{"calcs": []}"#,
        ] {
            let dashboard = parse_grafana_dashboard(&panel(default)).unwrap();
            assert!(dashboard.diagnostics.is_empty(), "{default}");
        }
        for custom in [
            r#"{"calcs": ["mean"]}"#,
            r#"{"values": true, "calcs": ["lastNotNull"]}"#,
            r#"{"calcs": ["lastNotNull"], "fields": "/^cpu$/"}"#,
        ] {
            let dashboard = parse_grafana_dashboard(&panel(custom)).unwrap();
            assert_eq!(dashboard.diagnostics.len(), 1, "{custom}");
        }
    }

    #[test]
    fn v2_diagnostics_mappings_and_reduce_options_reuse_classic_messages() {
        let classic = parse_grafana_dashboard(
            r#"{
                "title": "Classic",
                "panels": [{
                    "type": "stat",
                    "title": "Panel",
                    "targets": [{"expr": "up"}],
                    "fieldConfig": {"defaults": {"mappings": [{"type": "value"}]}},
                    "options": {"reduceOptions": {"calcs": ["mean"]}}
                }]
            }"#,
        )
        .unwrap();
        let mut json = valid_v2_resource();
        json["spec"]["elements"]["panel-1"]["spec"]["vizConfig"]["spec"]["fieldConfig"]["defaults"]
            ["mappings"] = serde_json::json!([{"type": "value"}]);
        json["spec"]["elements"]["panel-1"]["spec"]["vizConfig"]["spec"]["options"]["reduceOptions"] =
            serde_json::json!({"calcs": ["mean"]});

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert_eq!(dashboard.diagnostics.len(), 2);
        assert_eq!(dashboard.diagnostics[0].code, "ignored_field");
        assert_eq!(
            dashboard.diagnostics[0].path,
            "spec.elements[\"panel-1\"].spec.vizConfig.spec.options.reduceOptions"
        );
        assert_eq!(
            dashboard.diagnostics[0].message,
            classic.diagnostics[0].message
        );
        assert_eq!(dashboard.diagnostics[1].code, "ignored_field");
        assert_eq!(
            dashboard.diagnostics[1].path,
            "spec.elements[\"panel-1\"].spec.vizConfig.spec.fieldConfig.defaults.mappings"
        );
        assert_eq!(
            dashboard.diagnostics[1].message,
            classic.diagnostics[1].message
        );
    }

    #[test]
    fn v2_diagnostics_variable_analysis_uses_v2_expression_paths() {
        let mut json = valid_v2_resource();
        json["spec"]["variables"] = serde_json::json!([{
            "kind": "CustomVariable",
            "spec": {"name": "job", "current": {"text": "api", "value": "api"}}
        }]);
        json["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["queries"] = serde_json::json!([{
            "kind": "PanelQuery",
            "spec": {
                "hidden": false,
                "refId": "A",
                "query": {
                    "kind": "DataQuery",
                    "group": "prometheus",
                    "spec": {"expr": "up{job=~\"${job:regex}\", cluster=\"$cluster\"}"}
                }
            }
        }]);

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();
        let diagnostics = variable_diagnostics(&dashboard, &dashboard.vars);

        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.iter().all(|diagnostic| {
            diagnostic.path
                == "spec.elements[\"panel-1\"].spec.data.spec.queries[0].spec.query.spec.expr"
        }));
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "unsupported_variable_modifier")
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "unresolved_variable")
        );
    }

    #[test]
    fn v2_grid_item_repeat_is_recorded_for_its_panel() {
        let mut json = valid_v2_resource();
        make_v2_panel_importable(&mut json);
        json["spec"]["variables"] = serde_json::json!([
            {"kind": "CustomVariable", "spec": {"name": "job", "query": "api,worker"}}
        ]);
        let item = &mut json["spec"]["layout"]["spec"]["items"][0]["spec"];
        item["repeat"] = serde_json::json!({
            "mode": "variable",
            "value": "job",
            "direction": "v",
            "maxPerRow": 2
        });

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert_eq!(
            dashboard.repeats.panels.get(&0),
            Some(&crate::dashboard::Repeat {
                variable: "job".to_string(),
                direction: crate::dashboard::RepeatDirection::Vertical,
                max_per_row: Some(2),
            })
        );
        assert!(dashboard.diagnostics.is_empty());
    }

    fn v2_row_with_condition(condition: serde_json::Value) -> Result<DashboardImport> {
        let mut json = v2_row_resource_with_field("conditionalRendering", condition);
        json["spec"]["variables"] = serde_json::json!([
            {"kind": "CustomVariable", "spec": {"name": "env", "query": "prod,dev"}}
        ]);
        parse_grafana_dashboard(&json.to_string())
    }

    #[test]
    fn v2_conditional_rendering_groups_are_imported_for_rows_tabs_and_auto_grid_items() {
        use crate::conditions::{Condition, ConditionGroup, VariableOperator};

        let dashboard = v2_row_with_condition(serde_json::json!({
            "kind": "ConditionalRenderingGroup",
            "spec": {"visibility": "hide", "condition": "or", "items": [
                {"kind": "ConditionalRenderingVariable", "spec": {"variable": "env", "operator": "notMatches", "value": "^prod$"}},
                {"kind": "ConditionalRenderingData", "spec": {"value": false}},
                {"kind": "ConditionalRenderingTimeRangeSize", "spec": {"value": "7d"}},
                {"kind": "ConditionalRenderingFuture", "spec": {}}
            ]}
        }))
        .unwrap();

        assert_eq!(
            dashboard
                .conditions
                .rows
                .get(&crate::dashboard::RowId::new(0)),
            Some(&ConditionGroup {
                show: false,
                match_all: false,
                conditions: vec![
                    Condition::Variable {
                        name: "env".to_string(),
                        operator: VariableOperator::NotMatches,
                        value: "^prod$".to_string(),
                    },
                    Condition::Data { has_data: false },
                    Condition::TimeRangeAtMost(Some(604_800.0)),
                ],
            })
        );
        assert_eq!(dashboard.diagnostics.len(), 1);
        assert_eq!(dashboard.diagnostics[0].code, "unsupported_condition");
        assert_eq!(
            dashboard.diagnostics[0].path,
            "spec.layout.spec.rows[0].spec.conditionalRendering.spec.items[3]"
        );

        let mut json = valid_v2_resource();
        make_v2_panel_importable(&mut json);
        let condition = serde_json::json!({
            "kind": "ConditionalRenderingGroup",
            "spec": {"visibility": "show", "condition": "and", "items": [
                {"kind": "ConditionalRenderingData", "spec": {"value": true}}
            ]}
        });
        json["spec"]["layout"] = serde_json::json!({
            "kind": "TabsLayout",
            "spec": {"tabs": [{"kind": "TabsLayoutTab", "spec": {
                "title": "Tab",
                "conditionalRendering": condition,
                "layout": {"kind": "AutoGridLayout", "spec": {"items": [{
                    "kind": "AutoGridLayoutItem",
                    "spec": {
                        "element": {"kind": "ElementReference", "name": "panel-1"},
                        "conditionalRendering": condition
                    }
                }]}}
            }}]}
        });

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        let data = ConditionGroup {
            show: true,
            match_all: true,
            conditions: vec![Condition::Data { has_data: true }],
        };
        assert_eq!(dashboard.conditions.panels.get(&0), Some(&data));
        assert_eq!(
            dashboard
                .conditions
                .tabs
                .get(&(crate::dashboard::TabGroupId::new(0), 0)),
            Some(&data)
        );
    }

    /// `v2_grafana13_conditional.json` was authored through Grafana 13.2.3's V2 API
    /// with variable, data, and time range conditions on rows, tabs, and an auto
    /// grid item.
    #[test]
    fn v2_grafana13_conditional_rendering_fixture_imports_every_group() {
        use crate::conditions::Condition;

        let dashboard = parse_grafana_dashboard(include_str!(
            "../tests/fixtures/grafana/v2_grafana13_conditional.json"
        ))
        .unwrap();

        assert!(
            dashboard.diagnostics.is_empty(),
            "{:?}",
            dashboard.diagnostics
        );
        assert_eq!(dashboard.conditions.rows.len(), 2);
        assert_eq!(dashboard.conditions.tabs.len(), 1);
        assert_eq!(dashboard.conditions.panels.len(), 1);
        let tab = dashboard.conditions.tabs.values().next().unwrap();
        assert_eq!(tab.conditions, [Condition::TimeRangeAtMost(Some(3600.0))]);
        let panel = dashboard.conditions.panels.values().next().unwrap();
        assert_eq!(panel.conditions, [Condition::Data { has_data: true }]);
    }

    #[test]
    fn v2_rejects_malformed_conditional_rendering_at_native_paths() {
        let path = "spec.layout.spec.rows[0].spec.conditionalRendering";
        for (condition, expected) in [
            (
                serde_json::json!({"kind": "Other", "spec": {}}),
                format!("{path}.kind"),
            ),
            (
                serde_json::json!({"kind": "ConditionalRenderingGroup", "spec": {"visibility": "maybe"}}),
                format!("{path}.spec.visibility"),
            ),
            (
                serde_json::json!({"kind": "ConditionalRenderingGroup", "spec": {"condition": "xor"}}),
                format!("{path}.spec.condition"),
            ),
            (
                serde_json::json!({"kind": "ConditionalRenderingGroup", "spec": {"items": [
                    {"kind": "ConditionalRenderingVariable", "spec": {"variable": "env", "operator": "like"}}
                ]}}),
                format!("{path}.spec.items[0].spec.operator"),
            ),
            (
                serde_json::json!({"kind": "ConditionalRenderingGroup", "spec": {"items": [
                    {"kind": "ConditionalRenderingVariable", "spec": {"value": "x"}}
                ]}}),
                format!("{path}.spec.items[0].spec.variable"),
            ),
        ] {
            let error = v2_row_with_condition(condition.clone())
                .unwrap_err()
                .to_string();
            assert!(error.contains(&expected), "{condition}: {error}");
        }
    }

    #[test]
    fn v2_null_or_empty_repeats_are_ignored() {
        for repeat in [
            serde_json::Value::Null,
            serde_json::json!({"mode": "variable", "value": ""}),
        ] {
            let mut json = valid_v2_resource();
            make_v2_panel_importable(&mut json);
            json["spec"]["layout"]["spec"]["items"][0]["spec"]["repeat"] = repeat;

            let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

            assert!(dashboard.repeats.is_empty());
            assert!(dashboard.diagnostics.is_empty());
        }
    }

    #[test]
    fn v2_repeats_of_undefined_variables_render_once_with_a_diagnostic() {
        let mut json = valid_v2_resource();
        make_v2_panel_importable(&mut json);
        json["spec"]["layout"]["spec"]["items"][0]["spec"]["repeat"] =
            serde_json::json!({"mode": "variable", "value": "missing"});

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert!(dashboard.repeats.is_empty());
        assert_eq!(dashboard.queries.len(), 1);
        assert_eq!(dashboard.diagnostics.len(), 1);
        assert_eq!(dashboard.diagnostics[0].code, "unknown_repeat_variable");
        assert_eq!(dashboard.diagnostics[0].path, "spec.elements[\"panel-1\"]");
    }

    #[test]
    fn v2_rejects_malformed_repeats_at_native_paths() {
        for (repeat, expected) in [
            (
                serde_json::json!({"mode": "query", "value": "job"}),
                "repeat.mode",
            ),
            (
                serde_json::json!({"value": "job", "direction": "x"}),
                "repeat.direction",
            ),
            (serde_json::json!({"value": 1}), "repeat.value"),
            (serde_json::json!([]), "repeat"),
        ] {
            let mut json = valid_v2_resource();
            json["spec"]["layout"]["spec"]["items"][0]["spec"]["repeat"] = repeat.clone();

            let error = parse_grafana_dashboard(&json.to_string())
                .unwrap_err()
                .to_string();

            assert!(
                error.contains(&format!("spec.layout.spec.items[0].spec.{expected}")),
                "{repeat}: {error}"
            );
        }
    }

    #[test]
    fn v2_row_tab_and_auto_grid_repeats_are_recorded() {
        let mut json = valid_v2_resource();
        make_v2_panel_importable(&mut json);
        json["spec"]["variables"] = serde_json::json!([
            {"kind": "CustomVariable", "spec": {"name": "dc", "query": "eu,us"}}
        ]);
        let repeat = serde_json::json!({"mode": "variable", "value": "dc"});
        json["spec"]["layout"] = serde_json::json!({
            "kind": "TabsLayout",
            "spec": {"tabs": [{"kind": "TabsLayoutTab", "spec": {
                "title": "$dc",
                "repeat": repeat,
                "layout": {"kind": "RowsLayout", "spec": {"rows": [{"kind": "RowsLayoutRow", "spec": {
                    "title": "Row $dc",
                    "repeat": repeat,
                    "layout": {"kind": "AutoGridLayout", "spec": {"items": [{
                        "kind": "AutoGridLayoutItem",
                        "spec": {
                            "element": {"kind": "ElementReference", "name": "panel-1"},
                            "repeat": repeat
                        }
                    }]}}
                }}]}}
            }}]}
        });

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        let dc = crate::dashboard::Repeat::new("dc");
        assert_eq!(dashboard.repeats.panels.get(&0), Some(&dc));
        assert_eq!(
            dashboard.repeats.rows.get(&crate::dashboard::RowId::new(0)),
            Some(&dc)
        );
        assert_eq!(
            dashboard
                .repeats
                .tabs
                .get(&(crate::dashboard::TabGroupId::new(0), 0)),
            Some(&dc)
        );
        // Without a saved selection, the first option is selected.
        assert_eq!(
            dashboard.var_values.get("dc"),
            Some(&vec!["eu".to_string()])
        );
        assert_eq!(dashboard.vars.get("dc").map(String::as_str), Some("eu"));
    }

    #[test]
    fn rejects_wrong_v2_grid_item_kind() {
        let mut json = valid_v2_resource();
        json["spec"]["layout"]["spec"]["items"][0]["kind"] = serde_json::json!("RowsLayoutItem");
        assert!(
            parse_grafana_dashboard(&json.to_string())
                .unwrap_err()
                .to_string()
                .contains("spec.layout.spec.items[0].kind")
        );
    }

    #[test]
    fn rejects_wrong_v2_element_reference_kind() {
        let mut json = valid_v2_resource();
        json["spec"]["layout"]["spec"]["items"][0]["spec"]["element"]["kind"] =
            serde_json::json!("Panel");
        assert!(
            parse_grafana_dashboard(&json.to_string())
                .unwrap_err()
                .to_string()
                .contains("spec.layout.spec.items[0].spec.element.kind")
        );
    }

    #[test]
    fn rejects_invalid_v2_grid_coordinate() {
        for value in [
            None,
            Some(serde_json::json!("0")),
            Some(serde_json::json!(2147483648_u64)),
        ] {
            let mut json = valid_v2_resource();
            if let Some(value) = value {
                json["spec"]["layout"]["spec"]["items"][0]["spec"]["x"] = value;
            } else {
                json["spec"]["layout"]["spec"]["items"][0]["spec"]
                    .as_object_mut()
                    .unwrap()
                    .remove("x");
            }
            assert!(
                parse_grafana_dashboard(&json.to_string())
                    .unwrap_err()
                    .to_string()
                    .contains("spec.layout.spec.items[0].spec.x")
            );
        }
    }

    #[test]
    fn rejects_missing_v2_element_name() {
        let mut json = valid_v2_resource();
        json["spec"]["layout"]["spec"]["items"][0]["spec"]["element"]
            .as_object_mut()
            .unwrap()
            .remove("name");
        assert!(
            parse_grafana_dashboard(&json.to_string())
                .unwrap_err()
                .to_string()
                .contains("spec.layout.spec.items[0].spec.element.name")
        );
    }

    #[test]
    fn rejects_unresolved_v2_element_name() {
        let mut json = valid_v2_resource();
        json["spec"]["layout"]["spec"]["items"][0]["spec"]["element"]["name"] =
            serde_json::json!("absent");
        assert!(
            parse_grafana_dashboard(&json.to_string())
                .unwrap_err()
                .to_string()
                .contains("spec.layout.spec.items[0].spec.element.name")
        );
    }

    /// `v2_grafana13_export.yaml` is `v2_grafana13_export.json` re-serialized as
    /// YAML, the other format Grafana's "Export as code" offers for V2 resources.
    #[test]
    fn v2_yaml_resource_imports_like_its_json_equivalent() {
        let json = parse_grafana_dashboard(include_str!(
            "../tests/fixtures/grafana/v2_grafana13_export.json"
        ))
        .unwrap();
        let yaml = parse_grafana_dashboard(include_str!(
            "../tests/fixtures/grafana/v2_grafana13_export.yaml"
        ))
        .unwrap();

        assert_eq!(yaml.title, json.title);
        assert_eq!(yaml.layout, json.layout);
        assert_eq!(yaml.vars, json.vars);
        assert_eq!(yaml.query_vars, json.query_vars);
        assert_eq!(yaml.refresh_rate_ms, json.refresh_rate_ms);
        assert_eq!(yaml.diagnostics, json.diagnostics);
        let exprs = |import: &DashboardImport| -> Vec<Vec<String>> {
            import
                .queries
                .iter()
                .map(|query| query.exprs.clone())
                .collect()
        };
        assert_eq!(exprs(&yaml), exprs(&json));
    }

    #[test]
    fn document_format_follows_file_extension() {
        for (path, format) in [
            ("dash.json", DocumentFormat::Json),
            ("dash.JSON", DocumentFormat::Json),
            ("dash.yaml", DocumentFormat::Yaml),
            ("dash.yml", DocumentFormat::Yaml),
            ("dash", DocumentFormat::Detect),
            ("dash.txt", DocumentFormat::Detect),
        ] {
            assert_eq!(
                DocumentFormat::from_path(std::path::Path::new(path)),
                format,
                "{path}"
            );
        }
    }

    #[test]
    fn json_files_report_json_errors_without_yaml_fallback() {
        let error = parse_document(
            "apiVersion: dashboard.grafana.app/v2\n",
            DocumentFormat::Json,
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("parsing Grafana dashboard JSON"));
    }

    #[test]
    fn undetectable_documents_report_both_parse_errors() {
        let error = parse_grafana_dashboard("{ not: [valid")
            .unwrap_err()
            .to_string();
        assert!(error.contains("not valid JSON"), "{error}");
        assert!(error.contains("or YAML"), "{error}");
    }

    #[test]
    fn yaml_documents_must_be_mappings() {
        let error = parse_document("- a\n- b\n", DocumentFormat::Yaml).unwrap_err();
        assert!(error.to_string().contains("expected a mapping"), "{error}");
    }

    #[test]
    fn classic_import_runs_through_normalized_model_without_semantic_changes() {
        let dashboard = parse_grafana_dashboard(
            r#"{
          "title":"Classic parity",
          "refresh":"15s",
          "templating":{"list":[{
            "name":"job",
            "type":"query",
            "query":"label_values(up, job)",
            "current":{"text":"api","value":"api"},
            "regex":""
          }]},
          "panels":[{
            "type":"timeseries",
            "title":"Requests",
            "gridPos":{"x":1,"y":2,"w":12,"h":8},
            "targets":[
              {"expr":"hidden","hide":true},
              {"expr":"rate(requests_total{job=\"$job\"}[5m])","legendFormat":"{{instance}}","instant":false}
            ],
            "fieldConfig":{"defaults":{"unit":"reqps","decimals":1,"min":0,"max":100}},
            "options":{"reduceOptions":{"calcs":["max"]}}
          }]
        }"#,
        )
        .unwrap();

        assert_eq!(dashboard.title, "Classic parity");
        assert_eq!(dashboard.refresh_rate_ms, Some(15_000));
        assert_eq!(dashboard.vars.get("job").map(String::as_str), Some("api"));
        assert_eq!(dashboard.query_vars[0].query, "label_values(up, job)");
        assert_eq!(dashboard.queries.len(), 1);
        let panel = &dashboard.queries[0];
        assert_eq!(panel.title, "Requests");
        assert_eq!(panel.exprs, ["rate(requests_total{job=\"$job\"}[5m])"]);
        assert_eq!(panel.expr_paths, ["panels[0].targets[1].expr"]);
        assert_eq!(panel.legends, [Some("{{instance}}".to_string())]);
        assert_eq!(panel.query_modes, [crate::app::QueryMode::Range]);
        assert_eq!((panel.grid.unwrap().x, panel.grid.unwrap().y), (1, 2));
        assert_eq!((panel.grid.unwrap().w, panel.grid.unwrap().h), (12, 8));
        assert_eq!(panel.display.unit.as_deref(), Some("reqps"));
        assert_eq!(panel.display.decimals, Some(1));
        assert_eq!((panel.min, panel.max), (Some(0.0), Some(100.0)));
        assert_eq!(dashboard.diagnostics[0].code, "ignored_field");
        assert_eq!(
            dashboard.diagnostics[0].path,
            "panels[0].options.reduceOptions"
        );
    }

    /// What an import means for the dashboard, leaving out source paths, which
    /// differ between the Classic and V2 formats.
    fn import_semantics(import: &DashboardImport) -> String {
        let panels: Vec<String> = import
            .queries
            .iter()
            .map(|panel| {
                format!(
                    "{:?}",
                    (
                        &panel.title,
                        &panel.exprs,
                        &panel.legends,
                        &panel.query_modes,
                        panel.grid.map(|grid| (grid.x, grid.y, grid.w, grid.h)),
                        &panel.panel_type,
                        &panel.thresholds,
                        (panel.min, panel.max, panel.autogrid),
                        &panel.display,
                        &panel.options,
                    )
                )
            })
            .collect();
        let mut vars: Vec<_> = import.vars.iter().collect();
        vars.sort();
        let query_vars: Vec<_> = import
            .query_vars
            .iter()
            .map(|var| (&var.name, &var.query, &var.regex, var.select_all))
            .collect();
        let diagnostics: Vec<_> = import
            .diagnostics
            .iter()
            .map(|diagnostic| (&diagnostic.code, &diagnostic.message))
            .collect();
        format!(
            "title: {}\nrefresh: {:?}\nskipped: {}\nvars: {vars:?}\nquery vars: {query_vars:?}\nlayout: {:?}\ndiagnostics: {diagnostics:?}\npanels:\n{}",
            import.title,
            import.refresh_rate_ms,
            import.skipped_panels,
            import.layout,
            panels.join("\n")
        )
    }

    /// The V2 example dashboards are Grafana 13.2.3's own conversions of the
    /// Classic dashboards kept in `tests/fixtures/grafana/classic_examples`, so
    /// both formats must import to the same dashboard.
    #[test]
    fn classic_examples_and_their_v2_conversions_import_identically() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for (classic, example) in [
            (
                "all_visualizations.json",
                "examples/dashboards/all_visualizations.json",
            ),
            (
                "instant_queries.json",
                "examples/dashboards/instant_queries.json",
            ),
            (
                "prometheus_demo.json",
                "examples/dashboards/prometheus_demo.json",
            ),
            ("simple_test.json", "examples/dashboards/simple_test.json"),
            (
                "thresholds_demo.json",
                "examples/dashboards/thresholds_demo.json",
            ),
            ("vllm_demo.json", "examples/demo/vllm_demo.json"),
            ("vllm_grafana.json", "examples/demo/vllm/grafana.json"),
        ] {
            let classic = load_grafana_dashboard(
                &root
                    .join("tests/fixtures/grafana/classic_examples")
                    .join(classic),
            )
            .unwrap();
            let v2 = load_grafana_dashboard(&root.join(example)).unwrap();

            assert_eq!(
                import_semantics(&v2),
                import_semantics(&classic),
                "{example}"
            );
        }
    }

    #[test]
    fn classic_repeats_are_recorded_and_saved_copies_are_skipped() {
        let dashboard = parse_grafana_dashboard(
            r#"{
                "title": "Classic repeats",
                "templating": {"list": [{
                    "name": "instance",
                    "type": "custom",
                    "query": "a,b\\,c,label : d",
                    "multi": true,
                    "includeAll": true,
                    "current": {"text": ["All"], "value": ["$__all"]}
                }]},
                "panels": [
                    {
                        "type": "timeseries",
                        "title": "CPU $instance",
                        "repeat": "instance",
                        "repeatDirection": "v",
                        "maxPerRow": 3,
                        "gridPos": {"x": 0, "y": 0, "w": 12, "h": 4},
                        "targets": [{"expr": "up{instance=~\"$instance\"}"}]
                    },
                    {
                        "type": "timeseries",
                        "title": "CPU b",
                        "repeatPanelId": 1,
                        "gridPos": {"x": 12, "y": 0, "w": 12, "h": 4},
                        "targets": [{"expr": "up"}]
                    },
                    {
                        "type": "row",
                        "title": "Row $instance",
                        "repeat": "instance",
                        "collapsed": true,
                        "gridPos": {"x": 0, "y": 4, "w": 24, "h": 1},
                        "panels": []
                    }
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(dashboard.queries.len(), 1);
        assert_eq!(
            dashboard.repeats.panels.get(&0),
            Some(&crate::dashboard::Repeat {
                variable: "instance".to_string(),
                direction: crate::dashboard::RepeatDirection::Vertical,
                max_per_row: Some(3),
            })
        );
        assert_eq!(
            dashboard.repeats.rows.get(&crate::dashboard::RowId::new(0)),
            Some(&crate::dashboard::Repeat::new("instance"))
        );
        // Custom options split on unescaped commas, and `text : value` keeps the value.
        assert_eq!(
            dashboard.var_values.get("instance"),
            Some(&vec!["a".to_string(), "b,c".to_string(), "d".to_string()])
        );
        assert_eq!(
            dashboard.vars.get("instance").map(String::as_str),
            Some("(a|b,c|d)")
        );
        assert!(dashboard.regex_vars.contains("instance"));
    }

    #[test]
    fn test_parse_dashboard_vars() {
        let json = r#"
        {
            "title": "Test Dash",
            "templating": {
                "list": [
                    {
                        "name": "job",
                        "current": { "text": "node-exporter", "value": "node-exporter" }
                    },
                    {
                        "name": "instance",
                        "current": { "text": "All", "value": ["server1", "server2"] }
                    }
                ]
            }
        }
        "#;

        let dashboard = parse_grafana_dashboard(json).unwrap();

        assert_eq!(dashboard.title, "Test Dash");
        assert_eq!(
            dashboard.vars.get("job"),
            Some(&"node-exporter".to_string())
        );
        // Several selected values become a regex alternation, as in Grafana.
        assert_eq!(
            dashboard.vars.get("instance").map(String::as_str),
            Some("(server1|server2)")
        );
        assert_eq!(
            dashboard.var_values.get("instance"),
            Some(&vec!["server1".to_string(), "server2".to_string()])
        );
    }

    #[test]
    fn test_parse_axis_grid_show() {
        let json = r#"
        {
            "title": "Grid Test",
            "panels": [
                {
                    "type": "timeseries",
                    "title": "Grid Off",
                    "targets": [{ "expr": "up" }],
                    "fieldConfig": {
                        "defaults": {
                            "custom": {
                                "axisGridShow": false
                            }
                        }
                    }
                },
                {
                    "type": "timeseries",
                    "title": "Grid Default",
                    "targets": [{ "expr": "up" }]
                }
            ]
        }
        "#;
        let path = std::env::temp_dir().join("grafatui-axis-grid-test.json");
        std::fs::write(&path, json).unwrap();

        let dashboard = load_grafana_dashboard(&path).unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(dashboard.queries[0].autogrid, Some(false));
        assert_eq!(dashboard.queries[1].autogrid, None);
    }

    #[test]
    fn test_parse_field_display_format() {
        let json = r#"
        {
            "title": "Display Format Test",
            "panels": [
                {
                    "type": "stat",
                    "title": "Memory",
                    "targets": [{ "expr": "process_resident_memory_bytes" }],
                    "fieldConfig": {
                        "defaults": {
                            "unit": "bytes",
                            "decimals": 1,
                            "noValue": "n/a"
                        }
                    }
                },
                {
                    "type": "stat",
                    "title": "Default",
                    "targets": [{ "expr": "up" }]
                }
            ]
        }
        "#;
        let path = std::env::temp_dir().join("grafatui-display-format-test.json");
        std::fs::write(&path, json).unwrap();

        let dashboard = load_grafana_dashboard(&path).unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(dashboard.queries[0].display.unit.as_deref(), Some("bytes"));
        assert_eq!(dashboard.queries[0].display.decimals, Some(1));
        assert_eq!(
            dashboard.queries[0].display.no_value.as_deref(),
            Some("n/a")
        );
        assert_eq!(dashboard.queries[1].display.unit, None);
        assert_eq!(dashboard.queries[1].display.decimals, None);
        assert_eq!(dashboard.queries[1].display.no_value, None);
    }

    #[test]
    fn classic_query_options_set_panel_resolution() {
        let dashboard = parse_grafana_dashboard(
            r#"{
                "title": "Resolution",
                "panels": [
                    {
                        "type": "timeseries",
                        "title": "Tuned",
                        "interval": ">1m",
                        "maxDataPoints": 300,
                        "targets": [
                            { "expr": "up", "interval": "30s" },
                            { "expr": "hidden", "hide": true, "interval": "5m" },
                            { "expr": "node_load1", "interval": "" },
                            { "expr": "rate(x[5m])", "interval": "$min_step" }
                        ]
                    },
                    {
                        "type": "timeseries",
                        "title": "Defaults",
                        "maxDataPoints": "120",
                        "targets": [{ "expr": "up" }]
                    }
                ]
            }"#,
        )
        .unwrap();

        let tuned = &dashboard.queries[0].resolution;
        assert_eq!(tuned.min_interval.as_deref(), Some(">1m"));
        assert_eq!(tuned.max_data_points, Some(300));
        // Hidden targets are dropped, so intervals stay parallel to exprs.
        assert_eq!(
            tuned.target_min_intervals,
            [Some("30s".to_string()), None, Some("$min_step".to_string())]
        );
        let defaults = &dashboard.queries[1].resolution;
        assert_eq!(defaults.min_interval, None);
        assert_eq!(defaults.max_data_points, Some(120));
        assert!(
            !dashboard
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.path.ends_with("interval"))
        );
    }

    #[test]
    fn invalid_min_intervals_are_dropped_with_a_diagnostic() {
        let dashboard = parse_grafana_dashboard(
            r#"{
                "title": "Resolution",
                "panels": [{
                    "type": "timeseries",
                    "title": "Typo",
                    "interval": "fast",
                    "targets": [{ "expr": "up", "interval": "10q" }]
                }]
            }"#,
        )
        .unwrap();

        let resolution = &dashboard.queries[0].resolution;
        assert_eq!(resolution.min_interval, None);
        assert_eq!(resolution.target_min_intervals, [None]);
        let interval_diagnostics = dashboard
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.path.ends_with("interval"))
            .collect::<Vec<_>>();
        assert_eq!(interval_diagnostics.len(), 2);
        assert!(
            interval_diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code == "ignored_field")
        );
        assert!(interval_diagnostics.iter().any(|diagnostic| {
            diagnostic.path.ends_with("].interval") && diagnostic.message.contains("`fast`")
        }));
        assert!(interval_diagnostics.iter().any(|diagnostic| {
            diagnostic.path.ends_with("targets[0].interval") && diagnostic.message.contains("`10q`")
        }));
    }

    #[test]
    fn v2_query_options_set_panel_resolution() {
        let mut json = valid_v2_resource();
        make_v2_panel_importable(&mut json);
        let data = &mut json["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"];
        data["queryOptions"] = serde_json::json!({"interval": "2m", "maxDataPoints": 500});
        data["queries"][0]["spec"]["query"]["spec"]["interval"] = serde_json::json!("1m");

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        let resolution = &dashboard.queries[0].resolution;
        assert_eq!(resolution.min_interval.as_deref(), Some("2m"));
        assert_eq!(resolution.max_data_points, Some(500));
        assert_eq!(resolution.target_min_intervals, [Some("1m".to_string())]);
    }

    #[test]
    fn v2_null_query_options_are_empty() {
        let mut json = valid_v2_resource();
        make_v2_panel_importable(&mut json);
        json["spec"]["elements"]["panel-1"]["spec"]["data"]["spec"]["queryOptions"] =
            serde_json::Value::Null;

        let dashboard = parse_grafana_dashboard(&json.to_string()).unwrap();

        assert_eq!(
            dashboard.queries[0].resolution,
            crate::app::QueryResolution {
                target_min_intervals: vec![None],
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_target_instant_query_modes() {
        let json = r#"
        {
            "title": "Instant Mode Test",
            "panels": [
                {
                    "type": "timeseries",
                    "title": "Explicit Instant",
                    "targets": [
                        { "expr": "up", "instant": true },
                        { "expr": "rate(http_requests_total[5m])", "instant": false }
                    ]
                },
                {
                    "type": "gauge",
                    "title": "Gauge Default",
                    "targets": [{ "expr": "up" }]
                },
                {
                    "type": "bargauge",
                    "title": "Bar Gauge Default",
                    "targets": [{ "expr": "up" }]
                },
                {
                    "type": "table",
                    "title": "Table Default",
                    "targets": [{ "expr": "up" }]
                },
                {
                    "type": "stat",
                    "title": "Stat Default",
                    "targets": [{ "expr": "up" }]
                },
                {
                    "type": "gauge",
                    "title": "Gauge Range Override",
                    "targets": [{ "expr": "up", "instant": false }]
                }
            ]
        }
        "#;
        let path = std::env::temp_dir().join("grafatui-instant-mode-test.json");
        std::fs::write(&path, json).unwrap();

        let dashboard = load_grafana_dashboard(&path).unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(
            dashboard.queries[0].query_modes,
            vec![crate::app::QueryMode::Instant, crate::app::QueryMode::Range]
        );
        assert_eq!(
            dashboard.queries[1].query_modes,
            vec![crate::app::QueryMode::Instant]
        );
        assert_eq!(
            dashboard.queries[2].query_modes,
            vec![crate::app::QueryMode::Instant]
        );
        assert_eq!(
            dashboard.queries[3].query_modes,
            vec![crate::app::QueryMode::Instant]
        );
        assert_eq!(
            dashboard.queries[4].query_modes,
            vec![crate::app::QueryMode::Range]
        );
        assert_eq!(
            dashboard.queries[5].query_modes,
            vec![crate::app::QueryMode::Range]
        );
    }

    #[test]
    fn test_parse_query_variables() {
        let json = r#"
        {
            "title": "Query Vars",
            "templating": {
                "list": [
                    {
                        "name": "instance",
                        "query": "label_values(up, instance)",
                        "type": "query",
                        "regex": "/(.+)/",
                        "includeAll": false,
                        "current": { "text": "node-1", "value": "node-1" }
                    },
                    {
                        "name": "model",
                        "query": { "query": "label_values(model_name)" },
                        "type": "query",
                        "current": { "text": "llama", "value": "llama" }
                    },
                    {
                        "name": "all_instance",
                        "query": "label_values(up, instance)",
                        "type": "query",
                        "allValue": ".*",
                        "current": { "text": "All", "value": "$__all" }
                    }
                ]
            }
        }
        "#;
        let path = std::env::temp_dir().join("grafatui-query-vars-test.json");
        std::fs::write(&path, json).unwrap();

        let dashboard = load_grafana_dashboard(&path).unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(dashboard.query_vars.len(), 3);
        assert_eq!(dashboard.query_vars[0].query, "label_values(up, instance)");
        assert_eq!(dashboard.query_vars[0].regex.as_deref(), Some("/(.+)/"));
        assert!(!dashboard.query_vars[0].select_all);
        assert_eq!(dashboard.query_vars[1].query, "label_values(model_name)");
        // `All` still resolves its options, so repeats can iterate them.
        assert!(dashboard.query_vars[2].select_all);
        assert_eq!(dashboard.query_vars[2].all_value.as_deref(), Some(".*"));
        assert_eq!(dashboard.vars.get("all_instance"), Some(&".*".to_string()));
    }

    #[test]
    fn test_parse_dashboard_refresh_duration() {
        let json = r#"
        {
            "title": "Refresh Dash",
            "refresh": "5s",
            "panels": [
                {
                    "type": "timeseries",
                    "title": "Up",
                    "targets": [{ "expr": "up" }]
                }
            ]
        }
        "#;
        let path = std::env::temp_dir().join("grafatui-refresh-test.json");
        std::fs::write(&path, json).unwrap();

        let dashboard = load_grafana_dashboard(&path).unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(dashboard.refresh_rate_ms, Some(5000));
    }

    #[test]
    fn test_import_timeseries_graph_options() {
        let json = r#"{
            "title": "Graph options",
            "panels": [{
                "type": "timeseries",
                "title": "Area points",
                "targets": [{ "expr": "up" }],
                "fieldConfig": {
                    "defaults": {
                        "custom": {
                            "drawStyle": "line",
                            "showPoints": "always",
                            "fillOpacity": 20,
                            "axisPlacement": "hidden",
                            "lineInterpolation": "smooth",
                            "stacking": { "mode": "normal" }
                        }
                    }
                }
            }]
        }"#;

        let out = parse_grafana_dashboard(json).unwrap();

        let options = match &out.queries[0].options {
            crate::app::PanelOptions::Graph(options) => options,
            other => panic!("expected graph options, got {other:?}"),
        };
        assert_eq!(options.draw_style, crate::app::GraphDrawStyle::Line);
        assert_eq!(options.show_points, crate::app::GraphPointMode::Always);
        assert_eq!(options.fill_opacity, Some(20));
        assert_eq!(
            options.axis_placement,
            crate::app::GraphAxisPlacement::Hidden
        );
        assert_eq!(options.line_interpolation.as_deref(), Some("smooth"));
        assert_eq!(options.stacking, crate::app::GraphStackingMode::Normal);
    }

    #[test]
    fn test_import_graph_options_fallbacks_and_non_graph_none() {
        let json = r#"{
            "title": "Fallbacks",
            "panels": [
                {
                    "type": "timeseries",
                    "title": "Unknown values",
                    "targets": [{ "expr": "up" }],
                    "fieldConfig": {
                        "defaults": {
                            "custom": {
                                "drawStyle": "candles",
                                "showPoints": "sometimes",
                                "fillOpacity": 999,
                                "axisPlacement": "right",
                                "stacking": { "mode": "percent" }
                            }
                        }
                    }
                },
                {
                    "type": "stat",
                    "title": "Stat",
                    "targets": [{ "expr": "up" }]
                }
            ]
        }"#;

        let out = parse_grafana_dashboard(json).unwrap();

        let graph_options = match &out.queries[0].options {
            crate::app::PanelOptions::Graph(options) => options,
            other => panic!("expected graph options, got {other:?}"),
        };
        assert_eq!(graph_options.draw_style, crate::app::GraphDrawStyle::Line);
        assert_eq!(graph_options.show_points, crate::app::GraphPointMode::Auto);
        assert_eq!(graph_options.fill_opacity, Some(100));
        assert_eq!(
            graph_options.axis_placement,
            crate::app::GraphAxisPlacement::Visible
        );
        assert_eq!(
            graph_options.stacking,
            crate::app::GraphStackingMode::Percent
        );
        assert_eq!(out.queries[1].options, crate::app::PanelOptions::None);
    }

    #[test]
    fn test_import_diagnostics_report_skipped_panel_type() {
        let json = r#"{
            "title": "Skipped",
            "panels": [
                { "type": "text", "title": "Notes" }
            ]
        }"#;
        let path = std::env::temp_dir().join("grafatui-skipped-panel-diagnostics.json");
        std::fs::write(&path, json).unwrap();

        let dashboard = load_grafana_dashboard(&path).unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(dashboard.skipped_panels, 1);
        assert_eq!(dashboard.diagnostics.len(), 1);
        assert_eq!(dashboard.diagnostics[0].code, "skipped_panel");
        assert_eq!(dashboard.diagnostics[0].path, "panels[0]");
        assert!(
            dashboard.diagnostics[0]
                .message
                .contains("unsupported panel type `text`")
        );
        assert!(dashboard.diagnostics[0].message.contains("Notes"));
    }

    #[test]
    fn test_import_diagnostics_report_ignored_high_impact_fields() {
        let json = r#"{
            "title": "Ignored Fields",
            "panels": [
                {
                    "type": "stat",
                    "title": "CPU",
                    "targets": [
                        { "expr": "up" }
                    ],
                    "fieldConfig": {
                        "defaults": {
                            "mappings": [
                                { "type": "value", "options": { "0": { "text": "Down" } } }
                            ]
                        }
                    },
                    "options": {
                        "reduceOptions": {
                            "calcs": ["mean"]
                        }
                    }
                }
            ]
        }"#;
        let path = std::env::temp_dir().join("grafatui-ignored-fields-diagnostics.json");
        std::fs::write(&path, json).unwrap();

        let dashboard = load_grafana_dashboard(&path).unwrap();
        std::fs::remove_file(path).unwrap();

        let diagnostics: Vec<_> = dashboard
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code.as_str(), diagnostic.path.as_str()))
            .collect();

        assert!(
            diagnostics.contains(&("ignored_field", "panels[0].fieldConfig.defaults.mappings"))
        );
        assert!(diagnostics.contains(&("ignored_field", "panels[0].options.reduceOptions")));
    }

    #[test]
    fn test_hidden_targets_are_not_imported_or_warned() {
        let json = r#"{
            "title": "Hidden Targets",
            "panels": [
                {
                    "type": "timeseries",
                    "title": "CPU",
                    "targets": [
                        { "expr": "helper_query", "hide": true },
                        { "expr": "visible_query" }
                    ]
                }
            ]
        }"#;
        let path = std::env::temp_dir().join("grafatui-hidden-targets-test.json");
        std::fs::write(&path, json).unwrap();

        let dashboard = load_grafana_dashboard(&path).unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(dashboard.queries.len(), 1);
        assert_eq!(dashboard.queries[0].exprs, vec!["visible_query"]);
        assert_eq!(
            dashboard.queries[0].expr_paths,
            vec!["panels[0].targets[1].expr"]
        );
        assert!(
            dashboard
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.path != "panels[0].targets[0].hide")
        );
    }

    #[test]
    fn test_import_diagnostics_preserve_nested_row_paths() {
        let json = r#"{
            "title": "Rows",
            "panels": [
                {
                    "type": "row",
                    "title": "Group",
                    "panels": [
                        { "type": "piechart", "title": "Pie" }
                    ]
                }
            ]
        }"#;
        let path = std::env::temp_dir().join("grafatui-nested-row-diagnostics.json");
        std::fs::write(&path, json).unwrap();

        let dashboard = load_grafana_dashboard(&path).unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(dashboard.diagnostics[0].code, "skipped_panel");
        assert_eq!(dashboard.diagnostics[0].path, "panels[0].panels[0]");
    }

    #[test]
    fn test_variable_diagnostics_report_modifiers_and_unresolved_variables() {
        let json = r#"{
            "title": "Variables",
            "templating": {
                "list": [
                    { "name": "job", "current": { "text": "node", "value": "node" } },
                    { "name": "instance", "current": { "text": "server", "value": "server" } },
                    {
                        "name": "query_var",
                        "type": "query",
                        "query": "label_values(up{job=\"$job\"}, instance)",
                        "current": { "text": "server", "value": "server" }
                    }
                ]
            },
            "panels": [
                {
                    "type": "timeseries",
                    "title": "CPU",
                    "targets": [
                        { "expr": "up{job=\"$job\", instance=\"${instance}\"}" },
                        { "expr": "up{job=~\"${job:regex}\", cluster=\"$cluster\", interval=\"$__interval\", range=\"$__range_s\"}" }
                    ]
                }
            ]
        }"#;
        let path = std::env::temp_dir().join("grafatui-variable-diagnostics.json");
        std::fs::write(&path, json).unwrap();

        let dashboard = load_grafana_dashboard(&path).unwrap();
        std::fs::remove_file(path).unwrap();

        let diagnostics = variable_diagnostics(&dashboard, &dashboard.vars);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "unsupported_variable_modifier"
                && diagnostic.path == "panels[0].targets[1].expr"
                && diagnostic.message.contains("${job:regex}")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "unresolved_variable"
                && diagnostic.path == "panels[0].targets[1].expr"
                && diagnostic.message.contains("$cluster")
        }));
    }
}
