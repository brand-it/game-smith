use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(
            m,
            "game_templates",
            &[
                ("id", ColType::PkAuto),
                ("name", ColType::String),
                ("description", ColType::TextNull),
                ("app_id", ColType::Integer),
                ("server_mod", ColType::StringNull),
                ("beta_branch", ColType::StringNull),
                ("boot_script", ColType::TextNull),
                ("use_steam_login", ColType::Boolean),
                ("auto_start", ColType::Boolean),
                ("auto_restart", ColType::Boolean),
                ("auto_update", ColType::Boolean),
                ("update_on_start", ColType::Boolean),
                ("restart_schedule", ColType::StringNull),
            ],
            &[],
        )
        .await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "game_templates").await
    }
}
