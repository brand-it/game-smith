.PHONY: help setup setup-check dev watch \
	test fmt fmt-check style lint qa \
	migrate-gen migrate-up build build-css release package package-rpm install run-release clean reset

help:
	@echo "Usage: make <target>"
	@echo ""
	@echo "Setup"
	@echo "  setup         Install system dependencies and configure local build"
	@echo "  setup-check   Check dependencies without installing"
	@echo ""
	@echo "Development"
	@echo "  dev           Start dev server (localhost:5150)"
	@echo "  watch         Auto-restart on file changes (requires cargo-watch)"
	@echo ""
	@echo "Quality"
	@echo "  test          Run all tests"
	@echo "  fmt           Format code"
	@echo "  fmt-check     Check formatting"
	@echo "  style         Auto-fix formatting and lint issues"
	@echo "  lint          Run clippy with strict rules"
	@echo "  qa            Run fmt-check, lint, and test"
	@echo ""
	@echo "Migrations"
	@echo "  migrate-gen   Generate new migration (NAME=create_games)"
	@echo "  migrate-up    Run pending migrations"
	@echo ""
	@echo "Build"
	@echo "  build         Build (debug)"
	@echo "  build-css     Compile Tailwind CSS from source to static assets"
	@echo "  release       Production build"
	@echo "  package       Build .deb and .AppImage packages (requires cargo-packager)"
	@echo "  package-rpm   Build .rpm package via podman (requires podman)"
	@echo ""
	@echo "Cleanup"
	@echo "  clean         Remove build artifacts"
	@echo "  reset         Full reset (remove DB + build artifacts)"

# ── Setup ──────────────────────────────────────────────────────────────

setup:
	./scripts/setup.sh

setup-check:
	./scripts/setup.sh --check

# ── Development ────────────────────────────────────────────────────────

dev:
	cargo run -- start

watch:
	cargo watch -x "run -- start"

# ── Quality ────────────────────────────────────────────────────────────

test:
	cargo test

fmt:
	cargo fmt --all


style:
	cargo fmt --all
	cargo clippy --fix --allow-dirty --allow-staged \
	  -- -W clippy::pedantic -W clippy::nursery -W rust-2018-idioms
fmt-check:
	cargo fmt --all -- --check

lint:
	cargo clippy -- -D warnings -W clippy::pedantic -W clippy::nursery -W rust-2018-idioms

qa:
	@$(MAKE) fmt-check && $(MAKE) lint && $(MAKE) test

# ── Migrations ─────────────────────────────────────────────────────────

migrate-gen:
ifndef NAME
	$(error NAME is required. Usage: make migrate-gen NAME=create_games)
endif
	cargo run -- generate migration $(NAME)

migrate-up:
	cargo run -- db up

# ── Build ──────────────────────────────────────────────────────────────

build:
	cargo build

TAILWIND_CLI := node_modules/.bin/tailwindcss
TWINDEIND_VERSION := v4.1.14

# Download Tailwind CLI binary if missing
$(TAILWIND_CLI):
	mkdir -p node_modules/.bin
	curl -fsSL -o $(TAILWIND_CLI) \
	  https://github.com/tailwindlabs/tailwindcss/releases/download/$(TWINDEIND_VERSION)/tailwindcss-linux-x64
	chmod +x $(TAILWIND_CLI)

# Compile Tailwind CSS from source to static assets
build-css: $(TAILWIND_CLI)
	$(TAILWIND_CLI) -i assets/css/tailwind.css -o assets/static/css/tailwind.css \
	  --minify

VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

package:
	NO_STRIP=1 cargo packager --release
	@echo ""
	@echo "Packages built:"
	@ls -1 target/release/game-smith_$(VERSION)* target/release/game-smith-$(VERSION)* 2>/dev/null | sed 's/^/  /'
package-rpm: package
	podman run --rm --security-opt label=disable \
	  -v "$(shell pwd):/app:ro" \
	  -v "$(shell pwd)/target/release:/out" \
	  fedora:41 bash -c " \
	    dnf install -y rpm-build 2>/dev/null | tail -3 && \
	    rpmbuild -bb /app/packaging/rpm/game-smith.spec \
	      --define '_rpmdir /out' \
	      --define 'srcdir /app' \
	      --define 'version $(VERSION)' \
	  "
	@echo ""
	@echo "Packages built:"
	@ls -1 target/release/game-smith_$(VERSION)* target/release/game-smith-$(VERSION)* \
	        target/release/x86_64/game-smith-$(VERSION)*.rpm 2>/dev/null | sed 's/^/  /'


# ── Install & Run ──────────────────────────────────────────────────────

# Build and install to ~/.local/bin (no reboot needed)
install: release
	install -Dm755 target/release/game-smith $(HOME)/.local/bin/game-smith
	@echo ""
	@echo "Installed to $$HOME/.local/bin/game-smith"

# Build and run directly (for testing changes without install)
run-release: release
	./target/release/game-smith start
release:
	cargo build --release

# ── Cleanup ────────────────────────────────────────────────────────────

clean:
	cargo clean

reset:
	rm -f *.sqlite
	cargo clean
