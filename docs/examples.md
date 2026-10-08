# Examples

The repository includes example Grafana dashboards and a local demo environment.
Every example dashboard is a Grafana 13 V2 resource. The core examples were
converted from Classic JSON by Grafana itself; their Classic originals are kept
in `tests/fixtures/grafana/classic_examples`, and a test checks that both
formats import to the same dashboard, so either works with `--grafana-json`.

## Demo Stack

Start Prometheus, node-exporter, and mock vLLM metrics:

```bash
cd examples/demo
docker-compose up -d
```

Run grafana-tui from the repository root:

```bash
cargo run -- --grafana-json examples/dashboards/prometheus_demo.json --prometheus-url http://localhost:19090
```

Stop the demo:

```bash
cd examples/demo
docker-compose down -v
```

## Included Dashboards

- `examples/dashboards/prometheus_demo.json`: recommended first demo for the bundled Prometheus stack.
- `examples/dashboards/all_visualizations.json`: compact dashboard showing the supported visualization types, including timeseries bars, area fill, point mode, and hidden-axis examples.
- `examples/dashboards/instant_queries.json`: demonstrates explicit instant targets and the default instant behavior for summary panels.
- `examples/dashboards/thresholds_demo.json`: demonstrates thresholds, field bounds, and threshold marker rendering.
- `examples/dashboards/grafana_v2_compatibility.json`: exact Grafana V2 resource with a `GridLayout`, a dynamic Prometheus `job` variable, refresh settings, and two supported panels. Run it with:

  ```bash
  cargo run -- --grafana-json examples/dashboards/grafana_v2_compatibility.json --prometheus-url http://localhost:19090
  ```

- `examples/dashboards/grafana_v2_rows.json`: exact Grafana V2 resource showing expanded, nested, and initially collapsed `RowsLayout` rows with three Prometheus panels. Run it with:

  ```bash
  cargo run -- --grafana-json examples/dashboards/grafana_v2_rows.json --prometheus-url http://localhost:19090
  ```

- `examples/dashboards/grafana_v2_autogrid.json`: exact Grafana V2 resource with an `AutoGridLayout` of five panels. Resize the terminal to see the columns reflow. Run it with:

  ```bash
  cargo run -- --grafana-json examples/dashboards/grafana_v2_autogrid.json --prometheus-url http://localhost:19090
  ```

- `examples/dashboards/grafana_v2_repeats.yaml`: Grafana V2 resource in YAML, exported by Grafana 13, that repeats a panel over an `All` selection of HTTP handlers and a row over a dynamically resolved `job` query variable. Run it with:

  ```bash
  cargo run -- --grafana-json examples/dashboards/grafana_v2_repeats.yaml --prometheus-url http://localhost:19090
  ```

- `examples/dashboards/grafana_v2_conditional.json`: Grafana V2 resource with conditional rendering: a row shown only for `env=prod`, a row only for `staging`, a server-error panel shown only when it has data, and a tab shown only for time ranges of an hour or less. Zoom with `+`/`-` to see the tab come and go. Run it with:

  ```bash
  cargo run -- --grafana-json examples/dashboards/grafana_v2_conditional.json --prometheus-url http://localhost:19090 --range 1h
  ```

- `examples/dashboards/grafana_v2_sections.json`: Grafana V2 resource with row variables: a row that overrides the dashboard's `quantile`, and a row whose own `handler` query variable repeats a panel for every `/api/v1/` handler. Run it with:

  ```bash
  cargo run -- --grafana-json examples/dashboards/grafana_v2_sections.json --prometheus-url http://localhost:19090
  ```

- `examples/dashboards/grafana_v2_tabs.json`: exact Grafana V2 resource with tab, row, and empty-content behavior. Focus a tab bar and use Left/Right to switch or Enter/Space to enter its content. Run it with:

  ```bash
  cargo run -- --grafana-json examples/dashboards/grafana_v2_tabs.json --prometheus-url http://localhost:19090
  ```

- Several dashboards at once, one per tab: repeat `--grafana-json`. Press Tab / Shift+Tab to switch; only the dashboard shown queries Prometheus. Run it with:

  ```bash
  cargo run -- --grafana-json examples/dashboards/prometheus_demo.json --grafana-json examples/dashboards/grafana_v2_tabs.json --prometheus-url http://localhost:19090
  ```

- `examples/demo/vllm/grafana.json`: vLLM-oriented dashboard for the mock demo services.
- `examples/demo/vllm_demo.json`: compact vLLM cluster overview for the mock demo services.

## HashiStack RDW Dashboards

`examples/demo/hashistack-rdw/` holds eight production operator dashboards for
a Consul/Nomad/Vault platform with a Consul Connect (Envoy) mesh and remote
developer workspaces: fleet overview, nodes, Nomad, Consul, Vault, service
mesh, workspaces and Prometheus. They target a real datacenter's mTLS
Prometheus, not the bundled demo stack. See its
[README](https://github.com/ryancraig/grafana-tui/blob/main/examples/demo/hashistack-rdw/README.md)
for the reader-certificate setup. Open all eight at once, one per tab:

```bash
cargo run --release -- --config ~/.config/grafana-tui/gcloud-dc.toml \
  $(printf -- '--grafana-json %s ' examples/demo/hashistack-rdw/*.json)
```

## More Detail

See the repository example docs:

- [examples/README.md](https://github.com/ryancraig/grafana-tui/blob/main/examples/README.md)
- [examples/demo/README.md](https://github.com/ryancraig/grafana-tui/blob/main/examples/demo/README.md)
- [examples/demo/hashistack-rdw/README.md](https://github.com/ryancraig/grafana-tui/blob/main/examples/demo/hashistack-rdw/README.md)
