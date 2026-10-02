# Grafatui Roadmap

Grafatui is a terminal-based Grafana-like UI for Prometheus. The roadmap is
oriented around three priorities:

1. **Production readiness** - the data path must be correct, responsive, and
   able to reach both unsecured and secured Prometheus-compatible backends.
2. **Grafana parity** - imported dashboards should preserve as much meaning as a
   terminal UI can reasonably express.
3. **User-visible product value** - parity work should make real dashboards
   easier to read, debug, and share.

> **Current version**: 0.1.12 · **Status**: Active development, pre-1.0

**Legend**:
- 🟢 Low complexity · 🟡 Medium complexity · 🔴 High complexity
- ✅ Shipped · 🔶 Partial · 🔜 Up next · 📋 Planned · 💡 Exploring

---

## What's Already Built (v0.1.x)

These features are shipped and available today:

| Area | Feature | Details |
|---|---|---|
| Panels | **7 panel type aliases** | `graph`, `timeseries`, `stat`, `gauge`, `bargauge`, `table`, `heatmap` |
| Import | **Grafana JSON import** | Load exported dashboards from local JSON files |
| Import | **24-column grid layout** | Faithful reproduction of Grafana's `gridPos` positioning |
| Variables | **Template variables** | `$var` / `${var}` substitution with CLI and config overrides |
| Variables | **Dynamic Prometheus variables** | Query-backed variables using `label_values(...)` and `query_result(...)` |
| Variables | **PromQL built-ins** | `$__interval`, `$__interval_ms`, `$__range`, `$__range_s`, `$__range_ms`, `$__rate_interval` |
| Queries | **Multiple targets per panel** | Multiple PromQL expressions render as separate series |
| Queries | **Instant target queries** | Honors `targets[].instant` and defaults summary table/gauge panels to Prometheus instant queries |
| Queries | **Legend formatting** | `{{label}}` syntax from Grafana |
| UI | **19 color themes** | Tokyo Night, Catppuccin and Gruvbox flavors, dracula, monokai, solarized dark/light, terminal |
| UI | **Time controls** | Zoom in/out, pan left/right, live mode toggle |
| UI | **Panel navigation** | Arrow keys, vim-style `j`/`k`, PgUp/PgDn, fullscreen, inspect mode |
| UI | **Panel search** | `/` to fuzzy-search panels by name |
| UI | **Interactive dashboard rows** | Classic row headers and collapsed state plus nested V2 `RowsLayout`; Enter/Space toggles, Left collapses, and Right expands selected rows |
| UI | **Interactive dashboard tabs** | Nested V2 `TabsLayout`; Left/Right switches the focused tab and Enter/Space enters its content |
| UI | **Mouse support** | Click to select, scroll, drag cursor in fullscreen inspect mode |
| UI | **Value inspection** | Cursor-based point-in-time data exploration |
| UI | **Series toggling** | Show/hide individual series with `1`-`9` |
| Annotations | **External annotation providers (iterations 1–3)** | Read-only file/command JSONL events on graph/timeseries panels, targeting, tag filtering, bounded provider protocol, concurrent refresh, export, and recording parity |
| Rendering | **Smart caching** | Request deduplication and caching for identical queries |
| Rendering | **Downsampling** | Max-pooling to preserve peaks while fitting the terminal |
| Rendering | **Adaptive time labels** | Date/time axis labels adjust to the selected range |
| Rendering | **Autogrid** | Global and per-panel guide lines, including configurable color |
| Field config | **Thresholds** | `fieldConfig.defaults.thresholds` for graph limit lines and Stat/Gauge/BarGauge coloring |
| Field config | **Threshold marker styles** | Dashed line, dot, braille, block, quadrant, sextant, octant, and related modes |
| Field config | **Field min/max bounds** | `fieldConfig.defaults.min` / `max` for graph y-axis bounds, gauge scaling, and percentage thresholds |
| Field config | **Display formatting subset** | Common `unit` values, `decimals`, and `noValue` for supported panel values, axes, legends, and exports |
| Export | **SVG/PNG snapshots** | Export the visible dashboard to SVG, PNG, or both |
| Export | **Recording frame bundles** | Changed-frame recordings with manifest metadata and frame caps |
| Config | **Config file** | TOML-based persistent configuration |
| Distribution | **Shell completions and man page** | Bash, Zsh, Fish, PowerShell, Elvish, plus generated man page |
| Distribution | **Cross-platform binaries** | Linux, macOS, and Windows release assets |
| Distribution | **Package formats** | `.deb`, `.rpm`, Homebrew formula support |

For a field-by-field breakdown of Grafana JSON compatibility, see the
[compatibility matrix](docs/grafana-compatibility.md). Keep that document
refreshed alongside parity work so it stays aligned with the current release.

### External Annotation Iterations

| Iteration | Scope | Status |
|---|---|---|
| 1 | External JSONL point annotations | ✅ Shipped |
| 2 | Navigable annotation popup, panel targeting, and tag filtering | ✅ Shipped |
| 3 | Bounded command provider protocol and Prometheus-coordinated refresh | ✅ Shipped |

These iterations are separate from Grafana `annotations` and `annotations.list`,
which remain unsupported.

### Provider Ecosystem and Integrations (Post-Core)

| Item | User value | Complexity | Status |
|---|---|---|---|
| Independent provider scheduling and redraw | Refresh slow providers without coupling them to Prometheus redraws | 🟡 | 📋 |
| Stable public Rust provider API | Build supported native providers outside Grafatui | 🟡 | 📋 |
| Possible Python SDK | Make provider authoring accessible without Rust | 🟡 | 💡 |
| Long-running providers | Reuse authenticated clients and streams safely | 🔴 | 📋 |
| Multiple annotation sources | Combine independent event systems in one overlay | 🔴 | 📋 |
| Range events | Show maintenance windows and other duration-based annotations | 🟡 | 📋 |
| Stable event IDs | Support deduplication and reliable provider updates | 🟡 | 📋 |
| Per-panel tag filters | Let each panel narrow the shared annotation stream | 🟡 | 📋 |
| Reference/community CI/CD providers | Turn durable CI/CD deployment records into useful overlays | 🟡 | 📋 |
| HTTP, Grafana, and vendor adapters | Add integrations only where demonstrated demand exists | 🔴 | 💡 |

---

## Roadmap Principles

Roadmap items are prioritized by:

1. **Grafana parity impact** - Does this make imported Grafana dashboards behave
   more like users expect?
2. **User-visible value** - Does this make dashboards more readable, trustworthy,
   or useful in a terminal?
3. **Trust in import results** - Does this reduce silent degradation when a
   dashboard contains unsupported fields?
4. **Implementation complexity** - Can the feature be shipped safely in a small,
   reviewable step?

The intent is to make parity work feel practical rather than academic. For
example, unit support is not just `fieldConfig.defaults.unit`; it is the
difference between readable latency/cache panels and raw float noise.

---

## Production Readiness

The compatibility ladder below assumes the data path is trustworthy. A
code audit on 2026-10-01 found gaps that block production use regardless of
how faithfully a dashboard imports, so this track comes first.

Plain HTTP with no authentication stays the zero-config default. Every auth
and TLS setting below is opt-in, so unsecured local and lab Prometheus
endpoints keep working unchanged.

### Correctness and Robustness

| Item | Why it blocks production | Complexity | Status |
|---|---|---|---|
| **Adaptive query step** | The fixed `--step` (default 5s) exceeded Prometheus's 11,000-point limit above about 15h, so long ranges and zoom-out failed; steps now scale with the range, honoring `maxDataPoints` and min intervals | 🟡 | ✅ |
| **Non-blocking refresh** | Refresh ran inline in the event loop, so a slow or unreachable backend froze input and delayed startup; refreshes now run in the background, with a connection indicator and retry backoff | 🔴 | ✅ |
| **Variable refresh policy** | Query variables are re-queried every refresh tick and their errors are swallowed; Grafana's `refresh` setting is ignored | 🟡 | 📋 |
| **Error and warning surfacing** | Last good data was dropped on error, only one error per panel survived, Prometheus `warnings` were discarded, and 4xx errors were retried; panels now keep data marked stale, show every failed query, and surface warnings | 🟡 | ✅ |
| **Unit and display fixes** | `bytes` scales by 1000 instead of 1024, `bps` displays as bytes/s, `legendFormat: __auto` renders literally, and stat/bar gauge distort negative or fractional values | 🟢 | 📋 |
| **Classic import diagnostics** | Classic non-Prometheus targets are sent to Prometheus, and `transformations`, `timeFrom`/`timeShift`, and overrides are dropped without a warning | 🟢 | 📋 |
| **Config validation** | A missing `--config` file and unknown keys are silently ignored; refresh, step, and range are unbounded | 🟢 | 📋 |
| **Key binding fixes and help overlay** | `[`/`]` only pan when Shift is reported, live mode can't be restored outside fullscreen, and `?` shows debug info rather than help | 🟢 | 📋 |
| **Logging** | No log file or verbosity flag, so field failures can't be diagnosed | 🟢 | 📋 |

### Secured Prometheus-Compatible Backends

| Item | Why it blocks production | Complexity | Status |
|---|---|---|---|
| **Optional authentication** | Bearer tokens and Basic auth from files or env vars, never literal CLI args; credentials are redacted from the debug bar and logs | 🟡 | 📋 |
| **Custom headers** | Multi-tenant Mimir and Cortex need `X-Scope-OrgID` | 🟢 | 📋 |
| **Optional TLS settings** | Custom CA bundles, the OS trust store, client certificates (mTLS), and an explicit insecure mode for lab use | 🟡 | 📋 |
| **Configurable timeouts** | The 10s request and 5s connect timeouts are hard-coded | 🟢 | 📋 |
| **Named datasource profiles** | Switch between environments and map dashboard datasource uids to endpoints | 🟡 | 📋 |
| **End-to-end tests against a mock backend** | Cover unsecured, auth, tenant-header, and TLS paths without a live server | 🟡 | 📋 |

### Release Engineering

| Item | Why it blocks production | Complexity | Status |
|---|---|---|---|
| **CI quality gates** | CI only builds and tests on Linux; add fmt, clippy, MSRV, macOS/Windows tests, and cargo-deny | 🟢 | 📋 |
| **Independent release channel** | Crate, installer, docs site, and release workflow still point at the upstream project | 🟡 | 📋 |
| **Terminal capability fallbacks** | Honor `NO_COLOR` and fall back from truecolor themes on 256-color terminals | 🟢 | 📋 |

---

## Compatibility Ladder

This is the main backlog, ordered by Grafana parity domain.

### 1. Import Diagnostics & Trust

| Feature | Grafana field / behavior | User value | Complexity | Status |
|---|---|---|---|---|
| **Compatibility matrix automation** | Documentation generated or checked against current code | Keeps users and contributors aligned with reality | 🟢 | 📋 |
| **Unsupported panel warnings** | Unsupported `panels[].type` and ignored high-impact fields | Makes import degradation visible instead of silent | 🟢 | ✅ |
| **Import validation** | `--validate`, `--strict`, and JSON import checks | Lets users check dashboards before launching the TUI | 🟢 | 🔶 |
| **Better JSON/import errors** | Parse errors with path/context where possible | Faster debugging for broken exports | 🟢 | 📋 |
| **Variable substitution diagnostics** | Missing variables, unsupported format modifiers | Explains empty panels caused by unresolved variables | 🟢 | ✅ |

### 2. Field Config Parity

| Feature | Grafana field / behavior | User value | Complexity | Status |
|---|---|---|---|---|
| **Broader unit formatting** | `fieldConfig.defaults.unit` | Extends the shipped common-unit subset to more Grafana units | 🟡 | 📋 |
| **Additional no-value coverage** | `fieldConfig.defaults.noValue` | Extends the shipped null-value fallback beyond current Stat/Table/export paths where relevant | 🟢 | 📋 |
| **Value mappings** | `fieldConfig.defaults.mappings` | Status codes and enum-like values become readable labels | 🟡 | 🔜 |
| **Display names** | `fieldConfig.defaults.displayName` | Series and table labels match Grafana naming | 🟢 | 📋 |
| **Color mode subset** | `fieldConfig.defaults.color` | Honors configured color intent where terminal rendering permits | 🟡 | 📋 |
| **Field overrides subset** | `fieldConfig.overrides` | Per-series units/names/colors for common matcher types | 🔴 | 💡 |

### 3. Panel Options Parity

| Feature | Grafana field / behavior | User value | Complexity | Status |
|---|---|---|---|---|
| **Reduce options** | `options.reduceOptions.calcs` | Stat/Gauge/BarGauge can use last, min, max, mean, total | 🟡 | 🔜 |
| **Legend display mode** | `options.legend.displayMode` | Hide/list/table modes map to compact TUI equivalents | 🟡 | 📋 |
| **Legend placement** | `options.legend.placement` | Bottom/right placement influences terminal layout where useful | 🟡 | 📋 |
| **Legend calculations** | `options.legend.calcs` | Min/max/avg/current values appear beside series names | 🟡 | 📋 |
| **Text/graph/color modes** | Stat and gauge display options | Imported summary panels better match Grafana intent | 🟡 | 📋 |
| **Tooltip behavior mapping** | `options.tooltip` | Documented mapping to inspect mode, with useful defaults | 🟢 | 📋 |

### 4. Target & Query Parity

| Feature | Grafana field / behavior | User value | Complexity | Status |
|---|---|---|---|---|
| **Hidden targets** | `targets[].hide` | Helper queries do not clutter imported panels | 🟢 | ✅ |
| **Instant query defaults** | Panel-specific fallback behavior when `targets[].instant` is omitted | Keeps summary panels fast while preserving range queries for charts | 🟢 | ✅ |
| **Target interval** | `targets[].interval` / `intervalFactor` | Panel-specific resolution is respected; `interval` is supported, `intervalFactor` is not | 🟡 | 🔶 |
| **Target ref IDs** | `targets[].refId` | Better diagnostics and future transformation support | 🟢 | 📋 |
| **Format handling** | `targets[].format` | Tables and heatmaps can choose more appropriate handling | 🟡 | 📋 |
| **Exemplar awareness** | `targets[].exemplar` | Document ignored behavior or expose limited metadata later | 🔴 | 💡 |

### 5. Template Variable Parity

| Feature | Grafana field / behavior | User value | Complexity | Status |
|---|---|---|---|---|
| **Multi-value variables** | `templating.list[].multi` | Imported dashboards can query multiple instances/jobs | 🟡 | ✅ |
| **Include-all variables** | `templating.list[].includeAll` | Grafana "All" semantics work more predictably | 🟡 | ✅ |
| **Variable option sorting** | `templating.list[].sort` | Deterministic variable values from Prometheus | 🟢 | 📋 |
| **Variable picker UI** | `templating.list[].options` | Users can switch variable values without restarting | 🟡 | 📋 |
| **Format modifiers** | `${var:regex}`, `${var:pipe}`, `${var:csv}` | Common Grafana PromQL templates import correctly | 🟡 | 📋 |
| **Datasource-aware variables** | `templating.list[].datasource` | Foundation for multi-source dashboards | 🔴 | 💡 |

### 6. Graph & Timeseries Parity

| Feature | Grafana field / behavior | User value | Complexity | Status |
|---|---|---|---|---|
| **Draw styles** | `fieldConfig.defaults.custom.drawStyle` | Line, bars, and points map to distinct terminal renderings | 🟡 | ✅ |
| **Stacking** | `fieldConfig.defaults.custom.stacking` | Stacked area/bar intent is visible in dense dashboards; parsed but not yet rendered | 🟡 | 🔶 |
| **Axis labels** | `fieldConfig.defaults.custom.axisLabel` | Imported axis meaning is visible where space allows | 🟢 | 📋 |
| **Axis placement** | `fieldConfig.defaults.custom.axisPlacement` | Left/right/hidden axis settings map to TUI behavior; `hidden` is honored | 🟡 | 🔶 |
| **Scale distribution** | `fieldConfig.defaults.custom.scaleDistribution` | Linear/log choices are respected or explicitly warned | 🟡 | 💡 |

### 7. Panel Type Parity

| Feature | Grafana panel type | User value | Complexity | Status |
|---|---|---|---|---|
| **Row headers** | `row` | Dashboard sections stay recognizable | 🟢 | ✅ |
| **Collapsed rows** | `row.collapsed` | Large dashboards can start folded | 🟡 | ✅ |
| **Text panel** | `text` | Notes/runbook snippets survive import | 🟢 | 📋 |
| **Histogram panel** | `histogram` | Histogram dashboards import with fewer skips | 🟡 | 💡 |
| **Pie chart fallback** | `piechart` | Small category summaries can render as bars/table | 🟡 | 💡 |
| **Logs panel path** | `logs` | Opens the route toward Loki/log exploration | 🔴 | 💡 |
| **State timeline/status history** | `state-timeline`, `status-history` | Better status dashboards | 🔴 | 💡 |

### 8. Datasource & Import Parity

| Feature | Grafana field / behavior | User value | Complexity | Status |
|---|---|---|---|---|
| **Grafana API dashboard loading** | Load dashboards by UID from Grafana | Avoids manual JSON export flow | 🟡 | 📋 |
| **Multiple Prometheus sources** | `datasource` references and source switching | Supports cluster/environment dashboards | 🟡 | 📋 |
| **Mixed datasource warnings** | Mixed panels and unsupported datasources | Makes unsupported imports clear | 🟢 | 📋 |
| **Loki support** | Logs datasource and logs panel | Useful terminal observability companion | 🔴 | 💡 |
| **InfluxDB support** | Influx datasource | Broadens dashboard compatibility | 🔴 | 💡 |

### 9. Grafana Dashboard Schema v2

Grafatui accepts JSON and YAML resources with the exact
`dashboard.grafana.app/v2` API version. V2 semantics with a terminal equivalent
are implemented; browser-only settings, such as viewport-filling rows, are
accepted and ignored. Semantics that are not implemented yet fail with a clear
import error rather than silently changing the dashboard.

| Feature | Grafana field / behavior | User value | Complexity | Status |
|---|---|---|---|---|
| **V2 Resource JSON compatibility** | `apiVersion: dashboard.grafana.app/v2` with a `GridLayout` | Imports Grafana 13's default JSON format without requiring a Classic export | 🔴 | ✅ |
| **Rows layout** | `RowsLayout` and nested row layouts | Preserves dashboard grouping and collapsed sections | 🔴 | ✅ |
| **Tabs layout** | `TabsLayout` and nested tabs | Preserves tabbed dashboard organization | 🔴 | ✅ |
| **Auto-grid layout** | `AutoGridLayout` | Preserves automatic panel placement and sizing | 🔴 | ✅ |
| **Repeat and dynamic layouts** | Layout and element `repeat` settings | Expands panels or groups from variable values | 🔴 | ✅ |
| **Conditional rendering** | `conditionalRendering` on supported containers | Shows or hides content using v2 conditions | 🔴 | ✅ |
| **Nested layout variables** | Variables scoped to rows and tabs | Preserves local variable scope in dynamic dashboards | 🔴 | ✅ |
| **Library panel resolution** | `LibraryPanel` element references | Imports reusable panels by resolving their external definitions | 🔴 | 📋 |
| **V2 Resource YAML** | YAML representation of the v2 resource | Supports Grafana's alternative as-code export format | 🟡 | ✅ |

---

## Milestone Slices

The compatibility ladder defines the backlog. Milestones turn it into shippable
increments.

### v0.2 - Grafana Import Fidelity

Goal: imported dashboards should be more readable and less silently degraded.

| Item | Why it belongs here | Complexity | Status |
|---|---|---|---|
| Value mappings | Makes status/stat panels useful instead of numeric-only | 🟡 | 🔜 |
| Broader unit formatting | Expands the shipped common-unit subset to more Grafana dashboards | 🟡 | 📋 |
| Additional no-value coverage | Completes the shipped fallback behavior where terminal rendering can use it | 🟢 | 📋 |
| Hidden targets | Prevents helper queries from appearing as normal series | 🟢 | ✅ |
| Unsupported panel warnings | Builds trust in imported results | 🟢 | ✅ |
| `--validate` import diagnostics | Gives users text, JSON, and strict non-interactive dashboard checks | 🟢 | 🔶 |

### v0.3 - Panel Semantics

Goal: stat, gauge, table, and legend behavior should match common Grafana
expectations.

| Item | Why it belongs here | Complexity | Status |
|---|---|---|---|
| Reduce options | Summary panels need more than "last" | 🟡 | 📋 |
| Display names | Imported labels become clearer without changing queries | 🟢 | 📋 |
| Legend display modes and placement | Dense dashboards need predictable legend behavior | 🟡 | 📋 |
| Legend calculations | Adds useful table-like summaries without a new panel type | 🟡 | 📋 |
| Target interval support | Respects panel-specific query resolution | 🟡 | ✅ |

### v0.4 - Graph & Timeseries Fidelity

Goal: graph and timeseries panels should preserve more visual intent within TUI
constraints.

| Item | Why it belongs here | Complexity | Status |
|---|---|---|---|
| Draw styles | Bars/points/lines should be distinguishable | 🟡 | ✅ |
| Stacking | Common Grafana area/bar semantics | 🟡 | 🔶 |
| Axis labels and placement | Preserves context for imported charts | 🟡 | 🔶 |
| Scale distribution handling | Honor or warn on log/non-linear scales | 🟡 | 💡 |
| Row headers and collapsed rows | Keeps large dashboard structure intact | 🟡 | ✅ |

### v0.5 - Exploration Workflow

Goal: after import fidelity improves, make Grafatui a stronger daily terminal
tool for investigating Prometheus data.

| Item | User value | Complexity | Status |
|---|---|---|---|
| Dashboard/panel quick switcher | Jump around large dashboard sets quickly | 🟢 | 📋 |
| Panel history | Return to recently inspected panels | 🟢 | 📋 |
| Ad-hoc PromQL query mode | Scratch queries without editing JSON | 🟡 | 📋 |
| Label explorer | Discover metrics and labels from the terminal | 🟡 | 📋 |
| File watch mode | Auto-reload dashboards/config while iterating | 🟢 | 📋 |
| Session restore | Resume last dashboard/time range | 🟢 | 📋 |
| Bookmarks | Save dashboard + time range combinations | 🟢 | 📋 |

### Later - Live Sources, Sharing, and Operations

These are valuable, but they should not outrank core Grafana import fidelity.

| Item | User value | Complexity | Status |
|---|---|---|---|
| Grafana API dashboard browser | Pull dashboards live from Grafana | 🟡 | 📋 |
| Multiple Prometheus sources | Switch clusters/environments | 🟡 | 📋 |
| CSV export | Export panel data for reports or debugging | 🟢 | 📋 |
| Copy to clipboard | Copy panel data or values quickly | 🟢 | 📋 |
| Share snapshot | Portable rendered dashboard snapshot | 🟡 | 💡 |
| Split view | Compare two panels side-by-side | 🟡 | 💡 |
| PromQL autocomplete | Suggest metrics, labels, and functions | 🔴 | 💡 |
| Alert rule viewer | Inspect Prometheus/Alertmanager state | 🟡 | 📋 |
| Alert silence creation | Operational action from the terminal | 🟡 | 💡 |
| Desktop notifications | Notify when thresholds are crossed | 🟡 | 💡 |
| Dashboard editor mode | Edit queries inline | 🟡 | 💡 |
| Dashboard creation wizard | Build dashboards from the TUI | 🔴 | 💡 |
| SSH tunnel mode | Connect to remote Prometheus more easily | 🟡 | 💡 |
| Colorblind palettes | Improve accessibility | 🟢 | 📋 |
| Custom keybindings | User-remappable shortcuts | 🟡 | 📋 |

---

## Best Next Steps

Recommended order for the next focused development cycle:

1. **Fix the data path**
   - Variable refresh policy.
   - Unit, display, key binding, config, and Classic import diagnostic fixes.

2. **Reach secured backends**
   - Opt-in auth, custom headers, TLS settings, and timeouts, with the
     unsecured default unchanged.
   - End-to-end tests against a mock backend for every mode.

3. **Harden releases**
   - CI quality gates and an independent release channel.

4. **Resume Grafana import fidelity**
   - Dashboard time and timezone, a runtime variable picker and format
     modifiers, reduce options, value mappings, display names, and legend
     options.
   - Prefer common Grafana dashboard correctness over new non-parity features.

---

## How to Contribute

If you'd like to help, the best starting points are items marked 🔜 and 🟢.
Roadmap items are especially useful when PRs include:

- A small Grafana JSON fixture that demonstrates the supported field.
- Unit tests for parsing and display behavior.
- A short note in `docs/grafana-compatibility.md` explaining the TUI mapping or
  limitation.

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines, and
[docs/grafana-compatibility.md](docs/grafana-compatibility.md) for the full Grafana JSON
feature-parity breakdown.
