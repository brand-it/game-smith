use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        // Add unique index on game_servers.name
        let stmt = Statement::from_string(
            m.get_database_backend(),
            "CREATE UNIQUE INDEX idx_game_servers_name ON game_servers (name)".to_string(),
        );
        m.get_connection().execute(stmt).await?;

        // Add unique index on game_servers.install_dir
        let stmt = Statement::from_string(
            m.get_database_backend(),
            "CREATE UNIQUE INDEX idx_game_servers_install_dir ON game_servers (install_dir)"
                .to_string(),
        );
        m.get_connection().execute(stmt).await?;

        // Add unique index on game_templates.name
        let stmt = Statement::from_string(
            m.get_database_backend(),
            "CREATE UNIQUE INDEX idx_game_templates_name ON game_templates (name)".to_string(),
        );
        m.get_connection().execute(stmt).await?;

        Ok(())
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        // Drop unique index on game_templates.name
        let stmt = Statement::from_string(
            m.get_database_backend(),
            "DROP INDEX IF EXISTS idx_game_templates_name".to_string(),
        );
        m.get_connection().execute(stmt).await?;

        // Drop unique index on game_servers.install_dir
        let stmt = Statement::from_string(
            m.get_database_backend(),
            "DROP INDEX IF EXISTS idx_game_servers_install_dir".to_string(),
        );
        m.get_connection().execute(stmt).await?;

        // Drop unique index on game_servers.name
        let stmt = Statement::from_string(
            m.get_database_backend(),
            "DROP INDEX IF EXISTS idx_game_servers_name".to_string(),
        );
        m.get_connection().execute(stmt).await?;

        Ok(())
    }
}
