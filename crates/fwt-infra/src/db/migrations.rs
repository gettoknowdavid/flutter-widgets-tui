//! Embedded catalog.db migrations.
//!
//! The migration SQL itself lives at `migrations/catalog/V1__initial_schema.sql`
//! (workspace root) as the single, version-controlled source of truth —
//! this module and `xtask/src/migrations.rs` each embed that same file
//! via `include_str!` at compile time. Do NOT duplicate the SQL body in
//! either crate; if you need a second migration, add a new
//! `V2__....sql` file and a corresponding `M::up(include_str!(...))`
//! entry in both places.

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

/// The full, ordered migration set. `rusqlite_migration::Migrations::
/// to_latest` is idempotent by construction — it reads SQLite's
/// `user_version` pragma and applies only migrations beyond the
/// connection's current version, so calling this twice against the
/// same file is a documented no-op on the second call.
pub static CATALOG_MIGRATIONS: std::sync::LazyLock<Migrations<'static>> =
    std::sync::LazyLock::new(|| {
        Migrations::new(vec![M::up(include_str!(
            "../../../../migrations/catalog/V1__initial_schema.sql"
        ))])
    });

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("failed to apply catalog.db migrations")]
    ApplyFailed(#[from] rusqlite_migration::Error),

    #[error("failed to open catalog.db connection")]
    ConnectionFailed(#[from] rusqlite::Error),
}

/// Applies every pending migration to `conn`, idempotently. Safe to
/// call on every startup — a fully-migrated connection is a no-op.
///
/// Callers are responsible for `PRAGMA foreign_keys = ON` on the
/// connection before calling this (this function does not set it
/// itself, since foreign-key enforcement is a connection-level
/// concern the caller may want to control independently of
/// migration application).
pub fn run_migrations(conn: &mut Connection) -> Result<(), MigrationError> {
    CATALOG_MIGRATIONS.to_latest(conn)?;
    Ok(())
}

/// Convenience constructor: opens `path`, enables foreign keys, and
/// runs migrations — the common case for both `xtask` (against a
/// fresh temp path) and any future `fwt-infra` read path that needs
/// a migration-checked connection.
pub fn open_and_migrate(path: &std::path::Path) -> Result<Connection, MigrationError> {
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    run_migrations(&mut conn)?;
    Ok(conn)
}
