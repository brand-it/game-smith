use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(
            m,
            "game_servers",
            &[
                ("id", ColType::PkAuto),
                ("app_id", ColType::Integer),
                ("name", ColType::String),
                ("install_dir", ColType::String),
                ("platform", ColType::String),
                ("status", ColType::String),
                ("pid", ColType::BigIntegerNull),
                ("boot_script", ColType::TextNull),
                ("auto_start", ColType::Boolean),
                ("auto_restart", ColType::Boolean),
                ("auto_update", ColType::Boolean),
                ("update_on_start", ColType::Boolean),
                ("restart_schedule", ColType::StringNull),
                ("last_error", ColType::TextNull),
                ("server_mod", ColType::StringNull),
                ("beta_branch", ColType::StringNull),
            ],
            &[],
        )
        .await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "game_servers").await
    }
}
