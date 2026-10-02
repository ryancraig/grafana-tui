# Configuration

Grafatui can be configured with CLI options, a TOML configuration file, or both. CLI options override values from the configuration file.

## Common CLI Options

| Option | Description | Default |
|---|---|---|
| `--prometheus-url <URL>` | Prometheus server URL | `http://localhost:9090` |
| `--ca-cert <FILE>` | PEM CA certificates to trust instead of the built-in roots (see [TLS](#connecting-to-an-mtls-prometheus)) | built-in roots |
| `--client-cert <FILE>` | PEM client certificate for mutual TLS, followed by any intermediate CAs; needs `--client-key` | none |
| `--client-key <FILE>` | PEM private key for `--client-cert` | none |
| `--grafana-json <FILE>` | Grafana dashboard file: Classic JSON, or V2 resource JSON or YAML (alias `--grafana-dashboard`) | none |
| `--annotations-file <FILE>` | Read-only external JSONL point-event file | none |
| `--annotations-command <PROGRAM>` | Read-only executable annotation provider | none |
| `--annotations-command-arg <ARG>` | Argument for `--annotations-command`; repeat to preserve order | none |
| `--annotations-command-timeout <DURATION>` | Maximum command-provider runtime | `10s` |
| `--validate` | Check the Grafana dashboard import and exit without starting the TUI | `false` |
| `--strict` | Make `--validate` fail when diagnostics contain warnings | `false` |
| `--format <FORMAT>` | Output format for `--validate`: `text` or `json` | `text` |
| `--range <DURATION>` | Time range window, such as `5m`, `1h`, or `24h` | `5m` |
| `--step <DURATION>` | Finest query step, such as `5s` or `30s`; longer ranges use a coarser step (see [Query resolution](#query-resolution)) | `5s` |
| `--scrape-interval <DURATION>` | Prometheus scrape interval, used for `$__rate_interval` | `15s` |
| `--var <KEY=VALUE>` | Override a dashboard variable (not a V2 row or tab variable); repeat a key to select several values | none |
| `--theme <NAME>` | UI theme | `tokyo-night` |
| `--list-themes` | Print the available theme names and exit | |
| `--transparent-background` | Keep the terminal's background instead of painting the theme's | `false` |
| `--threshold-marker <MARKER>` | Marker for threshold lines | `dashed` |
| `--autogrid-color <COLOR>` | Color for automatic graph grid lines and labels | theme grid color |
| `--export-dir <DIR>` | Directory for exports and recordings | `./grafatui-exports` |
| `--export-format <FORMAT>` | `svg`, `png`, or `both` | `svg` |
| `--record-max-frames <COUNT>` | Maximum changed frames per recording | `300` |
| `--refresh-rate <MS>` | Data fetch interval in milliseconds | `1000` |
| `--config <FILE>` | Configuration file path | none |

Run the full help output with:

```bash
grafatui --help
```

## Configuration File

Create `grafatui.toml` in `~/.config/grafatui/`, or pass a custom path with `--config`.

```toml
prometheus_url = "http://localhost:9090"
refresh_rate = 1000
time_range = "1h"
step = "5s"
scrape_interval = "15s"
theme = "dracula"
transparent_background = false
threshold_marker = "dashed"
export_dir = "./grafatui-exports"
export_format = "svg"
record_max_frames = 300
autogrid = true
autogrid_color = "dark-gray"  # omit to use the theme's grid color
grafana_json = "~/.config/grafatui/my-dashboard.json"
annotations_file = "./events.jsonl"

[vars]
job = "node"
instance = "server-01"
```

## Connecting to an mTLS Prometheus

TLS settings are optional. Without them, an `https://` URL is verified against
the built-in web PKI roots, and a plain `http://` URL works as it always has.

To reach a Prometheus that uses a private CA and requires client
certificates, set the `[tls]` table:

```toml
prometheus_url = "https://10.60.1.21:9090"

[tls]
ca_cert     = "~/.config/grafatui/tls/ca.pem"
client_cert = "~/.config/grafatui/tls/client.pem"
client_key  = "~/.config/grafatui/tls/client.key"
```

The CLI flags do the same. Each one overrides only its own `[tls]` key:

```bash
grafatui --prometheus-url https://10.60.1.21:9090 \
  --ca-cert ca.pem --client-cert client.pem --client-key client.key
```

| Key | Flag | Contents |
|---|---|---|
| `ca_cert` | `--ca-cert` | One or more PEM CA certificates. Only these CAs are trusted: the built-in roots are turned off. List both CAs during a CA rotation, and a server signed by either one verifies. |
| `client_cert` | `--client-cert` | The PEM client certificate, followed by any intermediate CAs that issued it. |
| `client_key` | `--client-key` | The client certificate's unencrypted PEM private key: PKCS#8 (`BEGIN PRIVATE KEY`), SEC1 (`BEGIN EC PRIVATE KEY`), or PKCS#1 (`BEGIN RSA PRIVATE KEY`). |

- Paths may start with `~`.
- `client_cert` and `client_key` must be set together.
- The options are independent: use `ca_cert` alone for a private-CA server
  that doesn't ask for client certificates, or `client_cert` and `client_key`
  alone for a publicly trusted one that does.
- Servers addressed by IP need the IP in their certificate's subject
  alternative names. TLS 1.2 and 1.3 are supported.
- `[tls]` rejects unknown keys, so a misspelled option is an error rather than
  silently ignored.

Grafatui checks the files at startup and exits with an error that names the
file and the problem when:

- a TLS option is set but `prometheus_url` isn't `https://`;
- a file is missing or unreadable, or isn't valid PEM;
- `ca_cert` or `client_cert` holds no certificate, or `client_key` holds no key
  or an encrypted one;
- `client_key` isn't the key of `client_cert`.

Errors show file paths, never key contents. Failures during the connection,
such as an unknown issuer or a rejected client certificate, show in the title
bar instead; see
[TLS Errors](troubleshooting.md#tls-errors).

## Query Resolution

Range queries choose their step the way Grafana's Prometheus datasource does:

1. Divide the time range by the panel's `maxDataPoints`, or by 1000 when it is
   unset.
2. Round to a Grafana interval, such as `20s`, `1m`, or `10m`.
3. Use the min interval instead when it is larger. The min interval is the
   target's `interval`, then the panel's `interval`, then `--step`.
4. Coarsen the step if the query would exceed Prometheus's limit of 11,000
   points per series.

`$__interval` is the step a query runs with. `$__rate_interval` is
`max($__interval + scrape interval, 4 × scrape interval)`. The scrape interval
is the query's min interval when the dashboard sets one, and `--scrape-interval`
otherwise.

With the defaults, a `5m` or `1h` range queries every `5s`, `24h` every `1m`,
and `7d` every `10m`. The title bar shows the step for panels without their own
query options.

## External Annotation Sources

Select one read-only annotation source: `annotations_file` or the nested
`[annotations_command]` table. The two TOML forms conflict. The
`--annotations-file` CLI flag conflicts with every command-source CLI flag;
CLI source selection still replaces the complete TOML annotation source.

```toml
[annotations_command]
program = "./target/debug/examples/git_annotation_provider"
args = ["."]
timeout = "10s"
```

`program` is required; `args` defaults to an empty list and `timeout` defaults
to `10s`. The matching CLI source is:

```bash
grafatui \
  --annotations-command ./target/debug/examples/git_annotation_provider \
  --annotations-command-arg=. \
  --annotations-command-timeout 10s
```

`--annotations-command-arg` and `--annotations-command-timeout` require
`--annotations-command`; repeat the argument flag to retain argument order.
`--annotations-file` and `--annotations-command` cannot be combined. A CLI file
or command has whole-source precedence over TOML: it replaces the configured
file or complete command configuration rather than merging individual fields.

## Themes

The default theme is `tokyo-night`. Run `grafatui --list-themes` to print every
available name; the theme currently selected is marked `(current)`.

| Family | Themes | Aliases |
|---|---|---|
| [Tokyo Night](https://github.com/folke/tokyonight.nvim) | `tokyo-night`, `tokyo-night-storm`, `tokyo-night-moon`, `tokyo-night-day` | `default`, `tokyo-night-night` |
| [Catppuccin](https://catppuccin.com/palette) | `catppuccin-mocha`, `catppuccin-macchiato`, `catppuccin-frappe`, `catppuccin-latte` | `catppuccin` (mocha) |
| [Gruvbox](https://github.com/morhetz/gruvbox) | `gruvbox-dark`, `gruvbox-dark-hard`, `gruvbox-dark-soft`, `gruvbox-light`, `gruvbox-light-hard`, `gruvbox-light-soft` | `gruvbox` (dark) |
| Other | `dracula`, `monokai`, `solarized-dark`, `solarized-light`, `terminal` | |

`terminal` uses the terminal's own ANSI colors and background. Theme names are
case-insensitive, and an unknown name is an error that lists the valid ones.

Press `T` while Grafatui runs to preview and switch themes live; see
[Theme Picker](keyboard-and-mouse.md#theme-picker).

Every theme colors the whole interface: panel chrome, popups, axes, grid,
cursor, gauges, heatmaps, status messages, and SVG/PNG exports.

Themes other than `terminal` paint their own background, so light themes stay
readable in a dark terminal. Set `transparent_background = true`, or pass
`--transparent-background`, to keep the terminal's background instead.
Exports always use the theme's background.

Use a theme from the CLI:

```bash
grafatui --theme catppuccin-latte
```

### Custom Themes

Define your own themes in `[themes.<name>]` tables. Each starts from a built-in
theme (`extends`, default `tokyo-night`) and overrides any of the roles below.
Select it like a built-in, with `theme = "<name>"` or `--theme <name>`.

```toml
theme = "night-shift"

[themes.night-shift]
extends = "tokyo-night-storm"
title = "#ff9e64"
border_focused = "#ff9e64"
grid = "#3b4261"
palette = ["#7aa2f7", "#9ece6a", "#e0af68", "#bb9af7"]

# Reusing a built-in name tweaks that theme in place.
[themes.catppuccin-latte]
extends = "catppuccin-latte"
background = "#ffffff"
```

Colors are `#rrggbb`, an ANSI name (`black`, `red`, `green`, `yellow`, `blue`,
`magenta`, `cyan`, `gray`, `dark-gray`, `white`, or a `light-` variant such as
`light-blue`), or `reset` for the terminal's own color.

| Key | Colors |
|---|---|
| `background` | Behind the whole dashboard |
| `surface` | Behind popups and modals |
| `text`, `text_muted`, `title` | Body text, hints, and panel titles |
| `border`, `border_focused` | Panel borders, and the selected panel or row |
| `selection_fg`, `selection_bg` | Highlighted entries in lists |
| `error`, `warning`, `success` | Panel errors, recording and paused indicators |
| `axis`, `grid`, `cursor` | Graph axes, automatic grid, and the inspect cursor |
| `gauge_track` | Unfilled part of gauges and bar gauges |
| `heatmap` | Three colors: low, mid, and high heatmap bands |
| `heatmap_empty` | Heatmap cells without a value |
| `annotation` | Annotation markers and details |
| `threshold_default` | Threshold lines whose dashboard color has no terminal equivalent |
| `palette` | Series colors, in order (at least one) |

Unknown keys and invalid colors are reported at startup with the theme and key
that caused them. `autogrid_color`, when set, still takes precedence over the
theme's `grid`.
