# Troubleshooting

## Prometheus Connection Refused

Grafatui keeps responding to keys while Prometheus is slow or down, and the
title bar shows the connection state:

| Title bar | Meaning |
|---|---|
| `◌ connecting` | The first refresh has not finished yet; panels show `Loading…` |
| `⟳ refreshing` | A refresh has been running for more than a second |
| `✗ Prometheus unreachable` | Every query in the last refresh failed to connect |
| `✗ Prometheus unreachable (TLS: …)` | The connection failed during TLS; see [TLS Errors](#tls-errors) |

While Prometheus is unreachable, Grafatui retries with a growing delay, up to 30
seconds between attempts, and recovers on its own when Prometheus answers again.
Press `r` to retry immediately. Queries that Prometheus rejects, such as invalid
PromQL, show as panel errors rather than as unreachable.

## TLS Errors

When an `https://` connection fails during TLS, the title bar names the cause.
The panel error repeats it, with what to check and the underlying error.
Grafatui retries with the same growing delay as for an unreachable server, so
it recovers once the certificates are fixed. Settings are described in
[Connecting to an mTLS Prometheus](configuration.md#connecting-to-an-mtls-prometheus).

### `TLS: unknown issuer`

The server's certificate isn't signed by a CA Grafatui trusts. With `ca_cert`
unset, only the built-in web PKI roots are trusted, so a server with a private
CA always fails this way. Set `ca_cert` to the CA bundle. If it is already set,
check that the bundle has the CA that signed the server's certificate. During
a CA rotation, include both CAs:

```bash
openssl s_client -connect 10.60.1.21:9090 -showcerts </dev/null  # the server's chain
openssl x509 -in ca.pem -noout -subject                            # the first CA in the bundle
```

### `TLS: client certificate required`

The server requires a client certificate, and none was sent. Set `client_cert`
and `client_key`.

### `TLS: client certificate rejected (…)`

The server received the client certificate but didn't accept it. The alert in
parentheses comes from the server: `UnknownCA` or `BadCertificate` usually
means a CA the server doesn't trust issued the certificate, and
`CertificateExpired` that it has expired. Check the certificate against the
server's client CA:

```bash
openssl verify -CAfile client-ca.pem client.pem
openssl x509 -in client.pem -noout -subject -issuer -enddate
```

### `TLS: certificate not valid for this host`

The server's certificate doesn't list the host or IP address in
`prometheus_url` among its subject alternative names. Use the name the
certificate was issued for, or reissue it with an IP SAN.

## Panel Markers

A panel keeps showing data when a later fetch has problems, and marks its title:

| Panel title | Meaning |
|---|---|
| `⚠ stale: queries failed` | Every query of the latest fetch failed; the chart shows the last data that loaded |
| `⚠ query failed` | Some of the panel's queries failed; the chart shows the ones that succeeded |
| `⚠ warning` | Prometheus returned the data with warnings, such as partial results |

Select the panel to see the error or warning in the footer. A panel that has
never loaded data shows the full error instead. A long title is shortened so
the marker stays in view.

Prometheus 3 also returns informational notices, such as `metric might not be a
counter` for `rate()` over a counter whose name does not end in `_total`. The
data is fine, so these get no marker: select the panel to see them in the
footer as `info:`. Prometheus rejects a query with
a 4xx status the same way every time, so only connection failures and 5xx or
429 responses are retried.

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
