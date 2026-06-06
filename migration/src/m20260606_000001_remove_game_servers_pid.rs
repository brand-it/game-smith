use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        remove_column(manager, "game_servers", "pid").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_column(manager, "game_servers", "pid", ColType::BigIntegerNull).await
    }
}
