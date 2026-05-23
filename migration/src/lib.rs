#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;
pub use sea_orm_migration::seaql_migrations;

mod m20260523_192956_command_runs;
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260523_192956_command_runs::Migration),
            // inject-above (do not remove this comment)
        ]
    }
}
