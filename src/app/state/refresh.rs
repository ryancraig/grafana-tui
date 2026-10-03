//! Background refreshes. Prometheus queries and annotation providers run in
//! spawned tasks while the event loop keeps handling input; their results are
//! applied to `AppState` as each task finishes.
//!
//! A refresh runs in stages, because resolved variables can change the layout
//! and therefore which panels need data:
//!
//! 1. Query variables, then the visible panels' data.
//! 2. Row and tab variables, when the dashboard has any.
//! 3. Panels that appeared after the layout or conditions changed.

use super::{AppState, PanelState, SeriesView};
use crate::annotations::{AnnotationProvider, AnnotationRefreshContext, ProviderPoll};
use crate::app::QueryMode;
use crate::app::data::{QueryIntervals, custom_legend, downsample, expand_expr, format_legend};
use crate::app::template::{ResolvedSections, Scope, SectionInstance, scoped_vars};
use crate::app::variables::{VariableReport, VariableUpdate, refresh_query_variables};
use crate::grafana::TemplateQueryVar;
use crate::prom;
use futures::StreamExt;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::task::{AbortHandle, JoinError, JoinSet};

/// How long a refresh runs before the title bar says it is still refreshing.
const SLOW_REFRESH: Duration = Duration::from_secs(1);

/// Longest wait between attempts while Prometheus is unreachable.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Panels fetched at once; each runs its queries in order.
const CONCURRENT_PANELS: usize = 4;

/// Whether Prometheus answered the latest refresh.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum BackendStatus {
    /// No refresh has finished yet.
    #[default]
    Connecting,
    Live,
    /// Every query in this many consecutive refreshes failed to connect.
    Unreachable {
        failures: u32,
    },
}

/// What the title bar shows about the backend, when anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackendIndicator {
    Connecting,
    Refreshing,
    Unreachable,
}

/// Background refresh tasks and the bookkeeping that orders their results.
#[derive(Debug, Default)]
pub(crate) struct RefreshTasks {
    /// Dropping the set aborts its tasks, which kills annotation provider
    /// processes when the app exits.
    tasks: JoinSet<RefreshMessage>,
    /// Counts started refreshes, so results of a replaced one are ignored.
    generation: u64,
    pipeline: Option<Pipeline>,
    /// Orders panel fetches, so an older result never replaces newer data.
    next_seq: u64,
    applied_seq: HashMap<usize, u64>,
    /// Changes whenever the layout is rebuilt and panel indices may be reused.
    layout_epoch: u64,
    annotations_running: bool,
    /// The window to load annotations for once the running load finishes.
    annotations_queued: Option<AnnotationRefreshContext>,
    status: BackendStatus,
    /// Why the latest refresh could not reach Prometheus, when it was a TLS
    /// failure such as an unknown issuer.
    unreachable_cause: Option<String>,
    /// The range and offset query variables last resolved for; `None` until
    /// they first resolve.
    variables_window: Option<(Duration, Duration)>,
    /// The user asked for a refresh, which re-queries time range variables.
    reload_requested: bool,
    /// Why each query variable last failed, by name.
    variable_errors: BTreeMap<String, String>,
}

impl RefreshTasks {
    /// Invalidates panel fetches started before the layout was rebuilt.
    pub(super) fn layout_changed(&mut self) {
        self.layout_epoch += 1;
        self.applied_seq.clear();
    }

    fn next_seq(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }

    fn apply_variable_report(&mut self, report: VariableReport) {
        for name in &report.queried {
            self.variable_errors.remove(name);
        }
        self.variable_errors.extend(report.errors);
    }

    fn is_current(&self, generation: u64) -> bool {
        self.pipeline
            .as_ref()
            .is_some_and(|pipeline| pipeline.generation == generation)
    }
}

/// The refresh in progress.
#[derive(Debug)]
struct Pipeline {
    generation: u64,
    started: Instant,
    slow_drawn: bool,
    /// The running stage, aborted when a newer refresh replaces this one.
    abort: AbortHandle,
    end_ts: i64,
    /// Panels the first stage fetched.
    fetched: Vec<usize>,
    /// Variable selections before the refresh, to detect changes.
    selected_values: HashMap<String, Vec<String>>,
    /// Why variables resolve in this refresh.
    variable_update: VariableUpdate,
    variables_window: (Duration, Duration),
    variables_changed: bool,
    counts: FetchCounts,
}

/// The finished work of a background task.
#[derive(Debug)]
pub(crate) enum RefreshMessage {
    /// Resolved query variables and the first stage's panel data.
    Data {
        generation: u64,
        vars: HashMap<String, String>,
        var_values: HashMap<String, Vec<String>>,
        report: VariableReport,
        batch: PanelBatch,
    },
    /// Resolved row and tab variables.
    Sections {
        generation: u64,
        epoch: u64,
        values: ResolvedSections,
        report: VariableReport,
    },
    /// Data for panels that became visible.
    Panels(PanelBatch),
    /// The annotation provider, handed back with its result.
    Annotations {
        provider: Box<dyn AnnotationProvider>,
        poll: ProviderPoll,
    },
}

#[derive(Debug)]
pub(crate) struct PanelBatch {
    seq: u64,
    epoch: u64,
    /// The refresh this batch finishes, if it is a refresh's last stage.
    pipeline: Option<u64>,
    fetches: Vec<PanelFetch>,
    counts: FetchCounts,
}

#[derive(Debug)]
struct PanelFetch {
    index: usize,
    series: Vec<SeriesView>,
    url: Option<String>,
    /// One message per failed query.
    errors: Vec<String>,
    warnings: Vec<String>,
    infos: Vec<String>,
    /// Whether any of the panel's queries succeeded.
    succeeded: bool,
}

/// Query outcomes, to tell an unreachable backend from failing queries.
#[derive(Debug, Default, Clone)]
struct FetchCounts {
    succeeded: usize,
    unreachable: usize,
    /// The first TLS failure among the unreachable queries.
    tls_failure: Option<String>,
}

impl FetchCounts {
    fn add(&mut self, other: Self) {
        self.succeeded += other.succeeded;
        self.unreachable += other.unreachable;
        if self.tls_failure.is_none() {
            self.tls_failure = other.tls_failure;
        }
    }
}

/// The time window and resolution settings one stage queries with.
#[derive(Debug, Clone, Copy)]
struct QueryWindow {
    range: Duration,
    min_step: Duration,
    scrape_interval: Duration,
    end_ts: i64,
}

/// A panel's queries, copied so they can run in the background.
#[derive(Debug)]
struct PanelJob {
    index: usize,
    panel: PanelState,
    scope: Option<Scope>,
}

impl AppState {
    /// Starts a refresh of the current window, replacing one in progress.
    pub(crate) fn start_refresh(&mut self) {
        if let Some(pipeline) = self.refreshes.pipeline.take() {
            pipeline.abort.abort();
        }
        self.refreshes.generation += 1;
        let generation = self.refreshes.generation;
        let variables_window = (self.range, self.time_offset);
        let variable_update = match self.refreshes.variables_window {
            None => VariableUpdate::Load,
            Some(window) if window != variables_window || self.refreshes.reload_requested => {
                VariableUpdate::TimeRangeChange
            }
            Some(_) => VariableUpdate::Tick,
        };
        let end_ts = chrono::Utc::now().timestamp() - self.time_offset.as_secs() as i64;
        let window = self.query_window(end_ts);
        let fetched = self.panels_to_fetch();
        let jobs = self.panel_jobs(&fetched);
        let seq = self.refreshes.next_seq();
        let epoch = self.refreshes.layout_epoch;
        let prometheus = self.prometheus.clone();
        let query_vars = self.query_vars.clone();
        let mut vars = self.vars.clone();
        let mut var_values = self.var_values.clone();
        let abort = self.refreshes.tasks.spawn(async move {
            let report = refresh_variables(
                &prometheus,
                &query_vars,
                variable_update,
                window,
                &mut vars,
                &mut var_values,
            )
            .await;
            let (fetches, counts) = fetch_panels(&prometheus, window, &vars, jobs).await;
            RefreshMessage::Data {
                generation,
                vars,
                var_values,
                report,
                batch: PanelBatch {
                    seq,
                    epoch,
                    pipeline: None,
                    fetches,
                    counts,
                },
            }
        });
        self.refreshes.pipeline = Some(Pipeline {
            generation,
            started: Instant::now(),
            slow_drawn: false,
            abort,
            end_ts,
            fetched,
            selected_values: self.var_values.clone(),
            variable_update,
            variables_window,
            variables_changed: false,
            counts: FetchCounts::default(),
        });
        self.start_annotation_refresh(AnnotationRefreshContext::from_unix_window(
            end_ts, self.range,
        ));
    }

    /// Refreshes at the user's request, re-querying variables that refresh
    /// when the time range changes, as Grafana's refresh button does.
    pub(crate) fn reload(&mut self) {
        self.refreshes.reload_requested = true;
        self.start_refresh();
    }

    /// Query variables that failed when they last resolved, with the error.
    pub(crate) fn variable_errors(&self) -> impl Iterator<Item = (&str, &str)> {
        self.refreshes
            .variable_errors
            .iter()
            .map(|(name, error)| (name.as_str(), error.as_str()))
    }

    /// Fetches panels that became visible, for the displayed window.
    pub(crate) fn fetch_panels(&mut self, indices: &[usize]) {
        self.spawn_panel_batch(indices, None, self.view_end_ts);
    }

    /// When the next periodic refresh is due; `None` while one is running.
    /// Retries back off while Prometheus is unreachable.
    pub(crate) fn next_refresh_at(&self) -> Option<Instant> {
        if self.refreshes.pipeline.is_some() {
            return None;
        }
        let interval = match self.refreshes.status {
            BackendStatus::Unreachable { failures } => {
                let backoff =
                    self.refresh_every.max(Duration::from_secs(1)) * 2_u32.pow(failures.min(5));
                backoff.min(MAX_BACKOFF).max(self.refresh_every)
            }
            BackendStatus::Connecting | BackendStatus::Live => self.refresh_every,
        };
        Some(self.last_refresh + interval)
    }

    /// When a running refresh becomes slow enough to show, if not shown yet.
    pub(crate) fn slow_refresh_at(&self) -> Option<Instant> {
        self.refreshes
            .pipeline
            .as_ref()
            .filter(|pipeline| !pipeline.slow_drawn)
            .map(|pipeline| pipeline.started + SLOW_REFRESH)
    }

    pub(crate) fn mark_slow_refresh_drawn(&mut self) {
        if let Some(pipeline) = self.refreshes.pipeline.as_mut() {
            pipeline.slow_drawn = true;
        }
    }

    pub(crate) fn backend_status(&self) -> BackendStatus {
        self.refreshes.status
    }

    /// Why Prometheus is unreachable, when a TLS failure is the reason.
    pub(crate) fn unreachable_cause(&self) -> Option<&str> {
        self.refreshes.unreachable_cause.as_deref()
    }

    pub(crate) fn backend_indicator(&self) -> Option<BackendIndicator> {
        match self.refreshes.status {
            BackendStatus::Connecting => Some(BackendIndicator::Connecting),
            BackendStatus::Unreachable { .. } => Some(BackendIndicator::Unreachable),
            BackendStatus::Live => self
                .refreshes
                .pipeline
                .as_ref()
                .filter(|pipeline| pipeline.started.elapsed() >= SLOW_REFRESH)
                .map(|_| BackendIndicator::Refreshing),
        }
    }

    /// Waits for the next background task to finish; `None` when none runs.
    pub(crate) async fn join_refresh_task(&mut self) -> Option<Result<RefreshMessage, JoinError>> {
        self.refreshes.tasks.join_next().await
    }

    /// Applies a finished task, starting the refresh's next stage if any.
    /// Returns whether anything shown changed.
    pub(crate) fn finish_refresh_task(
        &mut self,
        result: Result<RefreshMessage, JoinError>,
    ) -> bool {
        let message = match result {
            Ok(message) => message,
            Err(error) if error.is_cancelled() => return false,
            Err(error) => std::panic::resume_unwind(error.into_panic()),
        };
        match message {
            RefreshMessage::Data {
                generation,
                vars,
                var_values,
                report,
                batch,
            } => self.finish_data(generation, vars, var_values, report, batch),
            RefreshMessage::Sections {
                generation,
                epoch,
                values,
                report,
            } => self.finish_sections(generation, epoch, values, report),
            RefreshMessage::Panels(batch) => self.finish_panels(batch),
            RefreshMessage::Annotations { provider, poll } => {
                self.refreshes.annotations_running = false;
                let loaded = self.annotations.finish_refresh(provider, poll);
                self.reconcile_visible_annotation_targets();
                if let Some(context) = self.refreshes.annotations_queued.take() {
                    self.start_annotation_refresh(context);
                }
                loaded
            }
        }
    }

    /// Runs every background task to completion, including later stages.
    #[cfg(test)]
    pub(crate) async fn settle_refreshes(&mut self) {
        while let Some(result) = self.join_refresh_task().await {
            self.finish_refresh_task(result);
        }
    }

    /// Refreshes and waits until every stage has finished.
    #[cfg(test)]
    pub(crate) async fn refresh(&mut self) -> anyhow::Result<()> {
        self.start_refresh();
        self.settle_refreshes().await;
        Ok(())
    }

    fn finish_data(
        &mut self,
        generation: u64,
        vars: HashMap<String, String>,
        var_values: HashMap<String, Vec<String>>,
        report: VariableReport,
        mut batch: PanelBatch,
    ) -> bool {
        if !self.refreshes.is_current(generation) {
            return false;
        }
        let Some(mut pipeline) = self.refreshes.pipeline.take() else {
            return false;
        };
        self.vars = vars;
        self.var_values = var_values;
        self.refreshes.apply_variable_report(report);
        self.refreshes.variables_window = Some(pipeline.variables_window);
        self.refreshes.reload_requested = false;
        pipeline.counts.add(std::mem::take(&mut batch.counts));
        self.apply_panel_batch(batch);
        self.view_end_ts = pipeline.end_ts;

        pipeline.variables_changed =
            self.template.is_some() && self.var_values != pipeline.selected_values;
        if pipeline.variables_changed {
            self.materialize_layout();
        }
        if self.section_instances.is_empty() {
            self.continue_refresh(pipeline);
        } else {
            pipeline.abort =
                self.spawn_section_refresh(generation, pipeline.end_ts, pipeline.variable_update);
            self.refreshes.pipeline = Some(pipeline);
        }
        true
    }

    fn finish_sections(
        &mut self,
        generation: u64,
        epoch: u64,
        values: ResolvedSections,
        report: VariableReport,
    ) -> bool {
        if !self.refreshes.is_current(generation) {
            return false;
        }
        let Some(mut pipeline) = self.refreshes.pipeline.take() else {
            return false;
        };
        self.refreshes.apply_variable_report(report);
        if epoch == self.refreshes.layout_epoch && values != self.section_values {
            self.section_values = values;
            self.materialize_layout();
            pipeline.variables_changed = true;
        }
        self.continue_refresh(pipeline);
        true
    }

    fn finish_panels(&mut self, mut batch: PanelBatch) -> bool {
        let finishes = batch.pipeline;
        let counts = std::mem::take(&mut batch.counts);
        self.apply_panel_batch(batch);
        self.apply_conditions();
        if let Some(generation) = finishes
            && self.refreshes.is_current(generation)
            && let Some(mut pipeline) = self.refreshes.pipeline.take()
        {
            pipeline.counts.add(counts);
            self.complete_refresh(pipeline);
        }
        true
    }

    /// After variables resolve: fetches every panel again when they changed
    /// the layout, otherwise only panels that conditions newly reveal.
    fn continue_refresh(&mut self, mut pipeline: Pipeline) {
        let indices = if pipeline.variables_changed {
            self.panels_to_fetch()
        } else {
            self.apply_conditions();
            self.panels_to_fetch()
                .into_iter()
                .filter(|index| !pipeline.fetched.contains(index))
                .collect()
        };
        if indices.is_empty() {
            self.apply_conditions();
            self.complete_refresh(pipeline);
            return;
        }
        pipeline.abort =
            self.spawn_panel_batch(&indices, Some(pipeline.generation), pipeline.end_ts);
        self.refreshes.pipeline = Some(pipeline);
    }

    fn complete_refresh(&mut self, pipeline: Pipeline) {
        self.reconcile_visible_annotation_targets();
        self.last_refresh = Instant::now();
        let counts = pipeline.counts;
        let unreachable = counts.unreachable > 0 && counts.succeeded == 0;
        self.refreshes.status = if unreachable {
            let failures = match self.refreshes.status {
                BackendStatus::Unreachable { failures } => failures.saturating_add(1),
                BackendStatus::Connecting | BackendStatus::Live => 1,
            };
            BackendStatus::Unreachable { failures }
        } else {
            BackendStatus::Live
        };
        self.refreshes.unreachable_cause = counts.tls_failure.filter(|_| unreachable);
    }

    fn start_annotation_refresh(&mut self, context: AnnotationRefreshContext) {
        if self.refreshes.annotations_running {
            self.refreshes.annotations_queued = Some(context);
            return;
        }
        let Some(mut provider) = self.annotations.take_provider() else {
            return;
        };
        self.refreshes.annotations_running = true;
        self.refreshes.tasks.spawn(async move {
            let poll = provider.refresh(&context).await;
            RefreshMessage::Annotations { provider, poll }
        });
    }

    fn spawn_section_refresh(
        &mut self,
        generation: u64,
        end_ts: i64,
        update: VariableUpdate,
    ) -> AbortHandle {
        let prometheus = self.prometheus.clone();
        let instances = self.section_instances.clone();
        let vars = self.vars.clone();
        let mut values = self.section_values.clone();
        let window = self.query_window(end_ts);
        let epoch = self.refreshes.layout_epoch;
        self.refreshes.tasks.spawn(async move {
            let report =
                resolve_sections(&prometheus, &instances, update, &vars, &mut values, window).await;
            RefreshMessage::Sections {
                generation,
                epoch,
                values,
                report,
            }
        })
    }

    fn spawn_panel_batch(
        &mut self,
        indices: &[usize],
        pipeline: Option<u64>,
        end_ts: i64,
    ) -> AbortHandle {
        let jobs = self.panel_jobs(indices);
        let seq = self.refreshes.next_seq();
        let epoch = self.refreshes.layout_epoch;
        let prometheus = self.prometheus.clone();
        let vars = self.vars.clone();
        let window = self.query_window(end_ts);
        self.refreshes.tasks.spawn(async move {
            let (fetches, counts) = fetch_panels(&prometheus, window, &vars, jobs).await;
            RefreshMessage::Panels(PanelBatch {
                seq,
                epoch,
                pipeline,
                fetches,
                counts,
            })
        })
    }

    fn query_window(&self, end_ts: i64) -> QueryWindow {
        QueryWindow {
            range: self.range,
            min_step: self.min_step,
            scrape_interval: self.scrape_interval,
            end_ts,
        }
    }

    fn panel_jobs(&mut self, indices: &[usize]) -> Vec<PanelJob> {
        let indices = indices.iter().copied().collect::<HashSet<_>>();
        let mut jobs = Vec::with_capacity(indices.len());
        for (index, panel) in self.panels.iter_mut().enumerate() {
            if !indices.contains(&index) {
                continue;
            }
            // The copy only needs the queries, so the data is left out.
            let series = std::mem::take(&mut panel.series);
            let copy = panel.clone();
            panel.series = series;
            jobs.push(PanelJob {
                index,
                panel: copy,
                scope: self.panel_scopes.get(&index).cloned(),
            });
        }
        jobs
    }

    fn apply_panel_batch(&mut self, batch: PanelBatch) {
        if batch.epoch != self.refreshes.layout_epoch {
            return;
        }
        for fetch in batch.fetches {
            let Some(panel) = self.panels.get_mut(fetch.index) else {
                continue;
            };
            let applied = self.refreshes.applied_seq.entry(fetch.index).or_default();
            if *applied > batch.seq {
                continue;
            }
            *applied = batch.seq;
            if !fetch.succeeded && !fetch.errors.is_empty() && !panel.series.is_empty() {
                // Every query failed, so the last data stays on screen,
                // marked stale, until Prometheus answers again.
                panel.notices.stale = true;
            } else {
                // Series the user hid stay hidden across refreshes.
                let hidden: HashSet<&str> = panel
                    .series
                    .iter()
                    .filter(|series| !series.visible)
                    .map(|series| series.name.as_str())
                    .collect();
                let mut series = fetch.series;
                for series in &mut series {
                    series.visible = !hidden.contains(series.name.as_str());
                }
                panel.series = series;
                panel.last_samples = panel.series.iter().map(|s| s.points.len()).sum();
                panel.notices.stale = false;
            }
            if let Some(url) = fetch.url {
                panel.last_url = Some(url);
            }
            panel.last_error = (!fetch.errors.is_empty()).then(|| fetch.errors.join("\n"));
            panel.notices.warnings = fetch.warnings;
            panel.notices.infos = fetch.infos;
        }
    }
}

async fn refresh_variables(
    prometheus: &prom::PromClient,
    query_vars: &[TemplateQueryVar],
    update: VariableUpdate,
    window: QueryWindow,
    vars: &mut HashMap<String, String>,
    var_values: &mut HashMap<String, Vec<String>>,
) -> VariableReport {
    let intervals =
        QueryIntervals::new(window.range, window.min_step, window.scrape_interval, None);
    refresh_query_variables(
        prometheus,
        query_vars,
        update,
        window.range,
        intervals,
        window.end_ts,
        vars,
        var_values,
    )
    .await
}

/// Resolves the query variables of every row and tab copy that defines
/// them, expanded with the variables around that copy. Copies shown for the
/// first time resolve as on load.
async fn resolve_sections(
    prometheus: &prom::PromClient,
    instances: &[SectionInstance],
    update: VariableUpdate,
    vars: &HashMap<String, String>,
    values: &mut ResolvedSections,
    window: QueryWindow,
) -> VariableReport {
    let mut report = VariableReport::default();
    let intervals =
        QueryIntervals::new(window.range, window.min_step, window.scrape_interval, None);
    for instance in instances {
        let key = (instance.id, instance.scope.clone());
        let mut vars = scoped_vars(vars, Some(&instance.scope)).into_owned();
        let (update, mut selected) = match values.get(&key) {
            Some(selected) => (update, selected.clone()),
            None => (VariableUpdate::Load, instance.selected.clone()),
        };
        let instance_report = refresh_query_variables(
            prometheus,
            &instance.queries,
            update,
            window.range,
            intervals,
            window.end_ts,
            &mut vars,
            &mut selected,
        )
        .await;
        report.queried.extend(instance_report.queried);
        report.errors.extend(instance_report.errors);
        values.insert(key, selected);
    }
    report
}

async fn fetch_panels(
    prometheus: &prom::PromClient,
    window: QueryWindow,
    vars: &HashMap<String, String>,
    jobs: Vec<PanelJob>,
) -> (Vec<PanelFetch>, FetchCounts) {
    let results = futures::stream::iter(jobs)
        .map(|job| async move {
            let vars = scoped_vars(vars, job.scope.as_ref());
            fetch_panel(prometheus, job.index, &job.panel, window, &vars).await
        })
        .buffer_unordered(CONCURRENT_PANELS)
        .collect::<Vec<_>>()
        .await;
    let mut counts = FetchCounts::default();
    let fetches = results
        .into_iter()
        .map(|(fetch, panel_counts)| {
            counts.add(panel_counts);
            fetch
        })
        .collect();
    (fetches, counts)
}

async fn fetch_panel(
    prometheus: &prom::PromClient,
    index: usize,
    p: &PanelState,
    window: QueryWindow,
    vars: &HashMap<String, String>,
) -> (PanelFetch, FetchCounts) {
    let QueryWindow {
        range,
        min_step,
        scrape_interval,
        end_ts,
    } = window;
    let mut panel_results = Vec::new();
    let mut last_url = None;
    let mut errors = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut infos: Vec<String> = Vec::new();
    let mut counts = FetchCounts::default();

    for (i, expr) in p.exprs.iter().enumerate() {
        let intervals = p.query_intervals(i, range, min_step, scrape_interval, vars);
        let step = intervals.step;
        let expr_expanded = expand_expr(expr, range, intervals, vars);
        let legend_fmt = custom_legend(p.legends.get(i).and_then(Option::as_ref));
        let query_mode = p.query_mode(i);

        // Calculate start/end for URL display purposes
        let start_ts = end_ts - (range.as_secs() as i64);

        let url = match query_mode {
            QueryMode::Range => {
                prometheus.build_query_range_url(&expr_expanded, start_ts, end_ts, step)
            }
            QueryMode::Instant => prometheus.build_query_url(&expr_expanded, end_ts),
        };
        last_url = Some(url);

        let query_result = match query_mode {
            QueryMode::Range => {
                prometheus
                    .query_range(&expr_expanded, start_ts, end_ts, step)
                    .await
            }
            QueryMode::Instant => {
                prometheus
                    .query_instant_series(&expr_expanded, end_ts)
                    .await
            }
        };

        match query_result {
            Ok(result) => {
                counts.succeeded += 1;
                for warning in result.warnings {
                    if !warnings.contains(&warning) {
                        warnings.push(warning);
                    }
                }
                for info in result.infos {
                    if !infos.contains(&info) {
                        infos.push(info);
                    }
                }
                for s in result.series {
                    // NaN and ±Inf show as no value, as they cannot be
                    // scaled for gauges or compared with thresholds.
                    let latest_val = s
                        .values
                        .last()
                        .and_then(|(_, v)| v.parse::<f64>().ok())
                        .filter(|v| v.is_finite());
                    let legend_base = if let Some(fmt) = legend_fmt {
                        format_legend(fmt, &s.metric)
                    } else if s.metric.is_empty() {
                        expr_expanded.clone()
                    } else {
                        let mut labels: Vec<_> = s
                            .metric
                            .iter()
                            .map(|(k, v)| format!("{}=\"{}\"", k, v))
                            .collect();
                        labels.sort();
                        format!("{} {{{}}}", expr_expanded, labels.join(", "))
                    };

                    let mut pts = Vec::with_capacity(s.values.len());
                    for (ts, val) in s.values {
                        if let Ok(y) = val.parse::<f64>()
                            && y.is_finite()
                        {
                            pts.push((ts, y));
                        }
                    }
                    panel_results.push(SeriesView {
                        name: legend_base,
                        value: latest_val,
                        points: downsample(pts, 200),
                        visible: true,
                    });
                }
            }
            Err(e) => {
                if prom::is_transport_error(&e) {
                    counts.unreachable += 1;
                    if counts.tls_failure.is_none() {
                        counts.tls_failure = prom::tls_failure_cause(&e).map(str::to_string);
                    }
                }
                let query_name = match query_mode {
                    QueryMode::Range => "query_range",
                    QueryMode::Instant => "query",
                };
                errors.push(format!(
                    "{} failed for `{}`: {}",
                    query_name, expr_expanded, e
                ));
            }
        }
    }
    let fetch = PanelFetch {
        index,
        series: panel_results,
        url: last_url,
        errors,
        warnings,
        infos,
        succeeded: counts.succeeded > 0,
    };
    (fetch, counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::ProviderFuture;
    use crate::app::{PanelOptions, PanelType, YAxisMode};
    use crate::export::ExportOptions;
    use crate::grafana::VariableRefresh;
    use crate::theme::Theme;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Size;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const EMPTY_VECTOR: &str = r#"{"status":"success","data":{"resultType":"vector","result":[]}}"#;

    fn app_with_panels(url: &str, count: usize) -> AppState {
        let panels = (0..count)
            .map(|index| PanelState {
                title: format!("Panel {index}"),
                exprs: vec!["up".to_string()],
                legends: vec![None],
                query_modes: vec![QueryMode::Instant],
                series: vec![],
                last_error: None,
                last_url: None,
                last_samples: 0,
                grid: None,
                y_axis_mode: YAxisMode::Auto,
                panel_type: PanelType::Graph,
                thresholds: None,
                min: None,
                max: None,
                autogrid: None,
                display: crate::ui::DisplayFormat::default(),
                options: PanelOptions::None,
                resolution: Default::default(),
                notices: Default::default(),
            })
            .collect();
        AppState::new(
            prom::PromClient::new(url.to_string()),
            Duration::from_secs(300),
            Duration::from_secs(5),
            Duration::from_secs(1),
            "Test".to_string(),
            panels,
            0,
            Theme::default(),
            "dashed".to_string(),
            ExportOptions::default(),
        )
    }

    /// Accepts connections and never answers.
    async fn hung_prometheus() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                held.push(socket);
            }
        });
        format!("http://{address}")
    }

    /// An address nothing listens on, so connections are refused.
    fn refused_prometheus() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    }

    /// Answers every request with `status` and `body`.
    async fn prometheus_answering(status: &'static str, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut chunk = [0_u8; 1024];
                    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                        let read = socket.read(&mut chunk).await.unwrap();
                        if read == 0 {
                            return;
                        }
                        request.extend_from_slice(&chunk[..read]);
                    }
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len(),
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                });
            }
        });
        format!("http://{address}")
    }

    fn panel_batch(seq: u64, epoch: u64, name: &str) -> PanelBatch {
        PanelBatch {
            seq,
            epoch,
            pipeline: None,
            fetches: vec![PanelFetch {
                index: 0,
                series: vec![SeriesView {
                    name: name.to_string(),
                    value: Some(1.0),
                    points: vec![],
                    visible: true,
                }],
                url: None,
                errors: vec![],
                warnings: vec![],
                infos: vec![],
                succeeded: true,
            }],
            counts: FetchCounts::default(),
        }
    }

    fn failed_batch(seq: u64, errors: &[&str], succeeded: Option<&str>) -> PanelBatch {
        let mut batch = panel_batch(seq, 0, succeeded.unwrap_or_default());
        let fetch = &mut batch.fetches[0];
        if succeeded.is_none() {
            fetch.series.clear();
        }
        fetch.errors = errors.iter().map(|error| error.to_string()).collect();
        fetch.succeeded = succeeded.is_some();
        batch
    }

    #[test]
    fn failed_fetches_keep_the_last_data_marked_stale() {
        let mut app = app_with_panels("http://127.0.0.1:9", 1);
        app.finish_refresh_task(Ok(RefreshMessage::Panels(panel_batch(1, 0, "cpu"))));
        app.panels[0].series[0].visible = false;

        let failed = failed_batch(2, &["request failed: connection refused"], None);
        app.finish_refresh_task(Ok(RefreshMessage::Panels(failed)));

        let panel = &app.panels[0];
        assert_eq!(panel.series[0].name, "cpu");
        assert!(!panel.series[0].visible);
        assert!(panel.notices.stale);
        assert_eq!(
            panel.last_error.as_deref(),
            Some("request failed: connection refused")
        );

        app.finish_refresh_task(Ok(RefreshMessage::Panels(panel_batch(3, 0, "cpu"))));
        let panel = &app.panels[0];
        assert!(!panel.notices.stale);
        assert!(panel.last_error.is_none());
        assert!(!panel.series[0].visible);
    }

    #[test]
    fn partly_failed_fetches_show_the_queries_that_succeeded() {
        let mut app = app_with_panels("http://127.0.0.1:9", 1);
        app.finish_refresh_task(Ok(RefreshMessage::Panels(panel_batch(1, 0, "old"))));

        let partial = failed_batch(2, &["first failed", "second failed"], Some("new"));
        app.finish_refresh_task(Ok(RefreshMessage::Panels(partial)));

        let panel = &app.panels[0];
        assert_eq!(panel.series[0].name, "new");
        assert!(!panel.notices.stale);
        assert_eq!(
            panel.last_error.as_deref(),
            Some("first failed\nsecond failed")
        );
    }

    #[test]
    fn a_first_fetch_that_fails_has_no_data_to_keep() {
        let mut app = app_with_panels("http://127.0.0.1:9", 1);

        let failed = failed_batch(1, &["request failed"], None);
        app.finish_refresh_task(Ok(RefreshMessage::Panels(failed)));

        let panel = &app.panels[0];
        assert!(panel.series.is_empty());
        assert!(!panel.notices.stale);
        assert!(panel.last_error.is_some());
    }

    #[tokio::test]
    async fn warnings_are_kept_until_a_fetch_without_them() {
        let url = prometheus_answering(
            "200 OK",
            r#"{"status":"success","data":{"resultType":"vector","result":[
                {"metric":{"job":"api"},"value":[1700000000,"1"]}]},
                "warnings":["partial response"],
                "infos":["metric might not be a counter"]}"#,
        )
        .await;
        let mut app = app_with_panels(&url, 1);

        app.refresh().await.unwrap();
        assert_eq!(app.panels[0].notices.warnings, ["partial response"]);
        assert_eq!(
            app.panels[0].notices.infos,
            ["metric might not be a counter"]
        );
        assert_eq!(app.panels[0].series.len(), 1);

        // Prometheus goes away: the data stays, marked stale.
        app.prometheus = prom::PromClient::new(refused_prometheus());
        app.refresh().await.unwrap();
        let panel = &app.panels[0];
        assert_eq!(panel.series.len(), 1);
        assert!(panel.notices.stale);
        assert!(
            panel
                .last_error
                .as_deref()
                .unwrap()
                .contains("request failed")
        );
        assert!(panel.notices.warnings.is_empty());
        assert!(panel.notices.infos.is_empty());

        app.prometheus = prom::PromClient::new(prometheus_answering("200 OK", EMPTY_VECTOR).await);
        app.refresh().await.unwrap();
        let panel = &app.panels[0];
        assert!(!panel.notices.stale);
        assert!(panel.series.is_empty());
        assert!(panel.last_error.is_none());
    }

    /// Answers with `respond(path)`'s status and body, recording each path.
    async fn recording_prometheus(
        respond: fn(&str) -> (&'static str, &'static str),
    ) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let recorded = Arc::clone(&recorded);
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut chunk = [0_u8; 1024];
                    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                        let read = socket.read(&mut chunk).await.unwrap();
                        if read == 0 {
                            return;
                        }
                        request.extend_from_slice(&chunk[..read]);
                    }
                    let request = String::from_utf8_lossy(&request);
                    let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
                    let (status, body) = respond(&path);
                    recorded
                        .lock()
                        .unwrap()
                        .push(urlencoding::decode(&path).unwrap().into_owned());
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len(),
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                });
            }
        });
        (format!("http://{address}"), requests)
    }

    fn query_var(name: &str, query: &str, refresh: VariableRefresh) -> TemplateQueryVar {
        TemplateQueryVar {
            name: name.to_string(),
            query: query.to_string(),
            regex: None,
            query_path: format!("templating.list[{name}].query"),
            select_all: false,
            all_value: None,
            regex_values: false,
            refresh,
        }
    }

    fn requests_to(requests: &Mutex<Vec<String>>, prefix: &str) -> Vec<String> {
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn label_values(path: &str) -> (&'static str, &'static str) {
        let body = if path.contains("job=\"web\"") || path.contains("job%3D%22web%22") {
            r#"{"status":"success","data":["web-1"]}"#
        } else if path.starts_with("/api/v1/label/job/") {
            r#"{"status":"success","data":["api","web"]}"#
        } else if path.starts_with("/api/v1/label/instance/") {
            r#"{"status":"success","data":["api-1"]}"#
        } else if path.starts_with("/api/v1/label/broken/") {
            return (
                "400 Bad Request",
                r#"{"status":"error","errorType":"bad_data","error":"invalid label"}"#,
            );
        } else {
            EMPTY_VECTOR
        };
        ("200 OK", body)
    }

    #[tokio::test]
    async fn on_load_variables_resolve_once() {
        let (url, requests) = recording_prometheus(label_values).await;
        let mut app = app_with_panels(&url, 1);
        app.query_vars = vec![query_var(
            "job",
            "label_values(job)",
            VariableRefresh::OnLoad,
        )];

        app.refresh().await.unwrap();
        app.refresh().await.unwrap();
        app.zoom_out();
        app.refresh().await.unwrap();
        app.reload();
        app.settle_refreshes().await;

        let job = requests_to(&requests, "/api/v1/label/job/");
        assert_eq!(job.len(), 1, "{job:?}");
        assert!(
            job[0].contains("start=") && job[0].contains("end="),
            "{job:?}"
        );
        assert_eq!(app.vars["job"], "api");
    }

    #[tokio::test]
    async fn time_range_variables_resolve_when_the_range_changes() {
        let (url, requests) = recording_prometheus(label_values).await;
        let mut app = app_with_panels(&url, 1);
        app.query_vars = vec![query_var(
            "job",
            "label_values(job)",
            VariableRefresh::OnTimeRangeChange,
        )];

        app.refresh().await.unwrap();
        // Periodic refreshes of the same window leave variables alone.
        app.refresh().await.unwrap();
        assert_eq!(requests_to(&requests, "/api/v1/label/job/").len(), 1);

        app.zoom_out();
        app.refresh().await.unwrap();
        assert_eq!(requests_to(&requests, "/api/v1/label/job/").len(), 2);

        // A refresh the user asks for counts as a time range change.
        app.reload();
        app.settle_refreshes().await;
        assert_eq!(requests_to(&requests, "/api/v1/label/job/").len(), 3);
    }

    #[tokio::test]
    async fn dependent_variables_follow_a_changed_variable() {
        let (url, requests) = recording_prometheus(label_values).await;
        let mut app = app_with_panels(&url, 1);
        app.query_vars = vec![
            query_var(
                "job",
                "label_values(job)",
                VariableRefresh::OnTimeRangeChange,
            ),
            query_var(
                "instance",
                r#"label_values(up{job="$job"}, instance)"#,
                VariableRefresh::OnLoad,
            ),
        ];
        app.vars.insert("job".to_string(), "web".to_string());
        app.var_values
            .insert("job".to_string(), vec!["web".to_string()]);

        app.refresh().await.unwrap();
        let instance = requests_to(&requests, "/api/v1/label/instance/");
        assert!(
            instance[0].contains(r#"match[]=up{job="web"}"#),
            "{instance:?}"
        );
        assert_eq!(app.vars["instance"], "web-1");

        // `job` resolves to a new value, so `instance`, which references it,
        // resolves again even though it only refreshes on load.
        app.var_values
            .insert("job".to_string(), vec!["retired".to_string()]);
        app.zoom_out();
        app.refresh().await.unwrap();
        let instance = requests_to(&requests, "/api/v1/label/instance/");
        assert_eq!(instance.len(), 2, "{instance:?}");
        assert!(
            instance[1].contains(r#"match[]=up{job="api"}"#),
            "{instance:?}"
        );

        // Unchanged, it does not.
        app.zoom_out();
        app.refresh().await.unwrap();
        assert_eq!(requests_to(&requests, "/api/v1/label/instance/").len(), 2);
    }

    #[tokio::test]
    async fn a_failing_variable_keeps_its_value_and_the_rest_resolve() {
        let (url, _) = recording_prometheus(label_values).await;
        let mut app = app_with_panels(&url, 1);
        app.query_vars = vec![
            query_var(
                "broken",
                "label_values(broken)",
                VariableRefresh::OnTimeRangeChange,
            ),
            query_var("job", "label_values(job)", VariableRefresh::OnLoad),
        ];
        app.vars.insert("broken".to_string(), "saved".to_string());

        app.refresh().await.unwrap();

        assert_eq!(app.vars["broken"], "saved");
        assert_eq!(app.vars["job"], "api");
        let errors: Vec<_> = app.variable_errors().collect();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "broken");
        assert!(
            errors[0].1.contains("bad_data: invalid label"),
            "{errors:?}"
        );

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(160, 30)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw_ui(frame, &mut app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("Variable broken failed: prometheus 400 Bad Request"));
    }

    #[tokio::test]
    async fn input_is_handled_while_prometheus_hangs() {
        let mut app = app_with_panels(&hung_prometheus().await, 1);
        app.start_refresh();
        assert_eq!(app.backend_indicator(), Some(BackendIndicator::Connecting));
        assert_eq!(app.next_refresh_at(), None);
        assert!(!app.panels[0].has_loaded());

        let range = app.range;
        let zoom_out = KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE);
        let handled = crate::app::input::handle_key(zoom_out, Size::new(100, 40), &mut app);
        tokio::time::timeout(Duration::from_millis(500), handled)
            .await
            .expect("input waited for Prometheus")
            .unwrap();

        assert_eq!(app.range, range * 2);
        // The zoom replaced the hung refresh instead of queueing behind it.
        assert_eq!(app.refreshes.generation, 2);
        let replaced = tokio::time::timeout(Duration::from_secs(1), app.join_refresh_task())
            .await
            .expect("the replaced refresh was not aborted")
            .unwrap();
        assert!(replaced.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn unreachable_backend_backs_off_until_it_answers() {
        let mut app = app_with_panels(&refused_prometheus(), 1);

        app.refresh().await.unwrap();
        assert_eq!(
            app.backend_status(),
            BackendStatus::Unreachable { failures: 1 }
        );
        assert_eq!(app.backend_indicator(), Some(BackendIndicator::Unreachable));
        assert!(app.panels[0].last_error.is_some());
        let next = app.next_refresh_at().unwrap();
        assert_eq!(next - app.last_refresh, Duration::from_secs(2));

        app.refresh().await.unwrap();
        assert_eq!(
            app.backend_status(),
            BackendStatus::Unreachable { failures: 2 }
        );
        let next = app.next_refresh_at().unwrap();
        assert_eq!(next - app.last_refresh, Duration::from_secs(4));

        app.refreshes.status = BackendStatus::Unreachable { failures: 10 };
        let next = app.next_refresh_at().unwrap();
        assert_eq!(next - app.last_refresh, MAX_BACKOFF);

        app.prometheus = prom::PromClient::new(prometheus_answering("200 OK", EMPTY_VECTOR).await);
        app.refresh().await.unwrap();
        assert_eq!(app.backend_status(), BackendStatus::Live);
        assert_eq!(app.backend_indicator(), None);
        assert!(app.panels[0].last_error.is_none());
        let next = app.next_refresh_at().unwrap();
        assert_eq!(next - app.last_refresh, app.refresh_every);
    }

    #[tokio::test]
    async fn tls_failures_say_why_prometheus_is_unreachable() {
        use prom::tls_tests::{TestCa, mtls_prometheus, test_dir, tls_files};
        let (server_ca, client_ca) = (TestCa::new("server CA"), TestCa::new("client CA"));
        let (url, _) = mtls_prometheus(&server_ca.server_cert(), &client_ca).await;
        let dir = test_dir("refresh-cause");
        let mut app = app_with_panels(&url, 1);
        app.prometheus =
            prom::PromClient::with_tls(url.clone(), &tls_files(&dir, &server_ca.pem(), None))
                .unwrap();

        app.refresh().await.unwrap();

        assert_eq!(
            app.backend_status(),
            BackendStatus::Unreachable { failures: 1 }
        );
        assert_eq!(
            app.unreachable_cause(),
            Some("TLS: client certificate required")
        );
        let next = app.next_refresh_at().unwrap();
        assert_eq!(next - app.last_refresh, Duration::from_secs(2));

        let files = tls_files(&dir, &server_ca.pem(), Some(&client_ca.client_cert()));
        app.prometheus = prom::PromClient::with_tls(url, &files).unwrap();
        app.refresh().await.unwrap();
        assert_eq!(app.backend_status(), BackendStatus::Live);
        assert_eq!(app.unreachable_cause(), None);
    }

    #[tokio::test]
    async fn query_errors_do_not_mean_prometheus_is_unreachable() {
        let url = prometheus_answering(
            "400 Bad Request",
            r#"{"status":"error","errorType":"bad_data","error":"parse error"}"#,
        )
        .await;
        let mut app = app_with_panels(&url, 2);

        app.refresh().await.unwrap();

        assert_eq!(app.backend_status(), BackendStatus::Live);
        assert!(app.panels.iter().all(|panel| panel.last_error.is_some()));
    }

    #[test]
    fn older_panel_fetches_never_replace_newer_data() {
        let mut app = app_with_panels("http://127.0.0.1:9", 1);

        assert!(app.finish_refresh_task(Ok(RefreshMessage::Panels(panel_batch(2, 0, "newer")))));
        app.finish_refresh_task(Ok(RefreshMessage::Panels(panel_batch(1, 0, "older"))));
        assert_eq!(app.panels[0].series[0].name, "newer");

        // Indices may point at different panels after the layout is rebuilt.
        app.refreshes.layout_changed();
        app.finish_refresh_task(Ok(RefreshMessage::Panels(panel_batch(3, 0, "rebuilt"))));
        assert_eq!(app.panels[0].series[0].name, "newer");
    }

    #[tokio::test]
    async fn results_of_a_replaced_refresh_are_ignored() {
        let mut app = app_with_panels(&hung_prometheus().await, 1);
        app.start_refresh();
        app.start_refresh();

        let stale = RefreshMessage::Data {
            generation: 1,
            vars: HashMap::from([("job".to_string(), "stale".to_string())]),
            var_values: HashMap::new(),
            report: VariableReport::default(),
            batch: panel_batch(99, 0, "stale"),
        };

        assert!(!app.finish_refresh_task(Ok(stale)));
        assert!(app.vars.is_empty());
        assert!(app.panels[0].series.is_empty());
        assert!(app.refreshes.is_current(2));
    }

    #[tokio::test]
    async fn a_slow_refresh_is_shown_once() {
        let mut app = app_with_panels(&hung_prometheus().await, 1);
        app.refreshes.status = BackendStatus::Live;
        app.start_refresh();
        assert_eq!(app.backend_indicator(), None);
        let started = app.refreshes.pipeline.as_ref().unwrap().started;
        assert_eq!(app.slow_refresh_at(), Some(started + SLOW_REFRESH));

        app.refreshes.pipeline.as_mut().unwrap().started -= SLOW_REFRESH;
        assert_eq!(app.backend_indicator(), Some(BackendIndicator::Refreshing));
        app.mark_slow_refresh_drawn();
        assert_eq!(app.slow_refresh_at(), None);
    }

    #[derive(Debug)]
    struct WindowRecorder {
        windows: Arc<Mutex<Vec<AnnotationRefreshContext>>>,
    }

    impl AnnotationProvider for WindowRecorder {
        fn refresh<'a>(&'a mut self, context: &'a AnnotationRefreshContext) -> ProviderFuture<'a> {
            self.windows.lock().unwrap().push(context.clone());
            Box::pin(async { ProviderPoll::Unchanged })
        }
    }

    #[tokio::test]
    async fn annotation_loads_queue_behind_a_running_one() {
        let windows = Arc::new(Mutex::new(Vec::new()));
        let mut app = app_with_panels("http://127.0.0.1:9", 0);
        app.annotations =
            crate::annotations::AnnotationState::from_provider(Some(Box::new(WindowRecorder {
                windows: Arc::clone(&windows),
            })));

        app.start_refresh();
        app.zoom_out();
        app.start_refresh();
        app.settle_refreshes().await;

        let windows = windows.lock().unwrap();
        assert_eq!(windows.len(), 2);
        assert_eq!(
            windows[1].to - windows[1].from,
            chrono::TimeDelta::from_std(app.range).unwrap()
        );
        assert!(!app.refreshes.annotations_running);
        assert!(app.annotations.take_provider().is_some());
    }
}
