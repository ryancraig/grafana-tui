# grafana-tui — build tooling
BINARY    := grafana-tui
VERSION   := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
TARGET    ?= $(shell rustc -vV 2>/dev/null | sed -n 's/^host: //p')
DIST_DIR  := dist
PREFIX    ?= $(HOME)/.local
COVER_MIN ?= 95
CARGO     ?= cargo

.DEFAULT_GOAL := build

.PHONY: all build install run test test-install cover cover-html cover-check lint fmt vuln tidy \
        check package checksums version changelog clean tools help \
        docs-install docs-dev docs-build docs-preview docs-clean

all: check build

build: ## Build the release binary into target/release
	$(CARGO) build --release --locked

install: ## Install grafana-tui into PREFIX/bin (default ~/.local/bin)
	$(CARGO) install --path . --locked --root $(PREFIX)

run: ## Build and run the TUI; pass flags with ARGS="--prometheus-url ..."
	$(CARGO) run --locked -- $(ARGS)

test: ## Run unit and integration tests
	$(CARGO) test --locked

test-install: ## Run the install.sh behavior tests
	bash tests/install.sh

cover: ## Run tests with coverage and print the per-file summary
	$(CARGO) llvm-cov --locked

cover-html: ## Open an HTML coverage report
	$(CARGO) llvm-cov --locked --open

cover-check: ## Fail if line coverage < COVER_MIN (default 95%)
	$(CARGO) llvm-cov --locked --summary-only --fail-under-lines $(COVER_MIN)

lint: ## Run clippy over every target, denying warnings
	$(CARGO) clippy --locked --all-targets -- -D warnings

fmt: ## rustfmt over the tree
	$(CARGO) fmt

vuln: ## cargo-audit against the RustSec advisory database (needs the network)
	$(CARGO) audit

tidy: ## Fail if Cargo.lock is out of date with Cargo.toml (CI-friendly)
	$(CARGO) metadata --locked --format-version 1 >/dev/null

check: fmt lint test test-install cover-check ## Everything CI runs (lint + tests + coverage gate)

# install.sh extracts the binary by name from the archive root, so it sits at
# the top level of the tarball, next to the license files.
package: ## Build the release tarball for TARGET (default: host) into ./dist
	$(CARGO) build --release --locked --target $(TARGET)
	@mkdir -p $(DIST_DIR)
	@stage=$$(mktemp -d) && \
		cp target/$(TARGET)/release/$(BINARY) README.md LICENSE NOTICE "$$stage"/ && \
		tar -czf $(DIST_DIR)/$(BINARY)-$(TARGET).tar.gz -C "$$stage" $(BINARY) README.md LICENSE NOTICE && \
		rm -rf "$$stage"
	@echo "-> $(DIST_DIR)/$(BINARY)-$(TARGET).tar.gz"

checksums: ## Write the SHA-256 manifest install.sh verifies against
	@cd $(DIST_DIR) && files=$$(ls $(BINARY)-*.tar.gz $(BINARY)-*.zip 2>/dev/null || true) && \
		if [ -z "$$files" ]; then echo "no archives in $(DIST_DIR)"; exit 1; fi && \
		if command -v sha256sum >/dev/null 2>&1; then sha256sum $$files; else shasum -a 256 $$files; fi \
			> $(BINARY)-checksums.txt
	@cat $(DIST_DIR)/$(BINARY)-checksums.txt

version: ## Print the version from Cargo.toml
	@echo $(VERSION)

# The new section covers SINCE..HEAD, where SINCE defaults to the latest tag.
# The fork's first release has no tag to start from; pass the commit that
# released the last version in CHANGELOG.md: make changelog SINCE=ee4c529
SINCE ?= $(shell git describe --tags --abbrev=0 2>/dev/null)
changelog: ## Prepend the v$(VERSION) section to CHANGELOG.md (needs git-cliff)
	@[ -n "$(SINCE)" ] || { echo "no tag to start from; pass SINCE=<commit>"; exit 1; }
	git cliff --tag v$(VERSION) --prepend CHANGELOG.md $(SINCE)..HEAD

clean: ## Remove build artifacts
	rm -rf $(DIST_DIR)
	$(CARGO) clean

tools: ## Install dev tools (clippy, rustfmt, llvm-tools, cargo-audit, cargo-llvm-cov)
	rustup component add clippy rustfmt llvm-tools-preview
	$(CARGO) install --locked cargo-audit cargo-llvm-cov

help: ## Show this help
	@grep -hE '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-13s\033[0m %s\n", $$1, $$2}'

## Docs

# The user guide in docs/ is an Astro Starlight site, built with bun. CI
# builds it on every pull request, and .github/workflows/docs-release.yml
# publishes it to https://ryancraig.github.io/grafana-tui/ from main.

docs/node_modules: docs/package.json docs/bun.lock
	cd docs && bun install --frozen-lockfile
	@touch $@

docs-install: ## Install exactly what docs/bun.lock pins
	cd docs && bun install --frozen-lockfile

docs-dev: docs/node_modules ## Serve the docs with live reload at http://localhost:4321/grafana-tui/
	cd docs && bun --bun run dev

docs-build: docs/node_modules ## Build the docs site into docs/dist
	cd docs && bun --bun run build

docs-preview: docs-build ## Serve the built docs/dist to check a production build
	cd docs && bun --bun run preview

docs-clean: ## Remove the docs build output and dependencies
	rm -rf docs/dist docs/.astro docs/node_modules
