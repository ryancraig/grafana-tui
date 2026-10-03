# Grafatui Examples

This directory contains example Grafana dashboards and a demo environment for testing grafatui.

## Quick Demo

Want to try grafatui instantly? Use the pre-configured demo environment:

```bash
cd demo
docker-compose up -d && sleep 5 && cd ../.. && cargo run -- --grafana-json examples/dashboards/prometheus_demo.json --prometheus-url http://localhost:19090
```

See [`demo/README.md`](demo/README.md) for details.

## HashiStack RDW Dashboards

[`demo/hashistack-rdw/`](demo/hashistack-rdw/README.md) holds eight operator
dashboards for a Consul/Nomad/Vault platform with a Consul Connect (Envoy) mesh
and remote developer workspaces. They are Grafana 13 V2 resources and run
against a real datacenter's mTLS Prometheus (client certificate required), not
the bundled demo stack. The README covers issuing the reader certificate and
building the config.

## External Annotations

[`annotations.jsonl`](annotations.jsonl) is a read-only external JSONL event
source for graph and timeseries panels. Its `Deployed v2.4` event uses
`panel_titles` to target the bundled `HTTP Request Rate by Status Code` panel;
the remaining events omit `panel_titles` and therefore remain dashboard-wide.
Run it with a dashboard:

```bash
cargo run -- --grafana-json examples/dashboards/prometheus_demo.json --annotations-file examples/annotations.jsonl
```

See the [external annotations guide](../docs/annotations.md) for targeting,
tag filtering, inspection, reload behavior, and limitations.

### Git Command Provider

`git_annotation_provider.rs` turns commits in Grafatui's requested time window
into external JSONL annotations. Build it from the repository root:

```bash
cargo build --example git_annotation_provider
```

Then configure the compiled example as the one annotation source:

```toml
[annotations_command]
program = "./target/debug/examples/git_annotation_provider"
args = ["."]
timeout = "10s"
```

The optional first argument is the Git repository path and defaults to `.`.

## Dashboards

Every dashboard here is a Grafana 13 V2 resource. `prometheus_demo.json`,
`all_visualizations.json`, `instant_queries.json`, `thresholds_demo.json`,
`simple_test.json`, and the vLLM demo dashboards were converted from Classic
JSON by Grafana 13.2.3; the Classic originals live in
`tests/fixtures/grafana/classic_examples`, and a test checks that both formats
import identically.


### `prometheus_demo.json`
**Recommended for first-time users!** A comprehensive dashboard designed for the included demo environment.
Shows all 6 visualization types with real metrics from Prometheus monitoring itself:
- Graph, Gauge, Stat, Bar Gauge, Table, Heatmap
- Works immediately with `demo/docker-compose.yml`

### `all_visualizations.json`
Demonstrates all supported panel types in a single dashboard:
- **Graph**: Line chart showing CPU usage over time
- **Timeseries styles**: Bars with hidden axis, area fill, and points-only rendering
- **Gauge**: Progress bar for memory usage
- **Stat**: Big value display with sparkline for uptime
- **Bar Gauge**: Vertical bars comparing request rates
- **Table**: Tabular view of series
- **Heatmap**: Color-coded time-series intensity

### `instant_queries.json`
Demonstrates queries marked `instant` and the default instant query behavior
used by Gauge, Bar Gauge, and Table panels.

### `thresholds_demo.json`
Demonstrates threshold rendering, threshold marker styles, and explicit field
min/max bounds.

### `grafana_v2_compatibility.json`
An exact Grafana V2 resource with a `GridLayout`, a dynamic Prometheus `job`
variable, refresh settings, and two supported panels. Run it against the bundled
demo stack:

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_compatibility.json --prometheus-url http://localhost:19090
```

### `grafana_v2_rows.json`
An exact Grafana V2 `RowsLayout` resource demonstrating expanded, nested, and
initially collapsed rows with three Prometheus panels. It also includes a
hidden-header container, whose child panel flows directly beneath its parent
row. Run it against the bundled demo stack:

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_rows.json --prometheus-url http://localhost:19090
```

### `grafana_v2_autogrid.json`
An exact Grafana V2 `AutoGridLayout` resource with five Prometheus panels in up
to three columns. Columns reflow as the terminal is resized: two at 120 columns,
one at 80.

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_autogrid.json --prometheus-url http://localhost:19090
```

### `grafana_v2_repeats.yaml`
A Grafana V2 resource in YAML, as exported by Grafana 13. A request-rate panel
repeats horizontally, two per row, over an `All` selection of HTTP handlers,
pushing the panel below it down; a row repeats over every `job` that the
`label_values(up, job)` query variable resolves.

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_repeats.yaml --prometheus-url http://localhost:19090
```

### `grafana_v2_conditional.json`
A Grafana V2 resource authored in Grafana 13 with conditional rendering. The
`Production` row shows because `env` is `prod`, while the `Staging only` row is
hidden; the server-error panel appears only when Prometheus returns 5xx
requests; and the `Zoomed in` tab is shown only while the time range is an hour
or less.

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_conditional.json --prometheus-url http://localhost:19090 --range 1h
```

### `grafana_v2_sections.json`
A Grafana V2 resource authored in Grafana 13 with row variables. The
`Tail latency` row defines its own `quantile` (0.99), shadowing the dashboard's
0.9 inside that row only, and the `Busiest handlers` row defines a `handler`
query variable whose `/api/v1/` values a repeated panel iterates.

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_sections.json --prometheus-url http://localhost:19090
```

### `grafana_v2_tabs.json`
An exact Grafana V2 `TabsLayout` resource with panels, a nested row, and an
empty tab. Focus the tab bar and use Left/Right to switch tabs; Enter/Space
moves into the active content.

```bash
cargo run -- --grafana-json examples/dashboards/grafana_v2_tabs.json --prometheus-url http://localhost:19090
```

### Usage

```bash
# Test with local Prometheus (default port 9090)
cargo run -- --grafana-json examples/dashboards/all_visualizations.json

# Or with custom Prometheus URL
cargo run -- --grafana-json examples/dashboards/all_visualizations.json --prometheus-url http://prometheus.example.com:9090

# Override variables
cargo run -- --grafana-json examples/dashboards/all_visualizations.json --var instance=prometheus:9090
```

## Creating Your Own

You can export any Grafana dashboard as JSON and use it with grafatui:
1. In Grafana, go to Dashboard Settings → JSON Model
2. Copy the JSON
3. Save it to a file
4. Run: `grafatui --grafana-json your-dashboard.json`

## Supported Panel Types

- ✅ `graph` / `timeseries` - Line, point, bar, and area-style charts
- ✅ `gauge` - Progress bars
- ✅ `bargauge` - Bar charts
- ✅ `table` - Data tables
- ✅ `stat` - Single value + sparkline
- ✅ `heatmap` - Color intensity grid
