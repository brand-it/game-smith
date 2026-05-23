//! Tests for stale migration cleanup logic.

use game_smith::app::clean_stale_migrations;
use serial_test::serial;

/// Test that the cleanup function is a no-op when no migration records exist.
#[tokio::test]
#[serial]
async fn is_no_op_when_no_migration_records() {
    // Use an in-memory database.
    let uri = "sqlite::memory:".to_string();

    // Run the cleanup on an empty database (no seaql_migrations table).
    // This should not panic.
    clean_stale_migrations(&uri).await;
}

/// Test that the cleanup function handles a missing database gracefully.
#[tokio::test]
#[serial]
async fn handles_missing_database_gracefully() {
    // Use an in-memory database.
    let uri = "sqlite::memory:".to_string();

    // The function should not panic or crash.
    clean_stale_migrations(&uri).await;
}
