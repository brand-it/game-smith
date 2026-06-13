use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        let stmt = Statement::from_string(
            m.get_database_backend(),
            "ALTER TABLE game_servers ADD COLUMN template_id INTEGER REFERENCES game_templates(id)"
                .to_string(),
        );
        m.get_connection().execute(stmt).await?;
        Ok(())
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        let stmt = Statement::from_string(
            m.get_database_backend(),
            "ALTER TABLE game_servers DROP COLUMN template_id".to_string(),
        );
        m.get_connection().execute(stmt).await?;
        Ok(())
    }
}
