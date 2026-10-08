# Introduction

grafana-tui is a terminal user interface for Prometheus dashboards. It is designed for fast inspection, SSH sessions, local debugging, and environments where opening a browser-based Grafana instance is inconvenient.

grafana-tui reads Prometheus directly and can import Grafana dashboard JSON files. It renders supported panels as terminal charts, tables, gauges, stats, and heatmaps while keeping the workflow keyboard-first.

## When grafana-tui Fits

Use grafana-tui when you want:

- A lightweight Prometheus dashboard in your terminal.
- A familiar way to inspect exported Grafana dashboards.
- Fast startup and low resource usage.
- A dashboard that works well over SSH.
- SVG or PNG snapshots of the current TUI view.

grafana-tui is not a Grafana server replacement. It does not manage users, alerts, annotations, dashboard editing, plugins, or browser-only visualizations.

## Project Links

- [Repository](https://github.com/ryancraig/grafana-tui)
- [Crate](https://crates.io/crates/grafana-tui)
- [Rust API docs](https://docs.rs/grafana-tui)
- [Grafana compatibility matrix](grafana-compatibility.md)
