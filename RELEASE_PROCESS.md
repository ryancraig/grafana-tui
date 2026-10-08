# Release Process

grafana-tui is released by pushing a version tag. There is no release bot:
no workflow bumps versions, opens pull requests, or creates tags. The only
workflow that can write to the repository is `release.yml`, and it only
creates the GitHub release for the tag you pushed.

GitHub Releases are the only distribution channel. grafana-tui is not
published to crates.io or any third-party package manager. Users install the
release archives with `install.sh`, or by hand into `~/.local/bin` or another
directory on their `PATH`.

## Workflows

| Workflow | Runs on | Permissions | Does |
|---|---|---|---|
| `ci.yml` | Every push to `main`, every pull request | read | The quality gate below |
| `docs-release.yml` | Pushes to `main` that touch `docs/` | Pages | Builds the user guide and deploys it to GitHub Pages |
| `release.yml` | Pushing a `vX.Y.Z` tag | read; `publish` job only: write | Builds the archives and creates the GitHub release |

### The quality gate

`ci.yml` runs the same checks as `make check`, on Linux and macOS:

- `make fmt`, failing if it changes anything
- `make lint`: clippy over every target, with warnings denied
- `make test`
- `make test-install`: the `install.sh` behavior tests
- `make cover-check`: fails below 95% line coverage
- `make vuln`: `cargo audit` against the RustSec advisory database

It also runs:

- **`windows-build`:** `cargo build` and clippy on Windows.
- **`release-shape`:** `make tidy` (fails if `Cargo.lock` is stale), then
  `make package`. It checks that the archive holds a working `grafana-tui` at
  its root, where `install.sh` expects it.
- **`docs`:** `make docs-build`, so a broken page fails review instead of the
  deploy.

A pull request that fails the gate is not mergeable.

## Cutting a Release

1. Start from an up-to-date `main` whose CI run passed.
2. Choose the version. grafana-tui follows [Semantic Versioning](https://semver.org/)
   and is pre-1.0: a breaking change bumps the minor version, and anything else
   bumps the patch version.
3. Set `version` in `Cargo.toml`, then refresh `Cargo.lock`:

   ```bash
   cargo update --workspace
   ```

4. Regenerate the changelog. This needs
   [git-cliff](https://git-cliff.org/) (`cargo install --locked git-cliff`):

   ```bash
   make changelog
   ```

   It prepends a section built from the conventional commits since the last
   tag, through `cliff.toml`, and leaves earlier entries as they are. The
   first release of the fork has no tag to start from, so pass the commit
   that completed the 0.1.12 changelog: `make changelog SINCE=ee4c529`.
   Review the new section and edit it by hand if needed.
5. Commit, tag, and push:

   ```bash
   git commit -am "chore(release): prepare for v$(make -s version)"
   git push origin main
   git tag "v$(make -s version)"
   git push origin "v$(make -s version)"
   ```

Pushing the tag starts `release.yml`:

1. **`verify`:** fails unless the tag matches the `Cargo.toml` version.
2. **`package`:** each archive is built natively by `make package`, on
   read-only runners:
   - `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`, on Ubuntu
     22.04, so the binaries need only glibc 2.35
   - `x86_64-apple-darwin` and `aarch64-apple-darwin`
   - `x86_64-pc-windows-msvc`, as a `.zip`
3. **`publish`:** the only job with write access. It runs `make checksums` to
   write `grafana-tui-checksums.txt`, then `gh release create` with every
   archive and the manifest, and GitHub-generated notes.

Afterwards, confirm that the release lists five archives and
`grafana-tui-checksums.txt`, and that `install.sh` installs it:

```bash
GRAFANA_TUI_VERSION="v$(make -s version)" bash install.sh
```

If a release job fails, fix the cause on `main`, then delete the tag and push
it again:

```bash
git push --delete origin vX.Y.Z && git tag -d vX.Y.Z
```

## Building Release Archives Locally

```bash
make package                                  # host target, into ./dist
make package TARGET=aarch64-unknown-linux-gnu # needs that target's toolchain
make checksums                                # dist/grafana-tui-checksums.txt
```

Each tarball holds `grafana-tui`, `README.md`, `LICENSE` and `NOTICE` at its
root.

## Commit Message Format

The changelog is built from [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

| Type | Use for | Changelog |
|------|---------|-----------|
| `feat` | New feature | Features |
| `fix` | Bug fix | Bug Fixes |
| `docs` | Documentation only | Documentation |
| `perf` | Performance improvement | Performance |
| `refactor` | Code refactoring | Refactor |
| `style` | Code style/formatting | Styling |
| `test` | Adding tests | Testing |
| `chore`, `ci` | Maintenance | Miscellaneous Tasks |

`chore(release): prepare for …` and `chore(deps…)` commits are left out.

Mark breaking changes with `!` after the type, or a `BREAKING CHANGE:` footer:

```bash
git commit -m "refactor(app)!: change AppState constructor signature

BREAKING CHANGE: AppState::new() now requires a Theme parameter"
```

Recommended scopes: `app`, `ui`, `prom`, `grafana`, `config`, `zoom`, `theme`.

## Repository Settings

- **Actions → General → Workflow permissions:** read repository contents
  (the default). Leave "Allow GitHub Actions to create and approve pull
  requests" off; no workflow needs it.
- **Pages → Source:** GitHub Actions.
- **Secrets:** none. Every workflow uses its own short-lived `GITHUB_TOKEN`.
