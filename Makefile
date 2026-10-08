.PHONY: docs-install docs-dev docs-build docs-preview docs-clean

## --- Docs ---

# The user guide in docs/ is an Astro Starlight site, built with bun. CI
# (.github/workflows/docs.yml) builds it on pull requests and publishes it to
# https://ryancraig.github.io/grafana-tui/ from main.

# Installs exactly what docs/bun.lock pins, as CI does.
docs-install:
	cd docs && bun install --frozen-lockfile

docs/node_modules: docs/package.json docs/bun.lock
	cd docs && bun install --frozen-lockfile
	@touch $@

# Serves the site with live reload at http://localhost:4321/grafana-tui/.
docs-dev: docs/node_modules
	cd docs && bun --bun run dev

# Builds the static site into docs/dist, as CI does.
docs-build: docs/node_modules
	cd docs && bun --bun run build

# Serves the built docs/dist locally to check a production build.
docs-preview: docs-build
	cd docs && bun --bun run preview

docs-clean:
	rm -rf docs/dist docs/.astro docs/node_modules
