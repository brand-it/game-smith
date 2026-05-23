# Repository Guidelines

## Project Overview

Rust web application built with [loco-rs](https://github.com/loco-rs/loco) framework. Provides user authentication (register, login, password reset, magic links, email verification) with JWT-based auth.

## Architecture & Data Flow

```
src/bin/main.rs  →  cli::main::<App, Migrator>()  →  App implements Hooks
                                                      ├── initializers()  → ViewEngineInitializer
                                                      ├── routes()       → controllers::auth::routes()
                                                      └── connect_workers() → background workers
```

- `src/app.rs` — `App` struct implements `Hooks` trait: the single entry point defining routes, initializers, workers, seed/truncate logic.
- `src/models/_entities/` — Sea-ORM generated entities (auto-generated, do not edit).
- `src/models/users.rs` — User model with custom behavior (`ActiveModelBehavior`, `Authenticable`, lookup methods).
- `src/controllers/` — HTTP handlers using Axum extractors (`State<AppContext>`, `Json<T>`, `Path<T>`).
- `src/mailers/` — Email templates using Fluent i18n for localization.
- `src/views/` — Response DTOs (e.g., `LoginResponse`, `CurrentResponse`).
- `src/data/` — Data access layer.
- `src/initializers/` — App lifecycle hooks (runs before/after routes).
- `src/workers/` — Background workers registered via loco's `Queue`.
- `src/tasks/` — CLI tasks.

## Key Directories

| Path | Purpose |
|------|---------|
| `src/bin/main.rs` | Binary entry point |
| `src/app.rs` | App hooks (routes, initializers, workers) |
| `src/models/_entities/` | Auto-generated Sea-ORM entities — do not edit |
| `src/models/users.rs` | User model with custom logic |
| `src/controllers/auth.rs` | Authentication HTTP handlers |
| `src/mailers/auth.rs` | Email templates (Fluent i18n) |
| `src/initializers/view_engine.rs` | Tera + Fluent i18n setup |
| `config/*.yaml` | Environment configuration (dev, test) |
| `migration/` | Database migrations (Sea-ORM Migrator) |
| `assets/i18n/` | Fluent resource files |
|| `assets/i18n/_shared.ftl` | Shared Fluent terms (underscore prefix prevents ArcLoader from parsing as language tag) |
|| `src/desktop/` | Desktop integration (tray, notifications, browser open) |
|| `assets/icons/` | Tray icon assets |
|| `tests/` | Integration tests organized by domain |
## Development Commands

```bash
# Build
cargo build

# Run (development)
cargo run

# Desktop integration is always included (tray icon, notifications, browser auto-open)
cargo build

# Format (check only)
cargo fmt --all -- --check

# Lint (strict — pedantic + nursery + rust-2018-idioms, warnings as errors)
cargo clippy -- -D warnings -W clippy::pedantic -W clippy::nursery -W rust-2018-idioms

# Test (full suite)
cargo test

# Run single test
cargo test <test_name> -- --nocapture

# Migrations
cargo run -- generate migration <name>
```

All of the above are available as `make` targets. Run `make help` for the full list:

```text
  setup         Install system dependencies and configure local build
  setup-check   Check dependencies without installing
  dev           Start dev server (localhost:5150)
  dev-desktop   Start dev server (alias for dev — desktop always included)
  watch         Auto-restart on file changes (requires cargo-watch)
  test          Run all tests
  test-desktop  Run tests (alias for test — desktop always included)
  fmt           Format code
  fmt-check     Check formatting
  lint          Run clippy with strict rules
  qa            Run fmt-check, lint, and test
  migrate-gen   Generate new migration (NAME=create_games)
  migrate-up    Run pending migrations
  build         Build release binary (desktop always included)
  build-desktop Build release binary (alias for build)
  release       Production build
  clean         Remove build artifacts
  reset         Full reset (remove DB + build artifacts)
```

## Task Tracking

You **MUST** create a todo list using `todo_write` at the start of every task that involves code changes. The list **MUST** include verification steps before you mark the task complete.

### Required todo items for code changes

Every todo list for code changes **MUST** include:

- `Run make test` — all tests pass
- `Run make qa` — fmt-check, lint, and tests all pass

These are non-negotiable. Add them when you initialize the list; do not skip or drop them.

### When to skip

Skip the todo list only for trivial requests: single-line fixes, reading files, or answering questions that require no code changes.

## Code Conventions & Patterns

### Error Handling
- Use `loco_rs::Result` / `ModelError` throughout.
- Controllers return `Result<Response>`; use `unauthorized()`, `bad_request()` helpers.
- `?` operator for propagation; never `unwrap()` in production code.

### Async
- All DB operations and handlers are `async`. Use `#[debug_handler]` on controller functions.
- `ActiveModelBehavior` hooks (`before_save`, `after_save`) are async.

### Model Patterns
- Custom lookup methods live on `impl Model { ... }` in `src/models/users.rs`.
- ActiveModel behavior hooks live in `impl ActiveModelBehavior for users::ActiveModel`.
- Use `model::query::condition().eq(...)` for Sea-ORM queries.
- Validators implement `Validate` trait; wire to `impl Validatable for ActiveModel`.

### Auth
- JWT tokens generated via `user.generate_jwt(&secret, expiration)`.
- `Authenticable` trait required for JWT-based auth middleware.
- Magic links and password resets use short-lived tokens stored in DB.

### I18n
- Shared Fluent resources must have underscore-prefixed filenames (e.g., `_shared.ftl`) to prevent ArcLoader from parsing the stem as a language identifier and loading it twice, causing `FluentError::Overriding`.
- Locale-specific files live in `assets/i18n/<locale>/main.ftl`.
- Tera templates use `{{ t("key", args) }}` for translations.

### Views
- Response DTOs implement `serde::Serialize` and are returned via `format::json()`.
- Use `&str` over `&String` in function signatures (clippy `ptr_arg`).

- Naming conventions: use descriptive variable names (e.g., `tray_state`, not `t`; `tray_icon`, not `tr`). There is no cost in describing things clearly.

### Local Environment
- Linux requires GTK/appindicator system dependencies (`libgtk-3`, `libappindicator-gtk3`).
- No libxdo symlink needed — `tray-icon` is compiled with `libxdo` feature disabled.

### Install & Run
- `make install` builds release binary and copies to `~/.local/bin/game-smith` (no reboot needed)
- `make run-release` builds and runs directly from `target/release/`
- Run `./target/release/game-smith start` or `game-smith start` from PATH

### Desktop Integration
- System tray icon, desktop notifications, and browser auto-open are always compiled in (no feature flag).
- Configured via environment variables: `GAME_SMITH_DESKTOP_ENABLED`, `GAME_SMITH_DESKTOP_OPEN_BROWSER`, `GAME_SMITH_DESKTOP_TRAY_ENABLED`, `GAME_SMITH_DESKTOP_TRAY_TOOLTIP`, `GAME_SMITH_PORT`.
- Tray icon runs on a dedicated OS thread; menu events polled via `tray-icon`'s global channel.
- GTK and tray icon are initialized on thread 0 in `src/bin/main.rs` before the tokio runtime starts.
- Linux requires GTK/appindicator system dependencies (`libgtk-3`, `libappindicator-gtk3`).

## Testing & QA

Rust has three test locations:

1. **Inline `#[cfg(test)]` modules** in source files — unit tests, fast, test internal APIs.
2. **`tests/` directory** at crate root — integration tests. Each file is compiled as a separate crate that imports the library through its public API. This is where `cargo test` looks by default.
3. **`benches/` directory** — benchmarks.

### Test Structure
- `tests/models/` — Model integration tests.
- `tests/requests/` — HTTP handler integration tests.
- `tests/requests/prepare_data.rs` — Test data setup helpers.
- `tests/desktop/` — Desktop feature integration tests.
- `tests/tasks/`, `tests/workers/` — Task and worker tests.
- Snapshots stored alongside test files in `tests/*/snapshots/`.

### Test Patterns
```rust
#[tokio::test]
#[serial]
async fn test_name() {
    // 1. Configure insta settings
    configure_insta!();

    // 2. Boot test app (uses config/test.yaml)
    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");

    // 3. Seed fixtures (from src/fixtures/) if needed
    seed::<App>(&boot.app_context)
        .await
        .expect("Failed to seed database");

    // 4. Execute logic
    let res = Model::find_by_email(&boot.app_context.db, "test@example.com").await;

    // 5. Assert with insta snapshots
    assert_debug_snapshot!(res);
}
```

### Key Testing Conventions
- Every test boots the full application via `boot_test::<App>()`.
- Tests use `#[serial]` to avoid database contention.
- `seed::<App>(&ctx)` loads YAML fixtures from `src/fixtures/`.
- `cleanup_user_model()` provides sensitive data filters for snapshots.
- Test config: `config/test.yaml` (SQLite in-memory DB).

## Important Files

| File | Purpose |
|------|---------|
| `src/app.rs` | App hooks — routes, initializers, workers, seed |
| `src/models/users.rs` | User model, auth logic, password hashing |
| `src/controllers/auth.rs` | Authentication routes (register, login, reset, magic link) |
| `src/initializers/view_engine.rs` | Tera + Fluent i18n initialization |
| `config/test.yaml` | Test environment configuration (SQLite, JWT secret) |
| `Cargo.toml` | Dependencies, features, dev-dependencies |
| `migration/src/lib.rs` | Migration registry |
|| `src/desktop/mod.rs` | DesktopManager: tray icon, menu, browser open |
|| `src/desktop/notifications.rs` | Desktop notification helper |
|| `src/bin/main.rs` | DesktopManager initialization |
