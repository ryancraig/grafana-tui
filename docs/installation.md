# Installation

## Homebrew

Install grafana-tui with Homebrew on macOS or Linux:

```bash
brew install ryancraig/grafana-tui/grafana-tui
```

## Installer Script

Install the latest prebuilt release without requiring Rust:

```bash
bash -o pipefail -c 'curl --proto =https --tlsv1.2 -LsSf https://raw.githubusercontent.com/ryancraig/grafana-tui/main/install.sh | bash'
```

With `wget`:

```bash
bash -o pipefail -c 'wget -O- https://raw.githubusercontent.com/ryancraig/grafana-tui/main/install.sh | bash'
```

The script supports Linux and macOS on x86_64 and ARM64. It installs to
`$HOME/.local/bin` and never invokes `sudo`. Make sure that directory is on
your `PATH`.

Set `GRAFANA_TUI_INSTALL_DIR` to choose another destination:

```bash
bash -o pipefail -c 'curl --proto =https --tlsv1.2 -LsSf https://raw.githubusercontent.com/ryancraig/grafana-tui/main/install.sh | GRAFANA_TUI_INSTALL_DIR=/custom/bin bash'
```

Set `GRAFANA_TUI_VERSION` to install a specific release. The leading `v` is
optional:

```bash
bash -o pipefail -c 'curl --proto =https --tlsv1.2 -LsSf https://raw.githubusercontent.com/ryancraig/grafana-tui/main/install.sh | GRAFANA_TUI_VERSION=v0.1.11 bash'
```

Every release download is verified against its published SHA-256 checksum
manifest. Installation stops if the manifest is unavailable or verification
fails.

Reviewing downloaded scripts before running them is recommended:

```bash
curl --proto '=https' --tlsv1.2 -LsSf -o install.sh https://raw.githubusercontent.com/ryancraig/grafana-tui/main/install.sh
less install.sh
bash install.sh
```

## From Crates.io

Install the latest published release with Cargo:

```bash
cargo install grafana-tui
```

grafana-tui currently requires Rust 1.88 or newer.

## From Source

Clone the repository and install the local checkout:

```bash
git clone https://github.com/ryancraig/grafana-tui.git
cd grafana-tui
cargo install --path .
```

For development, use `cargo run` instead:

```bash
cargo run -- --prometheus-url http://localhost:9090
```

## Prebuilt Binaries

Prebuilt release assets are published on [GitHub Releases](https://github.com/ryancraig/grafana-tui/releases) for common Linux, macOS, and Windows targets.

## Shell Completions

grafana-tui can generate shell completions for Bash, Zsh, Fish, PowerShell, and Elvish.

```bash
# Bash
source <(grafana-tui completions bash)

# Zsh
source <(grafana-tui completions zsh)

# Fish
grafana-tui completions fish | source
```

## Man Page

Generate a man page from the CLI definition:

```bash
grafana-tui man > grafana-tui.1
```
