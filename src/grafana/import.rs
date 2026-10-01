use anyhow::Result;

use super::{DashboardImport, GridPos, ImportDiagnostic, QueryPanel, TemplateQueryVar, model};

pub(super) fn finish(dashboard: model::Dashboard) -> Result<DashboardImport> {
    let model::Dashboard {
        title,
        refresh,
        variables,
        layout,
        skipped_panels,
        diagnostics,
    } = dashboard;
    let mut out = DashboardImport {
        title,
        refresh_rate_ms: refresh.as_deref().and_then(parse_refresh_rate_ms),
        skipped_panels,
        diagnostics,
        ..DashboardImport::default()
    };
    let variable_names = variables
        .iter()
        .map(|variable| variable.name.clone())
        .collect();
    import_variables(&mut out, variables);
    let mut ids = LayoutIds {
        variable_names,
        ..LayoutIds::default()
    };
    out.layout =
        crate::dashboard::DashboardLayout::new(import_layout_nodes(layout, &mut out, &mut ids)?);
    Ok(out)
}

fn import_variables(out: &mut DashboardImport, variables: Vec<model::Variable>) {
    for variable in variables {
        let select_all = current_is_all(variable.current.as_ref());
        let regex_values = variable.multi || variable.include_all;
        if regex_values {
            out.regex_vars.insert(variable.name.clone());
        }
        let mut selected = selected_values(variable.current.as_ref());
        if selected.is_empty() && !select_all && variable.kind.as_deref() != Some("query") {
            // Grafana selects the first option when nothing is saved.
            selected.extend(variable.options.first().cloned());
        }
        if select_all {
            // `All` covers every option; known options let repeats iterate them now,
            // and dynamic query variables fill them in when they resolve.
            if !variable.options.is_empty() {
                out.var_values
                    .insert(variable.name.clone(), variable.options.clone());
            }
            let value = variable.all_value.clone().unwrap_or_else(|| {
                if variable.options.is_empty() {
                    ".*".to_string()
                } else {
                    crate::app::format_prometheus_values(&variable.options, true)
                }
            });
            out.vars.insert(variable.name.clone(), value);
        } else if !selected.is_empty() {
            out.var_values
                .insert(variable.name.clone(), selected.clone());
            out.vars.insert(
                variable.name.clone(),
                crate::app::format_prometheus_values(&selected, regex_values),
            );
        }

        // An explicit multi-value selection is kept as chosen; otherwise query
        // variables resolve against Prometheus like Grafana does on load.
        if variable.kind.as_deref() == Some("query")
            && selected.len() <= 1
            && let (Some(query), Some(query_path)) = (variable.query, variable.query_path)
        {
            out.query_vars.push(TemplateQueryVar {
                name: variable.name,
                query,
                regex: variable.regex.filter(|regex| !regex.trim().is_empty()),
                query_path,
                select_all,
                all_value: variable.all_value,
                regex_values,
            });
        }
    }
}

/// Non-empty values selected by a variable's `current` value, or its text as a
/// fallback, excluding Grafana's `$__all` marker.
fn selected_values(current: Option<&model::VariableCurrent>) -> Vec<String> {
    let Some(current) = current else {
        return Vec::new();
    };
    let values = |value: Option<&serde_json::Value>| -> Vec<String> {
        let values = match value {
            Some(serde_json::Value::String(value)) => vec![value.clone()],
            Some(serde_json::Value::Number(value)) => vec![value.to_string()],
            Some(serde_json::Value::Array(values)) => values
                .iter()
                .filter_map(|value| match value {
                    serde_json::Value::String(value) => Some(value.clone()),
                    serde_json::Value::Number(value) => Some(value.to_string()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        values
            .into_iter()
            .filter(|value| !value.is_empty() && value != "$__all")
            .collect()
    };
    let selected = values(current.value.as_ref());
    if selected.is_empty() {
        values(current.text.as_ref())
    } else {
        selected
    }
}

/// Whether `All` is selected: the value is Grafana's `$__all` marker, or, when
/// there is no value, the text says `All`. An explicit value list wins over text.
fn current_is_all(current: Option<&model::VariableCurrent>) -> bool {
    current.is_some_and(|current| match current.value.as_ref() {
        None | Some(serde_json::Value::Null) => value_is_all(current.text.as_ref()),
        Some(serde_json::Value::String(value)) if value.is_empty() => {
            value_is_all(current.text.as_ref())
        }
        value => value_is_all(value),
    })
}

fn value_is_all(value: Option<&serde_json::Value>) -> bool {
    match value {
        Some(serde_json::Value::String(value)) => {
            value == "$__all" || value.eq_ignore_ascii_case("all")
        }
        Some(serde_json::Value::Array(values)) => {
            values.iter().any(|value| value_is_all(Some(value)))
        }
        _ => false,
    }
}

fn import_panel(panel: model::Panel, out: &mut DashboardImport) -> Result<Option<usize>> {
    let panel_type = match panel.kind.as_str() {
        "graph" | "timeseries" => crate::app::PanelType::Graph,
        "stat" => crate::app::PanelType::Stat,
        "gauge" => crate::app::PanelType::Gauge,
        "bargauge" => crate::app::PanelType::BarGauge,
        "table" => crate::app::PanelType::Table,
        "heatmap" => crate::app::PanelType::Heatmap,
        _ => crate::app::PanelType::Unknown,
    };

    if panel_type == crate::app::PanelType::Unknown {
        if !panel.kind.is_empty() && panel.kind != "row" {
            out.skipped_panels += 1;
            out.diagnostics.push(ImportDiagnostic::new(
                "skipped_panel",
                panel.source_path,
                format!(
                    "unsupported panel type `{}` skipped for panel `{}`",
                    panel.kind, panel.title
                ),
            ));
        }
        return Ok(None);
    }

    let mut exprs = Vec::new();
    let mut expr_paths = Vec::new();
    let mut legends = Vec::new();
    let mut query_modes = Vec::new();
    for target in panel.targets {
        if target.hidden {
            continue;
        }
        if let Some(expr) = target.expr {
            exprs.push(expr);
            expr_paths.push(target.expr_path);
            legends.push(target.legend_format);
            query_modes.push(query_mode_for_target(target.instant, panel_type));
        }
    }

    let mut thresholds = None;
    let mut min = None;
    let mut max = None;
    let mut autogrid = None;
    let mut display = crate::ui::DisplayFormat::default();
    let mut graph_options = crate::app::GraphOptions::default();

    if let Some(path) = panel.transformations_path {
        out.diagnostics.push(ImportDiagnostic::new(
            "ignored_field",
            path,
            "`transformations` are not supported yet; queries will run without Grafana transformations",
        ));
    }

    if let Some(path) = panel.reduce_options_path {
        out.diagnostics.push(ImportDiagnostic::new(
            "ignored_field",
            path,
            "`options.reduceOptions` is not supported yet; Grafatui will use default value selection",
        ));
    }

    if let Some(defaults) = panel.field_defaults {
        if let Some(path) = defaults.mappings_path {
            out.diagnostics.push(ImportDiagnostic::new(
                    "ignored_field",
                    path,
                    "`fieldConfig.defaults.mappings` is not supported yet; value mappings will be ignored",
                ));
        }
        graph_options = graph_options_from_custom(defaults.custom.as_ref());
        display = crate::ui::DisplayFormat {
            unit: defaults.unit,
            decimals: defaults.decimals,
            no_value: defaults.no_value,
        };
        min = defaults.min;
        max = defaults.max;
        autogrid = defaults
            .custom
            .as_ref()
            .and_then(|custom| custom.axis_grid_show);
        thresholds = thresholds_from_model(defaults.thresholds, defaults.custom.as_ref());
    }

    if !exprs.is_empty() {
        let options = match panel_type {
            crate::app::PanelType::Graph => crate::app::PanelOptions::Graph(graph_options),
            _ => crate::app::PanelOptions::None,
        };
        let index = out.queries.len();
        out.queries.push(QueryPanel {
            title: panel.title,
            exprs,
            expr_paths,
            legends,
            query_modes,
            grid: panel.grid.map(|grid| GridPos {
                x: grid.x,
                y: grid.y,
                w: grid.w,
                h: grid.h,
            }),
            panel_type,
            thresholds,
            min,
            max,
            autogrid,
            display,
            options,
        });
        Ok(Some(index))
    } else {
        if panel.count_as_skipped_if_empty {
            out.skipped_panels += 1;
        }
        Ok(None)
    }
}

fn import_layout_nodes(
    nodes: Vec<model::LayoutNode>,
    out: &mut DashboardImport,
    ids: &mut LayoutIds,
) -> Result<Vec<crate::dashboard::DashboardLayoutItem>> {
    let mut items = Vec::new();
    for node in nodes {
        match node {
            model::LayoutNode::Panel(mut panel) => {
                let repeat = ids.checked_repeat(panel.repeat.take(), &panel.source_path, out);
                if let Some(index) = import_panel(panel, out)? {
                    if let Some(repeat) = repeat {
                        out.repeats.panels.insert(index, repeat);
                    }
                    items.push(crate::dashboard::DashboardLayoutItem::Panel(index));
                }
            }
            model::LayoutNode::Row(row) => {
                let id = crate::dashboard::RowId::new(ids.next_row);
                ids.next_row += 1;
                if let Some(repeat) = ids.checked_repeat(row.repeat, &row.source_path, out) {
                    out.repeats.rows.insert(id, repeat);
                }
                let children = import_layout_nodes(row.children, out, ids)?;
                items.push(crate::dashboard::DashboardLayoutItem::Row(
                    crate::dashboard::DashboardRow::new(
                        id,
                        row.title,
                        row.collapsed,
                        row.hidden_header,
                        children,
                    ),
                ));
            }
            model::LayoutNode::Tabs(group) => {
                let id = crate::dashboard::TabGroupId::new(ids.next_tabs);
                ids.next_tabs += 1;
                let mut tabs = Vec::with_capacity(group.tabs.len());
                for (index, tab) in group.tabs.into_iter().enumerate() {
                    if let Some(repeat) = ids.checked_repeat(tab.repeat, &tab.source_path, out) {
                        out.repeats.tabs.insert((id, index), repeat);
                    }
                    tabs.push(crate::dashboard::DashboardTab {
                        title: tab.title,
                        children: import_layout_nodes(tab.children, out, ids)?,
                    });
                }
                items.push(crate::dashboard::DashboardLayoutItem::Tabs(
                    crate::dashboard::DashboardTabs::new(id, tabs),
                ));
            }
            model::LayoutNode::AutoGrid(grid) => {
                let mut panels = Vec::with_capacity(grid.panels.len());
                for mut panel in grid.panels {
                    let repeat = ids.checked_repeat(panel.repeat.take(), &panel.source_path, out);
                    if let Some(index) = import_panel(panel, out)? {
                        if let Some(repeat) = repeat {
                            out.repeats.panels.insert(index, repeat);
                        }
                        panels.push(index);
                    }
                }
                if !panels.is_empty() {
                    items.push(crate::dashboard::DashboardLayoutItem::AutoGrid(
                        crate::dashboard::DashboardAutoGrid {
                            panels,
                            max_columns: grid.max_columns,
                            min_column_width: auto_grid_column_cells(grid.column_width_px),
                            row_height: auto_grid_row_units(grid.row_height_px),
                        },
                    ));
                }
            }
        }
    }
    Ok(items)
}

/// Grafana CSS pixels per terminal column, used to size auto grid columns.
///
/// This is Grafana's 8px design-system spacing unit, close to a typical monospace
/// cell width, so column counts match what Grafana shows at a similar pixel width.
const AUTO_GRID_PX_PER_COLUMN: f64 = 8.0;
/// Height of one fixed-grid unit in Grafana: a 30px cell plus its 8px margin.
const GRID_UNIT_PX: f64 = 38.0;
const GRID_MARGIN_PX: f64 = 8.0;

fn auto_grid_column_cells(width_px: f64) -> u16 {
    (width_px / AUTO_GRID_PX_PER_COLUMN).round().clamp(1.0, f64::from(u16::MAX)) as u16
}

/// Converts a pixel height to the fixed-grid units used by `GridPos::h`, where `h`
/// units span `h * 30px + (h - 1) * 8px`.
fn auto_grid_row_units(height_px: f64) -> u16 {
    ((height_px + GRID_MARGIN_PX) / GRID_UNIT_PX)
        .round()
        .clamp(1.0, f64::from(u16::MAX)) as u16
}

#[derive(Default)]
struct LayoutIds {
    next_row: usize,
    next_tabs: usize,
    variable_names: std::collections::HashSet<String>,
}

impl LayoutIds {
    /// Keeps a repeat whose variable the dashboard defines; otherwise the item is
    /// shown once, as Grafana does, with a diagnostic.
    fn checked_repeat(
        &self,
        repeat: Option<model::Repeat>,
        source_path: &str,
        out: &mut DashboardImport,
    ) -> Option<model::Repeat> {
        let repeat = repeat?;
        if self.variable_names.contains(&repeat.variable) {
            return Some(repeat);
        }
        out.diagnostics.push(ImportDiagnostic::new(
            "unknown_repeat_variable",
            source_path,
            format!(
                "repeat variable `{}` is not defined; the item is shown once",
                repeat.variable
            ),
        ));
        None
    }
}

fn query_mode_for_target(
    instant: Option<bool>,
    panel_type: crate::app::PanelType,
) -> crate::app::QueryMode {
    match instant {
        Some(true) => crate::app::QueryMode::Instant,
        Some(false) => crate::app::QueryMode::Range,
        None => default_query_mode_for_panel(panel_type),
    }
}

fn default_query_mode_for_panel(panel_type: crate::app::PanelType) -> crate::app::QueryMode {
    match panel_type {
        crate::app::PanelType::Gauge
        | crate::app::PanelType::BarGauge
        | crate::app::PanelType::Table => crate::app::QueryMode::Instant,
        _ => crate::app::QueryMode::Range,
    }
}

fn graph_options_from_custom(custom: Option<&model::GraphCustom>) -> crate::app::GraphOptions {
    let Some(custom) = custom else {
        return crate::app::GraphOptions::default();
    };
    crate::app::GraphOptions {
        draw_style: parse_graph_draw_style(custom.draw_style.as_deref()),
        show_points: parse_graph_point_mode(custom.show_points.as_deref()),
        fill_opacity: custom.fill_opacity.map(|value| value.min(100) as u8),
        axis_placement: parse_graph_axis_placement(custom.axis_placement.as_deref()),
        line_interpolation: custom
            .line_interpolation
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .cloned(),
        stacking: parse_graph_stacking_mode(custom.stacking_mode.as_deref()),
    }
}

fn thresholds_from_model(
    thresholds: Option<model::Thresholds>,
    custom: Option<&model::GraphCustom>,
) -> Option<crate::app::Thresholds> {
    let thresholds = thresholds?;
    let mode = match thresholds.mode.as_deref() {
        Some("percentage") => crate::app::ThresholdMode::Percentage,
        _ => crate::app::ThresholdMode::Absolute,
    };
    let mut steps: Vec<_> = thresholds
        .steps
        .into_iter()
        .map(|step| {
            let color = step.color.unwrap_or_else(|| "green".to_string());
            crate::app::ThresholdStep {
                value: step.value,
                color: crate::theme::parse_grafana_color(&color),
            }
        })
        .collect();
    steps.sort_by(|a, b| {
        let a_value = a.value.unwrap_or(f64::NEG_INFINITY);
        let b_value = b.value.unwrap_or(f64::NEG_INFINITY);
        a_value
            .partial_cmp(&b_value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    (!steps.is_empty()).then(|| crate::app::Thresholds {
        mode,
        steps,
        style: Some(
            custom
                .and_then(|custom| custom.thresholds_style_mode.clone())
                .unwrap_or_else(|| "line".to_string()),
        ),
    })
}

fn parse_graph_draw_style(value: Option<&str>) -> crate::app::GraphDrawStyle {
    match value {
        Some("points") => crate::app::GraphDrawStyle::Points,
        Some("bars") => crate::app::GraphDrawStyle::Bars,
        _ => crate::app::GraphDrawStyle::Line,
    }
}

fn parse_graph_point_mode(value: Option<&str>) -> crate::app::GraphPointMode {
    match value {
        Some("always") => crate::app::GraphPointMode::Always,
        Some("never") => crate::app::GraphPointMode::Never,
        _ => crate::app::GraphPointMode::Auto,
    }
}

fn parse_graph_axis_placement(value: Option<&str>) -> crate::app::GraphAxisPlacement {
    match value {
        Some("hidden") => crate::app::GraphAxisPlacement::Hidden,
        _ => crate::app::GraphAxisPlacement::Visible,
    }
}

fn parse_graph_stacking_mode(value: Option<&str>) -> crate::app::GraphStackingMode {
    match value {
        Some("normal") => crate::app::GraphStackingMode::Normal,
        Some("percent") => crate::app::GraphStackingMode::Percent,
        _ => crate::app::GraphStackingMode::Off,
    }
}

fn parse_refresh_rate_ms(refresh: &str) -> Option<u64> {
    let refresh = refresh.trim();
    if refresh.is_empty()
        || refresh.eq_ignore_ascii_case("false")
        || refresh.eq_ignore_ascii_case("off")
    {
        return None;
    }
    let duration = humantime::parse_duration(refresh).ok()?;
    u64::try_from(duration.as_millis()).ok()
}
