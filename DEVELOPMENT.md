# Development Notes

This document contains useful information for developing and testing grafana-tui.

## Testing Visualizations

### Using the Example Dashboard

The `examples/dashboards/all_visualizations.json` file demonstrates all supported panel types. To test it:

**Option 1: With a Running Prometheus Instance**
```bash
# If you have Prometheus running locally
cargo run -- --grafana-json examples/dashboards/all_visualizations.json

# Or connect to a remote Prometheus
cargo run -- --grafana-json examples/dashboards/all_visualizations.json --prometheus-url http://your-prometheus:9090
```

**Option 2: Quick Local Test with Prometheus**
```bash
# Download and run Prometheus locally for testing
docker run -p 9090:9090 prom/prometheus

# Then in another terminal:
cargo run -- --grafana-json examples/dashboards/all_visualizations.json
```

### Panel Types Tested

The example dashboard includes:
1. **Graph** - Time-series line chart
2. **Gauge** - Single value progress bar
3. **Stat** - Big number + sparkline
4. **Bar Gauge** - Comparative bars
5. **Table** - Tabular data view
6. **Heatmap** - Color-coded intensity grid

### Creating Test Dashboards

When creating test dashboards for new features:
1. Place them in `examples/dashboards/`
2. Name them descriptively (e.g., `feature_name_test.json`)
3. Document them in `examples/README.md`
4. Use realistic Prometheus queries that work with `prometheus_http_*` metrics

### UI/UX Example Rule

Every new UI/UX feature must update an existing example or add a new one. Each
example must remain compatible with the bundled mock/demo environment.

## Project Structure

```
grafana-tui/
├── src/                         # Rust source code
│   ├── main.rs                  # Program entry point and app wiring
│   ├── cli.rs                   # clap CLI argument and subcommand definitions
│   ├── config.rs                # TOML configuration loading and path expansion
│   ├── export.rs                # SVG/PNG export and changed-frame recordings
│   ├── grafana.rs               # Grafana dashboard JSON import
│   ├── prom.rs                  # Prometheus HTTP client
│   ├── theme.rs                 # Color themes and Grafana color parsing
│   ├── app/                     # Runtime state, input, refresh loop, and variables
│   │   ├── data.rs              # Query expansion, legend formatting, downsampling
│   │   ├── event_loop.rs        # Terminal event loop and export/recording actions
│   │   ├── input.rs             # Keyboard and mouse input handling
│   │   ├── state.rs             # App, panel, series, thresholds, and query state
│   │   └── variables.rs         # Dynamic Grafana template variable resolution
│   └── ui/                      # Ratatui rendering and formatting
│       ├── draw.rs              # Top-level UI composition
│       ├── format.rs            # Value, unit, and time formatting
│       ├── layout.rs            # Grid layout and panel hit-testing
│       └── panels/              # Panel renderers
│           ├── graph/           # Graph bounds, labels, overlays, thresholds, autogrid
│           ├── bar_gauge.rs     # Bar gauge renderer
│           ├── gauge.rs         # Gauge renderer
│           ├── heatmap.rs       # Heatmap renderer
│           ├── stat.rs          # Stat renderer
│           └── table.rs         # Table renderer
├── docs/                        # User guide: Astro Starlight site, built with bun
│   ├── astro.config.mjs         # Site base, sidebar, and edit links
│   └── src/content/docs/        # Guide pages, including grafana-compatibility.md
├── examples/                    # Example dashboards and local demo stack
│   ├── dashboards/              # Grafana dashboard JSON fixtures
│   └── demo/                    # Docker Compose Prometheus/node-exporter/vLLM demo
├── Makefile                     # Docs site targets (docs-dev, docs-build, ...)
└── Cargo.toml                   # Crate metadata and dependencies
```

## Running Tests

```bash
# Run all unit tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_calculate_y_bounds

# Build optimized release
cargo build --release
```

## Documentation Site

The user guide in `docs/` is an [Astro](https://astro.build/) site using the
[Starlight](https://starlight.astro.build/) theme. Pages are Markdown files in
`docs/src/content/docs/`, and the sidebar is set in `docs/astro.config.mjs`.
Link between pages with the site base, for example
`[Configuration](/grafana-tui/configuration/)`.

It needs [bun](https://bun.sh/) 1.3.9 or newer. From the repository root:

```bash
# Serve with live reload at http://localhost:4321/grafana-tui/
make docs-dev

# Build the static site into docs/dist, as CI does
make docs-build

# Serve the built site to check a production build
make docs-preview
```

The `Docs` workflow builds the site for pull requests that touch `docs/`, and
publishes it to <https://ryancraig.github.io/grafana-tui/> from `main`.

## Code Style

- Use `rustfmt` for formatting: `cargo fmt`
- Check lints: `cargo clippy`
- Document public APIs with doc comments (`///`)
- Keep functions focused and under 100 lines when possible
