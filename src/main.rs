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

#![forbid(unsafe_code)]

mod annotations;
mod app;
mod conditions;
mod config;
mod dashboard;
mod export;
mod grafana;
mod prom;
mod theme;
mod ui;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Parser;
use config::Config;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use serde::Serialize;
use theme::Theme;

mod cli;

use annotations::{AnnotationCommandConfig, AnnotationSourceConfig};
use cli::Args;

/// Main entry point for the Grafatui application.
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(cmd) = args.command {
        match cmd {
            cli::Commands::Completions { shell } => {
                use clap::CommandFactory;
                clap_complete::generate(
                    shell,
                    &mut Args::command(),
                    "grafatui",
                    &mut std::io::stdout(),
                );
            }
            cli::Commands::Man => {
                use clap::CommandFactory;
                let man = clap_mangen::Man::new(Args::command());
                man.render(&mut std::io::stdout())?;
            }
        }
        return Ok(());
    }

    // Load config
    let config = load_startup_config(args.config.clone())?;
    let dashboard_paths: Vec<std::path::PathBuf> = if args.grafana_json.is_empty() {
        &config.grafana_json
    } else {
        &args.grafana_json
    }
    .iter()
    .map(|path| config::expand_path(path))
    .collect();

    let theme_name = args
        .theme
        .clone()
        .or_else(|| config.theme.clone())
        .unwrap_or_else(|| theme::DEFAULT_THEME.to_string());
    if args.list_themes {
        let themes = theme::catalog(&theme::custom_themes(&config.themes)?);
        print!("{}", theme_list(&theme_name, &themes));
        return Ok(());
    }

    if args.validate {
        match dashboard_paths.as_slice() {
            [] => bail!("--validate requires --grafana-json or grafana_json in config"),
            [path] => {
                let dashboard = grafana::load_grafana_dashboard(path)?;
                let summary = validate_dashboard_import(dashboard, config.vars.clone(), &args.var);
                print_validation_summary(&summary, args.format, args.strict)?;
            }
            paths => {
                let dashboards = paths
                    .iter()
                    .map(|path| {
                        let dashboard = grafana::load_grafana_dashboard(path)
                            .with_context(|| path.display().to_string())?;
                        Ok(FileValidationSummary {
                            path: path.display().to_string(),
                            summary: validate_dashboard_import(
                                dashboard,
                                config.vars.clone(),
                                &args.var,
                            ),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                print_validation_summaries(
                    &MultiValidationSummary { dashboards },
                    args.format,
                    args.strict,
                )?;
            }
        }
        return Ok(());
    }

    let annotation_source = resolve_annotation_source(
        AnnotationCliSource {
            file: args.annotations_file,
            program: args.annotations_command,
            args: args.annotations_command_arg,
            timeout: args.annotations_command_timeout,
        },
        config.annotations_file,
        config.annotations_command,
    )?;

    let prometheus_url = args
        .prometheus_url
        .or(config.prometheus_url)
        .unwrap_or_else(|| "http://localhost:9090".to_string());
    let tls = prom::TlsFiles {
        ca_cert: args.ca_cert,
        client_cert: args.client_cert,
        client_key: args.client_key,
    }
    .or(config.tls)
    .map_paths(config::expand_path);

    let range_str = args
        .range
        .or(config.time_range)
        .unwrap_or_else(|| "5m".to_string());
    let range = app::parse_duration(&range_str).context("--range")?;

    let step_str = args
        .step
        .or(config.step)
        .unwrap_or_else(|| "5s".to_string());
    let step = app::parse_duration(&step_str).context("--step")?;

    let scrape_interval = match args.scrape_interval.or(config.scrape_interval) {
        Some(text) => app::parse_duration(&text).context("--scrape-interval")?,
        None => app::DEFAULT_SCRAPE_INTERVAL,
    };
    if scrape_interval.is_zero() {
        bail!("--scrape-interval must be greater than zero");
    }

    let export_dir = args
        .export_dir
        .or(config.export_dir)
        .map(|p| config::expand_path(&p))
        .unwrap_or_else(|| std::path::PathBuf::from("./grafatui-exports"));
    let export_format = args
        .export_format
        .or(config.export_format)
        .unwrap_or_default();
    let record_max_frames = args
        .record_max_frames
        .or(config.record_max_frames)
        .unwrap_or(300);
    let autogrid_enabled = config.autogrid.unwrap_or(true);
    let autogrid_color = args
        .autogrid_color
        .or(config.autogrid_color)
        .map(|color| theme::parse_grafana_color(&color))
        .filter(|color| *color != ratatui::style::Color::Reset);
    let transparent_background =
        args.transparent_background || config.transparent_background.unwrap_or(false);

    let mut variables = VariableState::default();
    let mut query_vars = Vec::new();
    let mut template = None;
    let mut dashboard_refresh_rate_ms = None;
    let mut dashboard_set = None;

    let prom = prom::PromClient::with_tls(prometheus_url, &tls)?;

    // Build panels from Grafana import or simple queries.
    let (title, panels, skipped_panels) = match dashboard_paths.as_slice() {
        [] => {
            merge_user_vars(&mut variables, config.vars.clone(), &args.var);
            ("grafatui".to_string(), app::default_queries(args.query), 0)
        }
        [path] => {
            let d = grafana::load_grafana_dashboard(path)?;
            let import_context = build_import_context(&d, config.vars.clone(), &args.var);
            print_import_diagnostics(&import_context.diagnostics);
            dashboard_refresh_rate_ms = d.refresh_rate_ms;
            variables = import_context.variables;
            query_vars = import_context.query_vars;

            let ps: Vec<_> = d.queries.into_iter().map(panel_state).collect();
            template = Some(
                app::DashboardTemplate::new(d.layout, d.repeats, &ps)
                    .with_conditions(d.conditions)
                    .with_sections(d.sections),
            );
            (format!("{} (imported)", d.title), ps, d.skipped_panels)
        }
        paths => {
            let multi = build_dashboard_set(
                grafana::load_grafana_dashboards(paths)?,
                config.vars.clone(),
                &args.var,
                args.refresh_rate,
                config.refresh_rate,
            );
            variables = multi.variables;
            template = Some(multi.template);
            dashboard_refresh_rate_ms =
                Some(multi.set.dashboards[0].refresh_every.as_millis() as u64);
            let first = &multi.set.dashboards[0];
            let heading = (
                format!("{} (imported)", first.title),
                multi.panels,
                first.skipped_panels,
            );
            dashboard_set = Some(multi.set);
            heading
        }
    };

    let themes = theme::catalog(&theme::custom_themes(&config.themes)?);
    let theme = theme::resolve(&theme_name, &themes)?;

    // Determine threshold marker
    let marker_name = args
        .threshold_marker
        .or(config.threshold_marker)
        .unwrap_or_else(|| "dashed-line".to_string());
    let refresh_rate = resolve_refresh_rate_ms(
        args.refresh_rate,
        config.refresh_rate,
        dashboard_refresh_rate_ms,
    );
    let refresh_every = Duration::from_millis(refresh_rate);

    let mut state = app::AppState::new(
        prom,
        range,
        step,
        refresh_every,
        title,
        panels,
        skipped_panels,
        theme,
        marker_name,
        export::ExportOptions {
            dir: export_dir,
            format: export_format,
            record_max_frames,
        }
        .validate()?,
    );
    state.annotations = annotations::AnnotationState::from_source(annotation_source);
    state.scrape_interval = scrape_interval;
    state.autogrid_enabled = autogrid_enabled;
    state.autogrid_color = autogrid_color;
    state.transparent_background = transparent_background;
    state.themes = themes;
    state.vars = variables.vars;
    state.var_values = variables.var_values;
    state.regex_vars = variables.regex_vars;
    state.all_vars = variables.all_vars;
    state.variable_names = variables.names;
    state.query_vars = query_vars;
    // Repeats expand from the variables, so the template is applied after them.
    if let Some(template) = template {
        state.apply_template(template);
    }
    if let Some(set) = dashboard_set {
        state.set_dashboards(set);
    }
    // The first refresh starts once the dashboard is drawn. Stopping on a
    // signal drops the app, which aborts refreshes and kills annotation
    // provider processes.
    let mut shutdown = ShutdownSignals::register();

    install_terminal_panic_hook();
    let guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;

    let res = tokio::select! {
        res = app::run_app(
            &mut terminal,
            &mut state,
            Duration::from_millis(args.tick_rate),
        ) => res,
        () = shutdown.recv() => Ok(()),
    };
    // Save a recording however the session ended; this is a no-op when the
    // event loop already saved it on quit.
    let finalized = app::finalize_recording_before_quit(&mut state);

    drop(guard);
    res.and(finalized)
}

/// Signals asking Grafatui to stop: SIGINT, and on Unix SIGTERM or SIGHUP (the
/// terminal closing).
///
/// Handlers are installed when this is created, replacing the default action
/// of killing the process outright. Stopping through `recv` instead drops the
/// running refresh, so annotation provider processes are killed and the
/// terminal is restored.
struct ShutdownSignals {
    #[cfg(unix)]
    signals: Vec<tokio::signal::unix::Signal>,
}

impl ShutdownSignals {
    fn register() -> Self {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            Self {
                signals: [
                    SignalKind::interrupt(),
                    SignalKind::terminate(),
                    SignalKind::hangup(),
                ]
                .into_iter()
                .filter_map(|kind| signal(kind).ok())
                .collect(),
            }
        }
        #[cfg(not(unix))]
        {
            Self {}
        }
    }

    /// Resolves when any of the signals arrives. Cancel-safe.
    async fn recv(&mut self) {
        #[cfg(unix)]
        {
            let received = self
                .signals
                .iter_mut()
                .map(|signal| Box::pin(signal.recv()));
            if received.len() == 0 {
                return std::future::pending().await;
            }
            futures::future::select_all(received).await;
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
}

/// Raw mode, the alternate screen, and mouse capture, undone when dropped,
/// including on early returns and while unwinding from a panic.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        // From here on, dropping the guard restores the terminal.
        let guard = Self;
        execute!(
            std::io::stdout(),
            EnterAlternateScreen,
            crossterm::event::EnableMouseCapture
        )?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// Restores the terminal, continuing past individual failures so one failing
/// step cannot leave the others undone.
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        std::io::stdout(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture,
        crossterm::cursor::Show
    );
}

/// Restores the terminal before the default panic message is printed, so the
/// message lands on the normal screen instead of the alternate one.
fn install_terminal_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous(info);
    }));
}

/// One theme name per line, marking the one `selected` resolves to.
fn theme_list(selected: &str, themes: &[Theme]) -> String {
    let current = theme::resolve(selected, themes)
        .ok()
        .map(|theme| theme.name);
    themes
        .iter()
        .map(|theme| {
            if current.as_ref() == Some(&theme.name) {
                format!("{} (current)\n", theme.name)
            } else {
                format!("{}\n", theme.name)
            }
        })
        .collect()
}

fn load_startup_config(path: Option<std::path::PathBuf>) -> Result<Config> {
    Config::load(path)
}

/// A panel to show for an imported Grafana panel, before any data.
fn panel_state(q: grafana::QueryPanel) -> app::PanelState {
    app::PanelState {
        title: q.title,
        exprs: q.exprs,
        legends: q.legends,
        query_modes: q.query_modes,
        series: vec![],
        last_error: None,
        last_url: None,
        last_samples: 0,
        grid: q.grid.map(|g| app::GridUnit {
            x: g.x,
            y: g.y,
            w: g.w,
            h: g.h,
        }),
        y_axis_mode: app::YAxisMode::Auto,
        panel_type: q.panel_type,
        thresholds: q.thresholds,
        min: q.min,
        max: q.max,
        autogrid: q.autogrid,
        display: q.display,
        options: q.options,
        resolution: q.resolution,
        notices: Default::default(),
    }
}

/// Several dashboards ready to show, one per tab.
struct DashboardSetup {
    panels: Vec<app::PanelState>,
    template: app::DashboardTemplate,
    variables: VariableState,
    set: app::DashboardSet,
}

/// Prepares several dashboards to show, one per tab. Each dashboard's
/// variables stay its own; config and `--var` values pin a variable in every
/// dashboard that defines it. Diagnostics are printed per dashboard.
fn build_dashboard_set(
    set: grafana::DashboardSet,
    config_vars: Option<HashMap<String, String>>,
    cli_vars: &[(String, String)],
    cli_refresh_rate: Option<u64>,
    config_refresh_rate: Option<u64>,
) -> DashboardSetup {
    let grafana::DashboardSet {
        mut combined,
        root,
        files,
    } = set;
    let mut names = HashSet::new();
    let mut dashboards = Vec::with_capacity(files.len());
    for file in files {
        let context = build_import_context(&file.single, config_vars.clone(), cli_vars);
        print_import_diagnostics_for(Some(&file.path), &context.diagnostics);
        names.extend(file.single.variable_names);
        let refresh_rate = resolve_refresh_rate_ms(
            cli_refresh_rate,
            config_refresh_rate,
            file.single.refresh_rate_ms,
        );
        dashboards.push(app::DashboardInfo {
            title: file.single.title,
            path: file.path,
            refresh_every: Duration::from_millis(refresh_rate),
            skipped_panels: file.single.skipped_panels,
        });
    }

    let mut variables = VariableState::default();
    let pinned = merge_user_vars(&mut variables, config_vars, cli_vars);
    variables.names.extend(names);
    pin_section_variables(&mut combined.sections, root, &pinned, &variables.var_values);

    let titles: Vec<String> = dashboards.iter().map(|info| info.title.clone()).collect();
    if let Some(crate::dashboard::DashboardLayoutItem::Tabs(group)) =
        combined.layout.items.first_mut()
    {
        for (tab, label) in group
            .tabs
            .iter_mut()
            .zip(app::dashboard_tab_labels(&titles))
        {
            tab.title = label;
        }
    }
    let section_owner = combined.layout.section_owners(root);
    let panels: Vec<_> = combined.queries.into_iter().map(panel_state).collect();
    let template = app::DashboardTemplate::new(combined.layout, combined.repeats, &panels)
        .with_conditions(combined.conditions)
        .with_sections(combined.sections);
    DashboardSetup {
        panels,
        template,
        variables,
        set: app::DashboardSet::new(root, dashboards, section_owner),
    }
}

/// Pins variables of every dashboard in tab group `root` to the user's values,
/// as `merge_user_vars` pins a single dashboard's.
fn pin_section_variables(
    sections: &mut HashMap<crate::dashboard::SectionId, Vec<grafana::SectionVariable>>,
    root: crate::dashboard::TabGroupId,
    pinned: &HashSet<String>,
    values: &HashMap<String, Vec<String>>,
) {
    for (section, variables) in sections.iter_mut() {
        if !matches!(section, crate::dashboard::SectionId::Tab(group, _) if *group == root) {
            continue;
        }
        for variable in variables {
            if !pinned.contains(&variable.name) {
                continue;
            }
            let pinned_values = values.get(&variable.name).cloned().unwrap_or_default();
            variable.regex = pinned_values.len() > 1;
            variable.values = pinned_values;
            variable.all = false;
            variable.all_value = None;
            variable.query = None;
        }
    }
}

fn resolve_refresh_rate_ms(
    cli_refresh_rate: Option<u64>,
    config_refresh_rate: Option<u64>,
    dashboard_refresh_rate: Option<u64>,
) -> u64 {
    cli_refresh_rate
        .or(config_refresh_rate)
        .or(dashboard_refresh_rate)
        .unwrap_or(1000)
}

#[derive(Debug, Default)]
struct AnnotationCliSource {
    file: Option<std::path::PathBuf>,
    program: Option<String>,
    args: Vec<String>,
    timeout: Option<String>,
}

fn resolve_annotation_source(
    cli: AnnotationCliSource,
    config_file: Option<std::path::PathBuf>,
    config_command: Option<AnnotationCommandConfig>,
) -> Result<Option<AnnotationSourceConfig>> {
    if config_file.is_some() && config_command.is_some() {
        bail!("annotations_file and annotations_command cannot both be configured");
    }
    if cli.file.is_some() && cli.program.is_some() {
        bail!("--annotations-file and --annotations-command cannot be combined");
    }
    if cli.program.is_none() && (!cli.args.is_empty() || cli.timeout.is_some()) {
        bail!("annotation command arguments and timeout require --annotations-command");
    }

    if let Some(path) = cli.file {
        return Ok(Some(AnnotationSourceConfig::File(config::expand_path(
            &path,
        ))));
    }
    if let Some(program) = cli.program {
        let timeout = match cli.timeout {
            Some(value) => humantime::parse_duration(&value)
                .with_context(|| "--annotations-command-timeout")?,
            None => annotations::DEFAULT_COMMAND_TIMEOUT,
        };
        validate_annotation_command(&program, timeout)?;
        return Ok(Some(AnnotationSourceConfig::Command(
            AnnotationCommandConfig {
                program,
                args: cli.args,
                timeout,
            },
        )));
    }

    if let Some(command) = config_command {
        validate_annotation_command(&command.program, command.timeout)?;
        return Ok(Some(AnnotationSourceConfig::Command(command)));
    }
    Ok(config_file.map(|path| AnnotationSourceConfig::File(config::expand_path(&path))))
}

fn validate_annotation_command(program: &str, timeout: Duration) -> Result<()> {
    if program.trim().is_empty() {
        bail!("annotation command program must not be empty");
    }
    if timeout.is_zero() {
        bail!("annotation command timeout must be greater than zero");
    }
    Ok(())
}

#[derive(Debug)]
struct ImportContext {
    variables: VariableState,
    query_vars: Vec<grafana::TemplateQueryVar>,
    diagnostics: Vec<grafana::ImportDiagnostic>,
}

/// Dashboard variables: query-formatted values, raw selections, and which
/// variables are regex-formatted.
#[derive(Debug, Default)]
struct VariableState {
    vars: HashMap<String, String>,
    var_values: HashMap<String, Vec<String>>,
    regex_vars: HashSet<String>,
    all_vars: HashSet<String>,
    names: HashSet<String>,
}

/// Validation of several dashboards, one entry per file.
#[derive(Debug, Serialize)]
struct MultiValidationSummary {
    dashboards: Vec<FileValidationSummary>,
}

#[derive(Debug, Serialize)]
struct FileValidationSummary {
    path: String,
    #[serde(flatten)]
    summary: ImportValidationSummary,
}

#[derive(Debug, Serialize)]
struct ImportValidationSummary {
    title: String,
    panel_count: usize,
    diagnostics: Vec<grafana::ImportDiagnostic>,
}

fn validate_dashboard_import(
    dashboard: grafana::DashboardImport,
    config_vars: Option<HashMap<String, String>>,
    cli_vars: &[(String, String)],
) -> ImportValidationSummary {
    let import_context = build_import_context(&dashboard, config_vars, cli_vars);
    ImportValidationSummary {
        title: dashboard.title,
        panel_count: dashboard.queries.len(),
        diagnostics: import_context.diagnostics,
    }
}

fn build_import_context(
    dashboard: &grafana::DashboardImport,
    config_vars: Option<HashMap<String, String>>,
    cli_vars: &[(String, String)],
) -> ImportContext {
    let mut variables = VariableState {
        vars: dashboard.vars.clone(),
        var_values: dashboard.var_values.clone(),
        regex_vars: dashboard.regex_vars.clone(),
        all_vars: dashboard.all_vars.clone(),
        names: dashboard.variable_names.clone(),
    };
    let pinned_vars = merge_user_vars(&mut variables, config_vars, cli_vars);

    let query_vars = dashboard
        .query_vars
        .iter()
        .filter(|var| !pinned_vars.contains(&var.name))
        .cloned()
        .collect();
    let mut diagnostics = dashboard.diagnostics.clone();
    diagnostics.extend(grafana::variable_diagnostics(dashboard, &variables.vars));

    ImportContext {
        variables,
        query_vars,
        diagnostics,
    }
}

/// Applies config and `--var` overrides, returning the variables they pin.
///
/// A single user value is used verbatim, so it may be a regex. Repeating
/// `--var` for one name selects several values, which are regex-escaped and
/// joined like a Grafana multi-value selection and iterated by repeats.
fn merge_user_vars(
    variables: &mut VariableState,
    config_vars: Option<HashMap<String, String>>,
    cli_vars: &[(String, String)],
) -> HashSet<String> {
    let mut selections: Vec<(String, Vec<String>)> = config_vars
        .into_iter()
        .flatten()
        .map(|(name, value)| (name, vec![value]))
        .collect();
    let mut cli_selections: Vec<(String, Vec<String>)> = Vec::new();
    for (name, value) in cli_vars {
        match cli_selections
            .iter_mut()
            .find(|(selected, _)| selected == name)
        {
            Some((_, values)) => values.push(value.clone()),
            None => cli_selections.push((name.clone(), vec![value.clone()])),
        }
    }
    selections.extend(cli_selections);

    let mut pinned_vars = HashSet::new();
    for (name, values) in selections {
        pinned_vars.insert(name.clone());
        variables.all_vars.remove(&name);
        variables.names.insert(name.clone());
        variables.vars.insert(
            name.clone(),
            app::format_prometheus_values(&values, values.len() > 1),
        );
        variables.var_values.insert(name, values);
    }

    pinned_vars
}

fn print_import_diagnostics(diagnostics: &[grafana::ImportDiagnostic]) {
    print_import_diagnostics_for(None, diagnostics);
}

/// Prints import diagnostics, naming the dashboard file when several load.
fn print_import_diagnostics_for(
    file: Option<&std::path::Path>,
    diagnostics: &[grafana::ImportDiagnostic],
) {
    if diagnostics.is_empty() {
        return;
    }

    let file = file.map(|path| path.display().to_string());
    match &file {
        Some(file) => eprintln!(
            "Grafana import diagnostics ({file}): {} warning(s)",
            diagnostics.len()
        ),
        None => eprintln!(
            "Grafana import diagnostics: {} warning(s)",
            diagnostics.len()
        ),
    }
    for diagnostic in diagnostics {
        let path = match &file {
            Some(file) => format!("{file}:{}", diagnostic.path),
            None => diagnostic.path.clone(),
        };
        eprintln!(
            "warning[grafana.import.{}] {path}: {}",
            diagnostic.code, diagnostic.message
        );
    }
}

/// Prints the validation of several dashboards: every file's diagnostics and
/// result, then fails under `strict` if any file had warnings.
fn print_validation_summaries(
    summaries: &MultiValidationSummary,
    format: cli::ValidateFormat,
    strict: bool,
) -> Result<()> {
    match format {
        cli::ValidateFormat::Text => {
            for file in &summaries.dashboards {
                print_import_diagnostics_for(
                    Some(std::path::Path::new(&file.path)),
                    &file.summary.diagnostics,
                );
            }
            for file in &summaries.dashboards {
                println!(
                    "Grafana dashboard is importable: {} ({} panel(s)) [{}]",
                    file.summary.title, file.summary.panel_count, file.path
                );
            }
        }
        cli::ValidateFormat::Json => {
            println!("{}", serde_json::to_string_pretty(summaries)?);
        }
    }
    let warnings: usize = summaries
        .dashboards
        .iter()
        .map(|file| file.summary.diagnostics.len())
        .sum();
    if strict && warnings > 0 {
        bail!("validation failed with {warnings} warning(s)");
    }
    Ok(())
}

fn print_validation_summary(
    summary: &ImportValidationSummary,
    format: cli::ValidateFormat,
    strict: bool,
) -> Result<()> {
    match format {
        cli::ValidateFormat::Text => {
            print_import_diagnostics(&summary.diagnostics);
            if strict && !summary.diagnostics.is_empty() {
                bail!(
                    "validation failed with {} warning(s)",
                    summary.diagnostics.len()
                );
            }
            println!(
                "Grafana dashboard is importable: {} ({} panel(s))",
                summary.title, summary.panel_count
            );
        }
        cli::ValidateFormat::Json => {
            println!("{}", serde_json::to_string_pretty(summary)?);
            if strict && !summary.diagnostics.is_empty() {
                bail!(
                    "validation failed with {} warning(s)",
                    summary.diagnostics.len()
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::{DashboardLayout, DashboardLayoutItem, DashboardRow, RowId};

    fn temp_config_path(name: &str) -> std::path::PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "grafatui-{name}-{}-{suffix}.toml",
            std::process::id()
        ))
    }

    #[test]
    fn imported_layout_replaces_the_initial_flat_layout() {
        let mut state = app::AppState::new(
            prom::PromClient::new("http://localhost:9090".to_string()),
            Duration::from_secs(60),
            Duration::from_secs(5),
            Duration::from_secs(1),
            "test".to_string(),
            app::default_queries(vec!["up".to_string()]),
            0,
            Theme::default(),
            "dashed-line".to_string(),
            export::ExportOptions::default(),
        );
        let layout = DashboardLayout::new(vec![DashboardLayoutItem::Row(DashboardRow::new(
            RowId::new(0),
            "Collapsed",
            true,
            false,
            vec![DashboardLayoutItem::Panel(0)],
        ))]);

        let template = app::DashboardTemplate::new(
            layout,
            crate::dashboard::Repeats::default(),
            &state.panels,
        );
        state.apply_template(template);

        assert_eq!(state.visible_panel_indices(), Vec::<usize>::new());
    }

    #[test]
    fn startup_config_loader_propagates_parse_errors() {
        let path = temp_config_path("malformed-startup-config");
        std::fs::write(
            &path,
            "[annotations_command]\nprogram = \"./provider\"\ntimeout = \"soon\"\n",
        )
        .unwrap();

        let result = load_startup_config(Some(path.clone()));

        std::fs::remove_file(path).unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_refresh_rate_precedence() {
        assert_eq!(
            resolve_refresh_rate_ms(Some(2000), Some(3000), Some(4000)),
            2000
        );
        assert_eq!(resolve_refresh_rate_ms(None, Some(3000), Some(4000)), 3000);
        assert_eq!(resolve_refresh_rate_ms(None, None, Some(4000)), 4000);
        assert_eq!(resolve_refresh_rate_ms(None, None, None), 1000);
    }

    #[test]
    fn annotation_source_cli_command_replaces_complete_toml_source() {
        let resolved = resolve_annotation_source(
            AnnotationCliSource {
                program: Some("./cli-provider".into()),
                args: vec!["cli".into()],
                timeout: Some("2s".into()),
                ..AnnotationCliSource::default()
            },
            Some("config.jsonl".into()),
            None,
        )
        .unwrap();

        assert_eq!(
            resolved,
            Some(annotations::AnnotationSourceConfig::Command(
                annotations::AnnotationCommandConfig {
                    program: "./cli-provider".into(),
                    args: vec!["cli".into()],
                    timeout: Duration::from_secs(2),
                },
            ))
        );
    }

    #[test]
    fn annotation_source_rejects_invalid_same_layer_configuration() {
        let toml_command = annotations::AnnotationCommandConfig {
            program: "./config-provider".into(),
            args: vec![],
            timeout: Duration::from_secs(10),
        };
        assert!(
            resolve_annotation_source(
                AnnotationCliSource::default(),
                Some("events.jsonl".into()),
                Some(toml_command),
            )
            .is_err()
        );
        assert!(
            resolve_annotation_source(
                AnnotationCliSource {
                    program: Some("   ".into()),
                    ..AnnotationCliSource::default()
                },
                None,
                None,
            )
            .is_err()
        );
        assert!(
            resolve_annotation_source(
                AnnotationCliSource {
                    program: Some("./provider".into()),
                    timeout: Some("0s".into()),
                    ..AnnotationCliSource::default()
                },
                None,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn annotation_source_returns_none_when_unconfigured() {
        assert_eq!(
            resolve_annotation_source(AnnotationCliSource::default(), None, None).unwrap(),
            None
        );
    }

    #[test]
    fn annotation_source_cli_file_replaces_toml_command() {
        let resolved = resolve_annotation_source(
            AnnotationCliSource {
                file: Some("cli.jsonl".into()),
                ..AnnotationCliSource::default()
            },
            None,
            Some(annotations::AnnotationCommandConfig {
                program: "./config-provider".into(),
                args: vec!["config".into()],
                timeout: Duration::from_secs(10),
            }),
        )
        .unwrap();

        assert_eq!(
            resolved,
            Some(annotations::AnnotationSourceConfig::File(
                "cli.jsonl".into()
            ))
        );
    }

    #[test]
    fn annotation_source_uses_toml_command_without_cli_source() {
        let command = annotations::AnnotationCommandConfig {
            program: "./config-provider".into(),
            args: vec!["config".into()],
            timeout: Duration::from_secs(3),
        };

        assert_eq!(
            resolve_annotation_source(AnnotationCliSource::default(), None, Some(command.clone()))
                .unwrap(),
            Some(annotations::AnnotationSourceConfig::Command(command))
        );
    }

    #[test]
    fn annotation_source_rejects_malformed_cli_timeout() {
        assert!(
            resolve_annotation_source(
                AnnotationCliSource {
                    program: Some("./provider".into()),
                    timeout: Some("soon".into()),
                    ..AnnotationCliSource::default()
                },
                None,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn test_validate_dashboard_import_adds_variable_diagnostics_without_prometheus() {
        let json = r#"{
            "title": "Validate",
            "panels": [
                {
                    "type": "timeseries",
                    "title": "CPU",
                    "targets": [
                        { "expr": "up{job=\"$job\", cluster=\"$cluster\"}" }
                    ]
                }
            ]
        }"#;
        let path = std::env::temp_dir().join("grafatui-validate-helper-test.json");
        std::fs::write(&path, json).unwrap();
        let dashboard = grafana::load_grafana_dashboard(&path).unwrap();
        std::fs::remove_file(path).unwrap();

        let summary =
            validate_dashboard_import(dashboard, None, &[("job".to_string(), "node".to_string())]);

        assert_eq!(summary.title, "Validate");
        assert_eq!(summary.panel_count, 1);
        assert_eq!(summary.diagnostics.len(), 1);
        assert_eq!(summary.diagnostics[0].code, "unresolved_variable");
        assert!(summary.diagnostics[0].message.contains("$cluster"));
    }

    #[test]
    fn test_merge_user_vars_applies_config_and_cli_overrides() {
        let mut variables = VariableState::default();
        variables
            .vars
            .insert("job".to_string(), "dashboard".to_string());
        let mut config_vars = HashMap::new();
        config_vars.insert("job".to_string(), "config".to_string());
        config_vars.insert("instance".to_string(), "config-instance".to_string());

        let pinned = merge_user_vars(
            &mut variables,
            Some(config_vars),
            &[("job".to_string(), "cli".to_string())],
        );

        assert_eq!(variables.vars.get("job"), Some(&"cli".to_string()));
        assert_eq!(
            variables.vars.get("instance"),
            Some(&"config-instance".to_string())
        );
        assert_eq!(
            variables.var_values.get("job"),
            Some(&vec!["cli".to_string()])
        );
        assert!(pinned.contains("job"));
        assert!(pinned.contains("instance"));
    }

    #[test]
    fn several_dashboards_keep_their_own_variables_and_refresh_rates() {
        use crate::app::dashboard_test_support::v2_dashboard;
        let nodes = v2_dashboard(
            "Ops / Nodes",
            "30s",
            "up",
            "label_values(node_uname_info, instance)",
        );
        let consul = v2_dashboard(
            "Ops / Consul",
            "1m",
            "up",
            "label_values(consul_up, instance)",
        );
        let set = grafana::parse_grafana_dashboards(&[&nodes, &consul]).unwrap();

        let setup = build_dashboard_set(set, None, &[], None, None);

        let refresh: Vec<Duration> = setup
            .set
            .dashboards
            .iter()
            .map(|info| info.refresh_every)
            .collect();
        assert_eq!(refresh, [Duration::from_secs(30), Duration::from_secs(60)]);
        assert!(setup.variables.vars.is_empty());
        assert!(setup.variables.names.contains("instance"));
        let titles: Vec<String> = setup
            .set
            .dashboards
            .iter()
            .map(|info| info.title.clone())
            .collect();
        assert_eq!(titles, ["Ops / Nodes", "Ops / Consul"]);

        // The CLI and config refresh rates apply to every dashboard.
        let set = grafana::parse_grafana_dashboards(&[&nodes, &consul]).unwrap();
        let setup = build_dashboard_set(set, None, &[], Some(5_000), Some(10_000));
        assert!(
            setup
                .set
                .dashboards
                .iter()
                .all(|info| info.refresh_every == Duration::from_secs(5))
        );
    }

    #[test]
    fn pinned_variables_apply_in_every_dashboard() {
        use crate::app::dashboard_test_support::v2_dashboard;
        use crate::dashboard::SectionId;
        let nodes = v2_dashboard(
            "Nodes",
            "30s",
            "up",
            "label_values(node_uname_info, instance)",
        );
        let consul = v2_dashboard("Consul", "30s", "up", "label_values(consul_up, instance)");
        let mut set = grafana::parse_grafana_dashboards(&[&nodes, &consul]).unwrap();
        let root = set.root;
        let mut variables = VariableState::default();
        let pinned = merge_user_vars(
            &mut variables,
            None,
            &[
                ("instance".to_string(), "host-1".to_string()),
                ("instance".to_string(), "host-2".to_string()),
            ],
        );

        pin_section_variables(
            &mut set.combined.sections,
            root,
            &pinned,
            &variables.var_values,
        );

        for tab in 0..2 {
            let instance = &set.combined.sections[&SectionId::Tab(root, tab)][0];
            assert_eq!(instance.name, "instance");
            assert_eq!(instance.values, ["host-1", "host-2"]);
            assert!(instance.regex);
            assert!(instance.query.is_none(), "pinned variables are not queried");
        }
    }

    #[test]
    fn repeated_cli_vars_select_several_values() {
        let mut variables = VariableState::default();

        merge_user_vars(
            &mut variables,
            None,
            &[
                ("job".to_string(), "api.v1".to_string()),
                ("job".to_string(), "web".to_string()),
                ("re".to_string(), ".*".to_string()),
            ],
        );

        assert_eq!(
            variables.vars.get("job").map(String::as_str),
            Some("(api\\\\.v1|web)")
        );
        assert_eq!(
            variables.var_values.get("job"),
            Some(&vec!["api.v1".to_string(), "web".to_string()])
        );
        // A single value stays verbatim, so it can still be a regex.
        assert_eq!(variables.vars.get("re").map(String::as_str), Some(".*"));
    }
}
