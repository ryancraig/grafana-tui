# Troubleshooting

## Prometheus Connection Refused

Grafatui keeps responding to keys while Prometheus is slow or down, and the
title bar shows the connection state:

| Title bar | Meaning |
|---|---|
| `◌ connecting` | The first refresh has not finished yet; panels show `Loading…` |
| `⟳ refreshing` | A refresh has been running for more than a second |
| `✗ Prometheus unreachable` | Every query in the last refresh failed to connect |

While Prometheus is unreachable, Grafatui retries with a growing delay, up to 30
seconds between attempts, and recovers on its own when Prometheus answers again.
Press `r` to retry immediately. Queries that Prometheus rejects, such as invalid
PromQL, show as panel errors rather than as unreachable.

Check that Prometheus is running and reachable:

```bash
curl http://localhost:9090/-/healthy
```

If you are using the demo stack, the Prometheus port is `19090`:

```bash
curl http://localhost:19090/-/healthy
```

## No Data Appears

Prometheus may need a few scrape intervals before data is available. Wait 10 to 15 seconds and force a refresh with `r`.

Also confirm that the dashboard queries match labels in your Prometheus server:

```bash
grafatui --prometheus-url http://localhost:9090 --grafana-json ./dashboard.json --var job=prometheus
```

## Dashboard Variables Do Not Match

Override variables explicitly with `--var`:

```bash
grafatui --grafana-json ./dashboard.json --var instance=localhost:9090
```

If a Grafana dashboard uses multi-select formatting modifiers such as `${var:csv}` or `${var:regex}`, check the [compatibility matrix](grafana-compatibility.md). Not every Grafana interpolation mode is implemented.

## Demo Port Conflict

The demo Prometheus service uses host port `19090`. If that port is already in use, edit `examples/demo/docker-compose.yml` and run Grafatui with the updated URL.

## Export Directory Problems

Set an explicit export directory:

```bash
grafatui --export-dir ./grafatui-exports
```

Make sure the directory is writable by your current user.
