# GameSmith

Cross-platform game server management tool written in Rust. Inspired by [WindowsGSM](https://github.com/WindowsGSM/Windows-GSM), but designed to be modular, extensible, and platform-agnostic.

Manage dedicated game servers — install, start, stop, monitor, update, back up — from a web interface, REST API, or Discord. No RDP or VNC required.

## Features

- **Plugin-based game support** — onboard any game as a plugin via a GitHub repository URL. Plugins are isolated, versioned, and upgradable independently.
- **Server lifecycle management** — install, start, stop, restart, delete, and import existing server instances.
- **Process monitoring & crash recovery** — detect abnormal exits, auto-restart with configurable retries and backoff.
- **System resource monitoring** — track per-process memory usage with tiered alerts (warn → alert) and user-defined automated actions.
- **Cron-like job scheduling** — automate restarts, updates, and other recurring tasks with cron syntax.
- **Backup & restore** — manual and scheduled backups with configurable retention, supporting local disk and S3.
- **SteamCMD integration** — install and update dedicated game servers through Steam.
- **Discord integration** — manage servers via commands, relay chat, and receive crash/status alerts.
- **Notifications** — configurable alerts across Discord, email, and extensible third-party channels.
- **Real-time dashboard** — live server statuses, historical uptime, crash/restart events, and key metrics.
- **Multi-user & RBAC** — role-based access control (admin, operator, guest), session management, and audit logging.
- **Documented REST API** — interactive OpenAPI/Swagger UI for programmatic access and integrations.

## Architecture

GameSmith is built with Rust and the [Loco](https://loco.rs) framework:

| Component | Technology |
|---|---|
| Framework | [Loco](https://loco.rs) |
| Web | [Axum](https://github.com/tokio-rs/axum) |
| ORM / Database | [Sea-ORM](https://www.sea-ql.org/SeaORM/) (SQLite, PostgreSQL) |
| Templating | [Tera](https://keats.github.io/tera/) + [Fluent](https://projectfluent.org/) (i18n) |
| Async runtime | [Tokio](https://tokio.rs/) |

## Project Structure

```
src/
  controllers/     # HTTP route handlers
  models/          # domain entities and ORM mappings
  mailers/         # email templates and delivery
  tasks/           # background jobs and scheduled work
  views/           # response templates
  workers/         # async background workers
  initializers/    # app setup (view engine, middleware, etc.)
  app.rs           # application configuration and routing
  lib.rs           # library root
  bin/main.rs      # CLI entry point

tests/
  requests/        # integration tests for HTTP endpoints
  models/          # model unit tests with snapshots
  tasks/           # task tests
  workers/         # worker tests

config/            # environment-specific configuration (development.yaml, test.yaml)
migration/         # database migrations (Sea-ORM)
assets/
  views/           # template files
  i18n/            # localization resources
  static/          # static assets
```

## Quick Start

```sh
make dev
```

Boots the server, runs pending migrations, and starts listening on `http://localhost:5150`. No external CLI tool required.

```sh
# Desktop mode (tray icon + auto-open browser)
make dev-desktop

# Production (SQLite + background workers)
make release
./target/release/game-smith start
```

## Installation

### Linux (`.deb` — Debian, Ubuntu, and derivatives)

Download the latest `.deb` from the [releases page](https://github.com/brand-it/game-smith/releases) and install it:

```sh
sudo dpkg -i game-smith_*.deb
sudo apt-get install -f   # resolves any missing dependencies
```

This installs:
- `/usr/bin/game-smith` — the server binary
- `/usr/share/applications/game-smith.desktop` — app launcher entry
- `/usr/share/icons/hicolor/*/apps/game-smith.png` — application icons

After installation the **Game Smith** icon appears in your application launcher under Utilities. Clicking it starts the server; a tray icon appears in your system tray with options to open the dashboard or quit.

### Linux (Fedora / Bazzite / rpm-ostree)

Install the `.rpm` package. The change takes effect after a reboot (or immediately with `--apply-live`):

```sh
rpm-ostree install game-smith-0.3.0-1.x86_64.rpm
# Optional: apply without rebooting
rpm-ostree install --apply-live game-smith-0.3.0-1.x86_64.rpm
```

This installs:
- `/usr/bin/game-smith` — the server binary
- `/usr/share/applications/game-smith.desktop` — app launcher entry
- `/usr/share/icons/hicolor/*/apps/game-smith.png` — application icons

### Linux (AppImage — any distro, no install)

The `.AppImage` is self-contained. Download it, mark it executable, and run it:

```sh
chmod +x game-smith_0.3.0_x86_64.AppImage
./game-smith_0.3.0_x86_64.AppImage start
```

To integrate with your desktop launcher:
```sh
mkdir -p ~/Applications
mv game-smith_0.3.0_x86_64.AppImage ~/Applications/game-smith.AppImage
# The tray icon and desktop entry are handled by the app itself
```

### Uninstalling

**Debian/Ubuntu:**
```sh
sudo dpkg -r game-smith
```

**Fedora / Bazzite (rpm-ostree):**
```sh
rpm-ostree uninstall game-smith
```

**AppImage:**
```sh
rm ~/Applications/game-smith.AppImage
```
## Development

### Setup

Run the setup script to check and install missing system dependencies:

```sh
make setup
```

Detects your OS/distro and installs GTK3, libappindicator, and xdotool (required for the `desktop` feature). Creates `.cargo/config.local.toml` with the necessary library paths.

On unsupported distributions, you'll need to install these manually.

> Local build configuration goes in `.cargo/config.local.toml` (gitignored).
> Never commit machine-specific paths to `.cargo/config.toml`.

### Running tests

```sh
make test
```

### Quality gates

```sh
make qa    # runs fmt-check, lint, and test
```

### Available targets

Run `make help` for a full list of available targets including migrations, builds, and cleanup.

### Configuration

Environment-specific configuration lives under `config/`. Key sections include database connection, server port, mailer, and authentication settings. Copy `config/development.yaml` and adjust for your environment.

## Contributing

Open issues and feature proposals are tracked on [GitHub](https://github.com/brand-it/game-smith/issues). See the [issues](https://github.com/brand-it/game-smith/issues) tab for the current roadmap.

## License

See [LICENSE](LICENSE).
