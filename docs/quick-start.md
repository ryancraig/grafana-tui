# Quick Start

## Connect to Prometheus

If Prometheus is already running locally:

```bash
grafatui --prometheus-url http://localhost:9090
```

Point Grafatui at another Prometheus server with the same option:

```bash
grafatui --prometheus-url http://prometheus.example.com:9090
```

## Import a Grafana Dashboard

Grafatui imports either a Classic JSON dashboard or an exact
`dashboard.grafana.app/v2` JSON resource that uses recursive grid, row, or tab
containers:

```bash
grafatui --prometheus-url http://localhost:9090 --grafana-json ./dashboard.json
```

Grafana 13 exports V2 resources as JSON or YAML, and both import directly.
Exports reference library panels by uid only, so enable **Share dashboard with
another instance** when exporting a dashboard that uses them. V1 Resource files
are unsupported.
See [Grafana Dashboard Import](grafana-dashboard-import.md) for the full format
requirements.

Override dashboard variables with repeated `--var` options; repeating a name
selects several values:

```bash
grafatui --grafana-json ./dash.json --var job=node --var instance=server-01
```

## Run the Demo

The repository includes a Prometheus demo stack and sample dashboards:

```bash
git clone https://github.com/fedexist/grafatui.git
cd grafatui
cd examples/demo && docker-compose up -d && sleep 5 && cd ../..
cargo run -- --grafana-json examples/dashboards/prometheus_demo.json --prometheus-url http://localhost:19090
```

When finished:

```bash
cd examples/demo
docker-compose down -v
```

## Useful First Keys

| Key | Action |
|---|---|
| `q` | Quit |
| `r` | Force refresh |
| `+` / `-` | Zoom out / in |
| `Shift+Left` / `Shift+Right` | Pan left / right |
| `f` | Fullscreen selected panel |
| `Enter` / `Space` on a row | Toggle the row |
| `Left` / `Right` on a row | Collapse / expand the row |
| `v` | Inspect values |
| `/` | Search visible rows and panels |
