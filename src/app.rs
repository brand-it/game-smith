use async_trait::async_trait;
use axum::{response::Redirect, routing::get};
use loco_rs::{
    app::{AppContext, Hooks, Initializer},
    bgworker::{BackgroundWorker, Queue},
    boot::{create_app, BootResult, StartMode},
    config::Config,
    controller::{AppRoutes, Routes},
    environment::Environment,
    task::Tasks,
    Result,
};
use migration::{seaql_migrations, Migrator, MigratorTrait};
use sea_orm::{ColumnTrait, Database, EntityTrait, QueryFilter};

#[allow(unused_imports)]
use crate::{
    controllers, initializers, tasks,
    workers::{
        command_exec::CommandExecWorker, log_cleanup::LogCleanupWorker,
        steamcmd_install::SteamCmdInstallWorker,
    },
};

pub struct App;
#[async_trait]
impl Hooks for App {
    fn app_name() -> &'static str {
        env!("CARGO_CRATE_NAME")
    }

    fn app_version() -> String {
        format!(
            "{} ({})",
            env!("CARGO_PKG_VERSION"),
            option_env!("BUILD_SHA")
                .or(option_env!("GITHUB_SHA"))
                .unwrap_or("dev")
        )
    }

    async fn load_config(env: &Environment) -> Result<Config> {
        // Tests use their own config with hardcoded values.
        if matches!(env, Environment::Test) {
            return Config::new(env);
        }

        let dirs = super::AppDirs::new(super::resolve_data_home());
        let logs_dir = dirs.logs_dir.to_string_lossy().to_string();

        let config = Config {
            logger: loco_rs::config::Logger {
                enable: true,
                pretty_backtrace: false,
                level: loco_rs::logger::LogLevel::Info,
                file_appender: Some(loco_rs::config::LoggerFileAppender {
                    enable: true,
                    non_blocking: true,
                    level: loco_rs::logger::LogLevel::Info,
                    format: loco_rs::logger::Format::Compact,
                    rotation: loco_rs::logger::Rotation::Daily,
                    dir: Some(logs_dir),
                    filename_prefix: Some("game-smith".to_string()),
                    filename_suffix: Some("log".to_string()),
                    max_log_files: 7,
                }),
                ..Default::default()
            },
            server: loco_rs::config::Server {
                port: 5150,
                binding: "127.0.0.1".to_string(),
                host: "http://127.0.0.1".to_string(),
                ident: None,
                middlewares: loco_rs::controller::middleware::Config::default(),
            },
            database: loco_rs::config::Database {
                uri: super::canonical_db_uri(),
                enable_logging: false,
                min_connections: 2,
                max_connections: 5,
                connect_timeout: 5_000,
                idle_timeout: 600_000,
                acquire_timeout: Some(5_000),
                auto_migrate: true,
                dangerously_truncate: false,
                dangerously_recreate: false,
                run_on_start: None,
            },
            cache: loco_rs::config::CacheConfig::default(),
            queue: None,
            auth: None,
            workers: loco_rs::config::Workers {
                mode: loco_rs::config::WorkerMode::BackgroundAsync,
            },
            mailer: None,
            initializers: None,
            settings: None,
            scheduler: None,
        };

        Ok(config)
    }

    async fn boot(
        mode: StartMode,
        environment: &Environment,
        config: Config,
    ) -> Result<BootResult> {
        create_app::<Self, Migrator>(mode, environment, config).await
    }

    async fn initializers(_ctx: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
        Ok(vec![
            Box::new(initializers::secret_key::SecretKeyInitializer),
            Box::new(initializers::db_validator::DbValidator),
            Box::new(initializers::steamcmd::SteamCmdInstaller),
            Box::new(initializers::embedded_i18n::EmbeddedI18n),
            Box::new(initializers::embedded_static::EmbeddedStatic),
            Box::new(initializers::command_log_socket::CommandLogInitializer),
        ])
    }

    fn routes(_ctx: &AppContext) -> AppRoutes {
        AppRoutes::with_default_routes()
            .add_route(controllers::commands::routes())
            .add_route(controllers::steamcmd::routes())
            .add_route(controllers::game_servers::routes())
            .add_route(controllers::steam_config::routes())
            .add_route(controllers::game_templates::routes())
            .add_route(controllers::autostart::routes())
            .add_route(controllers::shutdown::routes())
            .add_route(controllers::shutdown::ping_route())
            .add_route(Routes::new().add("/", get(redirect_to_servers)))
    }
    async fn connect_workers(ctx: &AppContext, queue: &Queue) -> Result<()> {
        queue.register(CommandExecWorker::build(ctx)).await?;
        queue.register(LogCleanupWorker::build(ctx)).await?;
        queue.register(SteamCmdInstallWorker::build(ctx)).await?;

        Ok(())
    }

    fn register_tasks(tasks: &mut Tasks) {
        tasks.register(tasks::pid_liveness::PidLiveness);
        // tasks-inject (do not remove)
    }
    async fn truncate(_ctx: &AppContext) -> Result<()> {
        Ok(())
    }
    async fn seed(_ctx: &AppContext, _base: &std::path::Path) -> Result<()> {
        Ok(())
    }
}

#[axum::debug_handler]
async fn redirect_to_servers() -> Redirect {
    Redirect::to("/servers")
}

/// Remove `seaql_migrations` records for migrations that no longer exist in the
/// codebase. Prevents boot failure when a previously-applied migration is deleted.
pub async fn clean_stale_migrations(uri: &str) {
    let db = match Database::connect(uri).await {
        Ok(db) => db,
        Err(e) => {
            tracing::debug!(error = %e, "skipping stale migration cleanup: cannot connect");
            return;
        }
    };

    let available: Vec<String> = Migrator::migrations()
        .iter()
        .map(|m| m.name().to_owned())
        .collect();

    let applied = match seaql_migrations::Entity::find().all(&db).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::debug!(error = %e, "skipping stale migration cleanup: cannot read migration table");
            return;
        }
    };

    for record in applied {
        if !available.iter().any(|s| s == &record.version) {
            let version = &record.version;
            let res = sea_orm::Delete::many(seaql_migrations::Entity)
                .filter(seaql_migrations::Column::Version.eq(version))
                .exec(&db)
                .await;
            crate::log_result(
                res,
                "removed stale migration record",
                "failed to remove stale migration record",
            );
        }
    }
}
