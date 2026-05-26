use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(
            m,
            "steam_credentials",
            &[
                ("id", ColType::PkAuto),
                ("username", ColType::String),
                ("enc_version", ColType::Integer),
                ("nonce", ColType::Binary),
                ("ciphertext", ColType::Binary),
            ],
            &[],
        )
        .await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "steam_credentials").await
    }
}
