use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(
            m,
            "command_runs",
            &[
                ("id", ColType::PkAuto),
                ("command", ColType::String),
                ("args", ColType::Json),
                ("working_dir", ColType::StringNull),
                ("log_path", ColType::StringNull),
                ("env", ColType::JsonNull),
                ("status", ColType::String),
                ("exit_code", ColType::IntegerNull),
                ("started_at", ColType::DateTime),
                ("completed_at", ColType::DateTimeNull),
                ("server_id", ColType::BigIntegerNull),
                ("log_removed", ColType::Boolean),
                ("pid", ColType::BigIntegerNull),
            ],
            &[],
        )
        .await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "command_runs").await
    }
}
