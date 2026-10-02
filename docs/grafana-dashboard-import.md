# Grafana Dashboard Import

Grafatui imports Grafana dashboards exported as JSON or YAML and renders their
supported panels in the terminal.

| Format | Status | Requirements |
|---|---|---|
| Classic JSON | ✅ Supported | Non-resource object with fields such as `title`, `panels`, and `templating` |
| V2 Resource JSON | ✅ Supported | Exact `apiVersion: dashboard.grafana.app/v2`, Grafana 13's default export format |
| V2 Resource YAML | ✅ Supported | The same resource, read from a `.yaml` or `.yml` file |
| V2 alpha/beta resources | ❌ Unsupported | `dashboard.grafana.app/v2alpha1` and `v2beta1`, exported by Grafana 12, are not accepted |
| V1 Resource JSON | ❌ Unsupported | The `dashboard.grafana.app/v1` resource envelope is not accepted |

`--grafana-json` (alias `--grafana-dashboard`) reads `.json` files as JSON and
`.yaml`/`.yml` files as YAML. Files with any other extension are parsed as JSON
first and then as YAML.

V2 resources and Classic dashboards share one importer, so panels, Prometheus
queries, variables, field configuration, and diagnostics behave the same in
both. On top of that, V2 dashboards support:

- `GridLayout`, `AutoGridLayout`, `RowsLayout`, and `TabsLayout`, nested in any
  combination ([auto grids](#auto-grid-layouts));
- repeated grid items, auto grid items, rows, and tabs ([repeats](#repeats));
- conditional rendering of rows, tabs, and auto grid items
  ([conditional rendering](#conditional-rendering));
- variables defined by rows and tabs ([row and tab variables](#row-and-tab-variables));
- `timeSettings.autoRefresh` as the default refresh interval.

Settings that only make sense in a browser, such as rows and auto grids that fill
the viewport, are accepted and ignored. Layout kinds Grafatui does not know, and
malformed fields, are import errors that name the field's path, such as
`spec.layout.spec.rows[0].spec.repeat.direction`.

Grafana's resource API writes empty lists and objects as `null` (for example
`links`, `transformations`, `options`, and `variables`), and its exporter may
omit them entirely. Grafatui treats both the same as an empty value, so
dashboards exported from Grafana 13 import unchanged. Exports made with
**Share dashboard with another instance** enabled also work: their queries carry
no datasource and run against the Prometheus server given by `--prometheus-url`.

Library panels are exported as a reference to the library panel's uid, without
the panel itself, so Grafatui skips them with an import diagnostic. Enable
**Share dashboard with another instance** when exporting to inline library
panels into the dashboard.

## Auto Grid Layouts

An `AutoGridLayout` places its panels left to right, top to bottom, in columns of
equal width. As in Grafana, the column count adapts to the available width: it
is the number of minimum-width columns that fit, capped by `maxColumnCount`
(default 3), and never more than the number of panels, so a grid with fewer
panels than columns stretches them across the full width. Resizing the terminal
reflows the grid.

Grafana sizes auto grids in CSS pixels. Grafatui converts them as follows:

| Setting | Grafana size | Grafatui size |
|---|---|---|
| `columnWidthMode: narrow` | 192px minimum column width | 24 terminal columns |
| `columnWidthMode: standard` (default) | 448px | 56 terminal columns |
| `columnWidthMode: wide` | 768px | 96 terminal columns |
| `columnWidthMode: custom`, `columnWidth: N` | N px | N / 8 terminal columns |
| `rowHeightMode: short` | 168px row height | 5 grid rows |
| `rowHeightMode: standard` (default) | 320px | 9 grid rows |
| `rowHeightMode: tall` | 512px | 14 grid rows |
| `rowHeightMode: custom`, `rowHeight: N` | N px | (N + 8) / 38 grid rows, rounded |

Terminal columns use Grafana's 8px spacing unit, so column counts match what
Grafana shows at a similar pixel width: a 120-column terminal shows two
`standard` columns, and an 80-column terminal shows one. Grid rows are the same
unit as a fixed-grid panel's `height` (30px plus an 8px margin in Grafana).
`fillScreen`, `fitContent`, and minimum/maximum height settings are ignored.

## Row and Tab Variables

A V2 row or tab can define `variables` of its own. As in Grafana, they apply to
the row or tab itself (its title, repeat, and conditions) and to everything
inside it, and they shadow a dashboard variable with the same name there; the
rest of the dashboard keeps the dashboard's value. Every variable kind Grafatui
supports at the dashboard level is supported here, with the same selection,
multi-value, and `All` rules.

Query variables in a row or tab resolve against Prometheus with the variables
around that row or tab. Inside a repeated row, each copy resolves its own, so a
`label_values(up{dc="$dc"}, host)` variable in a row repeated over `dc` lists
each data center's hosts.

`--var` and the config file's `vars` override dashboard variables only; a row or
tab variable with the same name still wins inside its row or tab.

## Conditional Rendering

V2 rows, tabs, and auto grid items can carry a `conditionalRendering` group that
shows or hides them. Grafatui evaluates these groups as Grafana 13 does:

| Condition | Holds when |
|---|---|
| Variable `equals` / `notEquals` | Any selected value equals the value (or not). `All` also matches while `All` is selected |
| Variable `matches` / `notMatches` | Any selected value matches the regular expression (or not); an invalid expression shows the item |
| Data (auto grid items only) | The panel returned data (`value: true`) or no data (`value: false`) |
| Time range size | The dashboard time range is at most the value, such as `1h` or `7d` |

A group's `condition` combines its conditions with `and` or `or`, and its
`visibility` shows (`show`) or hides (`hide`) the item when they hold.
Conditions that cannot be decided yet are left out: a variable the dashboard
does not define, an unparseable time range size, or a data condition before its
panel's first query. A group with nothing left to decide shows its item.

Items re-evaluate as query variables resolve, data refreshes, and the time range
changes with zoom and pan. Hidden rows, tabs, and auto grid items leave the
layout entirely; the auto grid reflows, a tab group whose active tab is hidden
switches to its first remaining tab, and collapsed rows and selected tabs are
remembered while hidden. Panels hidden by a data condition keep being queried
so they can reappear. Inside a repeated row or tab, a variable condition sees
that copy's own value.

## Export From Grafana

Grafana 13 exports dashboards in the V2 Resource model by default:

1. Open the dashboard in Grafana.
2. In the toolbar, open **Export** and select **Export as code**.
3. Choose JSON or YAML.
4. If the dashboard uses library panels, enable **Share dashboard with another
   instance** so they are inlined.
5. Download the file, or copy it into a local `.json` or `.yaml` file.
6. Run Grafatui with `--grafana-json`.

```bash
grafatui --prometheus-url http://localhost:9090 --grafana-json ./node-exporter.yaml
```

Grafana 12 exports dynamic dashboards as `v2alpha1` or `v2beta1` resources,
which Grafatui does not accept. Export those, or any dashboard from an older
Grafana, with the Classic model instead: under **Export as code**, expand
**Advanced options**, set **Model** to **Classic**, and save the JSON. Grafana
documents the available models and export controls in
[Export a dashboard as code](https://grafana.com/docs/grafana/latest/visualizations/dashboards/share-dashboards-panels/#export-a-dashboard-as-code).

## Supported Panel Types

Grafatui currently supports:

- `graph`
- `timeseries`
- `stat`
- `gauge`
- `bargauge`
- `table`
- `heatmap`

Classic row headers and their collapsed state are rendered and interactive.
Collapsed descendants are excluded from navigation, search, refresh, and
export. Hidden-header rows are transparent: they consume no header and their
children remain visible.

## Variables

Grafatui reads dashboard variables from `templating.list` (Classic) or
`spec.variables` (V2) and expands `$var` and `${var}` in PromQL expressions and
in panel, row, and tab titles. Names are matched whole, so `$job_name` never
expands a variable called `job`.

Defaults come from the dashboard's saved selection; a variable without one
selects its first option, as Grafana does. Override them from the CLI:

```bash
grafatui --grafana-json ./dash.json --var job=node --var instance=server-01
```

Prometheus query variables such as `label_values(up, instance)` and
`query_result(...)` are resolved before panel queries run. A saved selection
that Prometheus still offers is kept; otherwise the first value is selected.

### Multi-Value and All Selections

Multi-value and include-all variables follow Grafana's Prometheus datasource:

- Each value is regex-escaped, so `web.1` becomes `web\\.1` in the query.
- Several values become an alternation such as `(api|web\\.1)`, meant for
  `=~` matchers.
- `All` selects every option: a custom variable's options, or every value a
  query variable resolves. It interpolates as the variable's `allValue` when
  set, and otherwise as all values joined.

In titles, several values are shown joined with ` + `, like Grafana's text
format.

Repeat `--var` for one name to select several values. A single `--var` value is
used verbatim, so it can still be a regex such as `--var job='api|web'`:

```bash
grafatui --grafana-json ./dash.json --var instance=server-01 --var instance=server-02
```

## Repeats

Panels, rows, and tabs that repeat over a variable are copied once per selected
value, so `All` or a multi-value selection shows one copy per value. Each copy
uses its own value: `$var` in its title and queries is that single value
(regex-escaped for multi-value variables), and the copies of a repeated row or
tab carry that value into every panel inside them. Nested repeats combine.

| Repeat | Classic | V2 | Layout |
|---|---|---|---|
| Panel, horizontal | `repeat`, `repeatDirection: h` | `GridLayoutItem` `repeat.direction: h` | Spans the full grid width, in rows of up to `maxPerRow` equal columns (default 4) |
| Panel, vertical | `repeatDirection: v` | `repeat.direction: v` | Copies stack at the panel's width |
| Auto grid item | — | `AutoGridLayoutItem` `repeat` | Copies flow into the auto grid after the item |
| Row | row `repeat` | `RowsLayoutRow` `repeat` | One row per value |
| Tab | — | `TabsLayoutTab` `repeat` | One tab per value |

When a repeated grid panel grows, panels that start below it move down by the
height it adds, as in Grafana. If the variable has no selected values, the item
is shown once; if the variable is not defined, the item is also shown once and
an `unknown_repeat_variable` diagnostic is printed.

Copies follow query variables as they resolve. A copy keeps its place, data,
and collapsed state while its value stays selected.

## Import Diagnostics

Grafatui prints import warnings before starting the TUI when a dashboard uses
important Grafana features that are skipped or ignored. Diagnostics include
unsupported panel types, value mappings, reduce options, unresolved variables,
unsupported V2 datasources, and unsupported variable modifiers such as
`${var:regex}`. V2 diagnostics retain their `spec.*` source paths.

Run a non-interactive check with:

```bash
grafatui --validate --grafana-json ./dash.json
```

Warnings do not make validation fail. A dashboard that can be parsed and
imported exits successfully even if diagnostics are printed.

Use `--strict` to make warnings fail validation, or `--format json` to emit a
machine-readable summary. Fatal V2 layout errors fail validation in all modes;
`--strict` additionally fails when import diagnostics are present:

```bash
grafatui --validate --strict --grafana-json ./dash.json
grafatui --validate --format json --grafana-json ./dash.json
```

## Hidden Targets

Grafatui skips hidden queries during import: `targets[].hide` in Classic JSON
and `PanelQuery` `hidden` in V2 resources. Panels with a mix of hidden and
visible queries render only the visible ones.

## Query Modes

Grafatui honors a query's `instant` setting: `targets[].instant` in Classic JSON,
and `instant` in the Prometheus query `spec` of a V2 `PanelQuery`. Instant
queries use the Prometheus instant `query` endpoint, while range queries use
`query_range`.

If a target does not specify `instant`, Gauge, Bar Gauge, and Table panels
default to instant queries. Graph, Timeseries, Stat, and Heatmap panels default
to range queries.

## Field Configuration

Grafatui applies selected `fieldConfig.defaults` values where they map cleanly
to terminal rendering:

- `min` and `max` set explicit Graph y-axis bounds and Gauge limits.
- `thresholds` render graph threshold lines and drive dynamic coloring for Stat,
  Gauge, and Bar Gauge panels.
- `unit`, `decimals`, and `noValue` affect supported panel values, axes,
  legends, and exports.
- `custom.axisGridShow` controls per-panel graph guide lines.

## Built-In PromQL Variables

Grafatui expands the following Grafana-style variables:

- `$__interval`
- `$__interval_ms`
- `$__range`
- `$__range_s`
- `$__range_ms`
- `$__rate_interval`
- `$__rate_interval_ms`

`$__interval` is the query's step, which scales with the time range and honors
panel `maxDataPoints` and `interval` and target `interval`. See
[query resolution](configuration.md#query-resolution).

## Compatibility Details

See the [Grafana compatibility matrix](grafana-compatibility.md) for field-by-field support details.
