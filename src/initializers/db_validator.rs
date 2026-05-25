use async_trait::async_trait;
use loco_rs::{
    app::{AppContext, Initializer},
    environment::Environment,
    Result,
};
use sea_orm::{ConnectionTrait, DbBackend, Statement};

pub struct DbValidator;

#[async_trait]
impl Initializer for DbValidator {
    fn name(&self) -> String {
        "db-connection-validator".to_string()
    }

    async fn before_run(&self, ctx: &AppContext) -> Result<()> {
        if matches!(ctx.environment, Environment::Test) {
            return Ok(());
        }

        let expected = crate::canonical_db_uri();
        let expected_path = expected
            .strip_prefix("sqlite://")
            .unwrap_or(&expected)
            .split('?')
            .next()
            .unwrap_or(&expected);

        let stmt = Statement::from_string(DbBackend::Sqlite, "PRAGMA database_list");
        if let Ok(Some(result)) = ctx.db.query_one(stmt).await {
            if let Ok(file) = result.try_get::<String>("", "file") {
                if !file.is_empty() && file != expected_path {
                    tracing::warn!(
                        actual = %file,
                        expected = %expected_path,
                        "DB connection points to unexpected database"
                    );
                }
            }
        }

        Ok(())
    }
}
