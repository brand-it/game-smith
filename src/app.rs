use async_trait::async_trait;
use loco_rs::{
    app::{AppContext, Hooks, Initializer},
    bgworker::{BackgroundWorker, Queue},
    boot::{create_app, BootResult, StartMode},
    config::Config,
    controller::AppRoutes,
    environment::Environment,
    task::Tasks,
    Result,
};
use migration::{seaql_migrations, Migrator, MigratorTrait};
use sea_orm::{ColumnTrait, Database, EntityTrait, QueryFilter};

#[allow(unused_imports)]
use crate::{initializers, tasks, workers::downloader::DownloadWorker};

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

        let mut config = Config::new(env)?;
        let dirs = super::AppDirs::new(super::resolve_data_home());

        // Override database URI with computed XDG path.
        config.database.uri = dirs.db_uri();

        // Override log directory with computed XDG path.
        if let Some(file_appender) = &mut config.logger.file_appender {
            file_appender.dir = Some(dirs.logs_dir.to_string_lossy().to_string());
        }

        clean_stale_migrations(&config.database.uri).await;

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
        Ok(vec![Box::new(
            initializers::view_engine::ViewEngineInitializer,
        )])
    }

    fn routes(_ctx: &AppContext) -> AppRoutes {
        AppRoutes::with_default_routes()
    }
    async fn connect_workers(ctx: &AppContext, queue: &Queue) -> Result<()> {
        queue.register(DownloadWorker::build(ctx)).await?;
        Ok(())
    }

    #[allow(unused_variables)]
    fn register_tasks(tasks: &mut Tasks) {
        // tasks-inject (do not remove)
    }
    async fn truncate(_ctx: &AppContext) -> Result<()> {
        Ok(())
    }
    async fn seed(_ctx: &AppContext, _base: &std::path::Path) -> Result<()> {
        Ok(())
    }
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
            let _ = sea_orm::Delete::many(seaql_migrations::Entity)
                .filter(seaql_migrations::Column::Version.eq(&record.version))
                .exec(&db)
                .await;
            tracing::info!(migration = %record.version, "removed stale migration record");
        }
    }
}
