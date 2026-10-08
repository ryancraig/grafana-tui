---
title: Installation
description: Install grafana-tui into ~/.local/bin or another directory on your PATH.
---

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

Set `GRAFANA_TUI_INSTALL_DIR` to install into another directory on your `PATH`
that you can write to:

```bash
bash -o pipefail -c 'curl --proto =https --tlsv1.2 -LsSf https://raw.githubusercontent.com/ryancraig/grafana-tui/main/install.sh | GRAFANA_TUI_INSTALL_DIR=$HOME/bin bash'
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

## Prebuilt Archive

Each [GitHub Release](https://github.com/ryancraig/grafana-tui/releases)
publishes a `.tar.gz` archive for Linux and macOS on x86_64 and ARM64, plus
`grafana-tui-checksums.txt` with their SHA-256 hashes. To install one by hand,
download the archive for your platform and the checksum manifest, verify it,
and extract the binary into `~/.local/bin`:

```bash
asset=grafana-tui-x86_64-unknown-linux-gnu.tar.gz
base=https://github.com/ryancraig/grafana-tui/releases/latest/download
curl --proto '=https' --tlsv1.2 -LsSfO "${base}/${asset}"
curl --proto '=https' --tlsv1.2 -LsSfO "${base}/grafana-tui-checksums.txt"
sha256sum --check --ignore-missing grafana-tui-checksums.txt
mkdir -p ~/.local/bin
tar -xzf "${asset}" -C ~/.local/bin grafana-tui
```

On macOS, use `shasum -a 256 --check --ignore-missing` instead of `sha256sum`.

On Windows, download `grafana-tui-x86_64-pc-windows-msvc.zip` and extract
`grafana-tui.exe` into a directory on your `PATH`.

## From Source

grafana-tui requires Rust 1.88 or newer. Clone the repository and install the
binary into `~/.local/bin`:

```bash
git clone https://github.com/ryancraig/grafana-tui.git
cd grafana-tui
cargo install --path . --root ~/.local
```

For development, use `cargo run` instead:

```bash
cargo run -- --prometheus-url http://localhost:9090
```

## Making Sure It Is on PATH

Every method above installs a single self-contained binary; grafana-tui is not
distributed through crates.io or any third-party package manager. If your
shell cannot find `grafana-tui`, add the install directory to your `PATH`, for
example in `~/.bashrc` or `~/.zshrc`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

To uninstall, delete the binary from the directory you installed it into.

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
