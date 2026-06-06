#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;
pub use sea_orm_migration::seaql_migrations;

mod m20260523_192956_command_runs;
mod m20260524_201932_command_run_title;
mod m20260524_211537_create_game_servers;
mod m20260525_000001_create_steam_credentials;
mod m20260527_225027_add_use_steam_login;
mod m20260606_000001_remove_game_servers_pid;
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260523_192956_command_runs::Migration),
            Box::new(m20260524_201932_command_run_title::Migration),
            Box::new(m20260524_211537_create_game_servers::Migration),
            Box::new(m20260525_000001_create_steam_credentials::Migration),
            Box::new(m20260527_225027_add_use_steam_login::Migration),
            Box::new(m20260606_000001_remove_game_servers_pid::Migration),
            // inject-above (do not remove this comment)
        ]
    }
}
