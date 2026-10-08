# grafana-tui

[![CI](https://github.com/ryancraig/grafana-tui/workflows/CI/badge.svg)](https://github.com/ryancraig/grafana-tui/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust Version](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)

**grafana-tui** is a terminal user interface for Prometheus, inspired by Grafana. It lets you inspect time-series dashboards from a fast, keyboard-driven TUI that works well over SSH and in minimal environments.

[![asciicast](https://asciinema.org/a/vMRNEjG0FEDKGP31.svg)](https://asciinema.org/a/vMRNEjG0FEDKGP31)

## Quick Start

Install the latest prebuilt binary into `~/.local/bin`:

```bash
bash -o pipefail -c 'curl --proto =https --tlsv1.2 -LsSf https://raw.githubusercontent.com/ryancraig/grafana-tui/main/install.sh | bash'
```

See [Installation](https://ryancraig.github.io/grafana-tui/installation.html) for other directories, pinned versions, manual downloads, and building from source.

Run against a Prometheus instance:

```bash
grafana-tui --prometheus-url http://localhost:9090
```

Or try the included demo:

```bash
git clone https://github.com/ryancraig/grafana-tui.git
cd grafana-tui
cd examples/demo && docker-compose up -d && sleep 5 && cd ../..
cargo run -- --grafana-json examples/dashboards/prometheus_demo.json --prometheus-url http://localhost:19090
```

## Features

- Prometheus range and instant queries with async fetching.
- Grafana Classic JSON and V2 Resource JSON/YAML import for graph, timeseries, stat, gauge, bar gauge, table, and heatmap panels, including interactive nested rows and tabs.
- Template variables, Grafana built-in PromQL variables, legend formatting, thresholds, and grid layout support.
- Grafana timeseries draw styles for lines, points, bars, area fill, hidden axes, and per-panel grid visibility.
- Keyboard-first navigation, row/tab/panel search, interactive rows and tabs, fullscreen mode, mouse selection, and value inspection.
- SVG/PNG export and changed-frame recording bundles.
- TOML configuration and 19 built-in themes, including Tokyo Night, Catppuccin, and Gruvbox flavors.
- Read-only external file or command-backed JSONL point annotations with panel targeting, tag filtering, and navigable cluster details.

## Documentation

- [User guide](https://ryancraig.github.io/grafana-tui/)
- [Installation](https://ryancraig.github.io/grafana-tui/installation.html)
- [Quick start](https://ryancraig.github.io/grafana-tui/quick-start.html)
- [Configuration](https://ryancraig.github.io/grafana-tui/configuration.html)
- [External annotations](https://ryancraig.github.io/grafana-tui/annotations.html)
- [Grafana dashboard import](https://ryancraig.github.io/grafana-tui/grafana-dashboard-import.html)
- [Grafana compatibility matrix](https://ryancraig.github.io/grafana-tui/grafana-compatibility.html)
- [Examples](examples/README.md)

## Common Commands

```bash
# Import a Grafana dashboard
grafana-tui --prometheus-url http://localhost:9090 --grafana-json ./dashboard.json

# Load several dashboards, one per tab (Tab / Shift+Tab to switch)
grafana-tui --grafana-json ./nodes.json --grafana-json ./consul.json

# Override Grafana template variables
grafana-tui --grafana-json ./dash.json --var job=node --var instance=server-01

# Use a theme (run --list-themes for every name)
grafana-tui --theme catppuccin-latte

# Overlay read-only JSONL point events
grafana-tui --grafana-json ./dashboard.json --annotations-file ./events.jsonl

# Query read-only annotations through a command provider
grafana-tui --grafana-json ./dashboard.json --annotations-command ./target/debug/examples/git_annotation_provider --annotations-command-arg=.

# Generate shell completions or a man page
grafana-tui completions zsh
grafana-tui man
```

Grafana 13 users can import an exact `dashboard.grafana.app/v2` JSON or YAML
resource with recursive `GridLayout`, `AutoGridLayout`, `RowsLayout`, and
`TabsLayout` containers, including repeated panels, rows, and tabs, conditional
rendering, and row and tab variables. Exports reference library panels by uid
only, so enable **Share dashboard with another instance** when exporting to
inline them. See the
[dashboard import guide](https://ryancraig.github.io/grafana-tui/grafana-dashboard-import.html)
for the current format requirements.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines and [DEVELOPMENT.md](DEVELOPMENT.md) for local development notes.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

Copyright 2025 Federico D'Ambrosio
