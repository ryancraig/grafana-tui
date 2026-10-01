# Configuration

Grafatui can be configured with CLI options, a TOML configuration file, or both. CLI options override values from the configuration file.

## Common CLI Options

| Option | Description | Default |
|---|---|---|
| `--prometheus-url <URL>` | Prometheus server URL | `http://localhost:9090` |
| `--grafana-json <FILE>` | Grafana dashboard file: Classic JSON, or V2 resource JSON or YAML (alias `--grafana-dashboard`) | none |
| `--annotations-file <FILE>` | Read-only external JSONL point-event file | none |
| `--annotations-command <PROGRAM>` | Read-only executable annotation provider | none |
| `--annotations-command-arg <ARG>` | Argument for `--annotations-command`; repeat to preserve order | none |
| `--annotations-command-timeout <DURATION>` | Maximum command-provider runtime | `10s` |
| `--validate` | Check the Grafana dashboard import and exit without starting the TUI | `false` |
| `--strict` | Make `--validate` fail when diagnostics contain warnings | `false` |
| `--format <FORMAT>` | Output format for `--validate`: `text` or `json` | `text` |
| `--range <DURATION>` | Time range window, such as `5m`, `1h`, or `24h` | `5m` |
| `--step <DURATION>` | Query step resolution, such as `5s` or `30s` | `5s` |
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
