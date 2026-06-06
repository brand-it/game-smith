use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        // SQLite has no IF EXISTS for DROP COLUMN — check manually.
        let exists = Statement::from_string(
            m.get_database_backend(),
            r"SELECT COUNT(*) FROM pragma_table_info('game_servers') WHERE name = 'pid'"
                .to_string(),
        );
        let row = m.get_connection().query_one(exists).await?;
        let count = match row {
            Some(r) => r.try_get("", "COUNT(*)")?,
            None => 0i64,
        };

        if count > 0 {
            let stmt = Statement::from_string(
                m.get_database_backend(),
                "ALTER TABLE game_servers DROP COLUMN pid".to_string(),
            );
            m.get_connection().execute(stmt).await?;
        }

        Ok(())
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        let stmt = Statement::from_string(
            m.get_database_backend(),
            "ALTER TABLE game_servers ADD COLUMN pid BIGINT".to_string(),
        );
        m.get_connection().execute(stmt).await?;
        Ok(())
    }
}
