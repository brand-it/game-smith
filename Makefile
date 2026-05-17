.PHONY: help setup setup-check dev dev-desktop watch \
	test test-desktop fmt fmt-check lint qa \
	migrate-gen migrate-up build build-desktop release clean reset

help:
	@echo "Usage: make <target>"
	@echo ""
	@echo "Setup"
	@echo "  setup         Install system dependencies and configure local build"
	@echo "  setup-check   Check dependencies without installing"
	@echo ""
	@echo "Development"
	@echo "  dev           Start dev server (localhost:5150)"
	@echo "  dev-desktop   Start dev server with desktop features (tray + browser)"
	@echo "  watch         Auto-restart on file changes (requires cargo-watch)"
	@echo ""
	@echo "Quality"
	@echo "  test          Run all tests"
	@echo "  test-desktop  Run tests with desktop feature"
	@echo "  fmt           Format code"
	@echo "  fmt-check     Check formatting"
	@echo "  lint          Run clippy with strict rules"
	@echo "  qa            Run fmt-check, lint, and test"
	@echo ""
	@echo "Migrations"
	@echo "  migrate-gen   Generate new migration (NAME=create_games)"
	@echo "  migrate-up    Run pending migrations"
	@echo ""
	@echo "Build"
	@echo "  build         Build without features"
	@echo "  build-desktop Build with desktop features"
	@echo "  release       Production build with desktop features"
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

dev-desktop:
	cargo run --features desktop -- start

watch:
	cargo watch -x "run -- start"

# ── Quality ────────────────────────────────────────────────────────────

test:
	cargo test

test-desktop:
	cargo test --features desktop

fmt:
	cargo fmt --all

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

build-desktop:
	cargo build --features desktop

release:
	cargo build --release --features desktop

# ── Cleanup ────────────────────────────────────────────────────────────

clean:
	cargo clean

reset:
	rm -f *.sqlite
	cargo clean
