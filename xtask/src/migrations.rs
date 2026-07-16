//! Mirrors `fwt-infra/src/db/migrations.rs` — see that file's module
//! doc for why the SQL is embedded twice but must never be *written*
//! twice. Keep both `include_str!` paths pointed at the exact same
//! `migrations/catalog/*.sql` files.

use rusqlite_migration::{Migrations, M};

pub static CATALOG_MIGRATIONS: std::sync::LazyLock<Migrations<'static>> =
    std::sync::LazyLock::new(|| {
        Migrations::new(vec![M::up(include_str!(
            "../../migrations/catalog/V1__initial_schema.sql"
        ))])
    });
