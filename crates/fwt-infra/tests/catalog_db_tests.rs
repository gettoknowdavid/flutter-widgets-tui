//! Integration tests for catalog.db's schema, migrations, and FTS5
//! sync behavior. Every test here runs against a `tempfile`-created
//! temporary SQLite path — none touch a real OS data-directory file.

use fwt_infra::db::migrations::{open_and_migrate, run_migrations};
use rusqlite::{params, Connection};
use tempfile::NamedTempFile;

fn temp_db_path() -> NamedTempFile {
    NamedTempFile::new().expect("failed to create temp file")
}

// =============================================================
// Migration idempotency
// =============================================================

#[test]
fn migrations_apply_cleanly_to_a_fresh_database() {
    let tmp = temp_db_path();
    let conn = open_and_migrate(tmp.path()).expect("first migration run should succeed");

    // Assert the widgets table exists with the expected columns via
    // PRAGMA table_info, per the ticket's testing plan.
    let mut stmt = conn.prepare("PRAGMA table_info(widgets)").unwrap();
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    for expected in [
        "id",
        "name",
        "top_level",
        "design_system",
        "categories",
        "summary",
        "overview_markdown",
        "related_widget_id",
    ] {
        assert!(
            columns.contains(&expected.to_string()),
            "widgets table missing expected column `{expected}`; found {columns:?}"
        );
    }
}

#[test]
fn migrations_are_idempotent_when_applied_twice() {
    let tmp = temp_db_path();

    let mut conn = Connection::open(tmp.path()).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();

    run_migrations(&mut conn).expect("first run");
    // Second application against the SAME connection/file must be a
    // documented no-op — rusqlite_migration checks `user_version`
    // and skips already-applied steps.
    run_migrations(&mut conn).expect("second run must not error");

    let table_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='widgets'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        table_count, 1,
        "widgets table must not be duplicated/recreated"
    );
}

// =============================================================
// FTS5 sync verification
// =============================================================

fn insert_test_widget(conn: &Connection, name: &str, summary: &str) -> i64 {
    conn.execute(
        "INSERT INTO widgets (name, top_level, design_system, categories, summary, overview_markdown)
         VALUES (?1, 'Base Widgets', 'base', '[\"Scrolling\"]', ?2, '')",
        params![name, summary],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn fts_match_count(conn: &Connection, query: &str) -> i64 {
    conn.query_row(
        "SELECT count(*) FROM widgets_fts WHERE widgets_fts MATCH ?1",
        params![query],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn insert_is_reflected_in_fts_index() {
    let tmp = temp_db_path();
    let conn = open_and_migrate(tmp.path()).unwrap();

    insert_test_widget(&conn, "ListView", "A scrollable list of widgets.");

    assert_eq!(fts_match_count(&conn, "ListView"), 1);
    assert_eq!(fts_match_count(&conn, "scrollable"), 1);
    assert_eq!(fts_match_count(&conn, "NoSuchWidget"), 0);
}

#[test]
fn update_is_reflected_in_fts_index() {
    let tmp = temp_db_path();
    let conn = open_and_migrate(tmp.path()).unwrap();

    let id = insert_test_widget(&conn, "ListView", "Original summary text.");
    assert_eq!(fts_match_count(&conn, "Original"), 1);
    assert_eq!(fts_match_count(&conn, "Updated"), 0);

    conn.execute(
        "UPDATE widgets SET summary = ?1 WHERE id = ?2",
        params!["Updated summary text.", id],
    )
    .unwrap();

    // The OLD text must no longer match, and the NEW text must —
    // this is the exact delete-then-insert pairing the widgets_au
    // trigger exists to guarantee.
    assert_eq!(fts_match_count(&conn, "Original"), 0);
    assert_eq!(fts_match_count(&conn, "Updated"), 1);
}

#[test]
fn delete_removes_the_widget_from_the_fts_index() {
    let tmp = temp_db_path();
    let conn = open_and_migrate(tmp.path()).unwrap();

    let id = insert_test_widget(&conn, "ListView", "A scrollable list.");
    assert_eq!(fts_match_count(&conn, "ListView"), 1);

    conn.execute("DELETE FROM widgets WHERE id = ?1", params![id])
        .unwrap();

    assert_eq!(
        fts_match_count(&conn, "ListView"),
        0,
        "deleted widget must not remain searchable via widgets_fts"
    );

    // Also confirm the FTS index's internal structure is actually
    // healthy after the delete, not just "no longer matching by
    // accident" — the fts5vocab-free way to sanity-check this is an
    // 'integrity-check' command, which errors if the b-tree is
    // desynced from `widgets`.
    conn.execute(
        "INSERT INTO widgets_fts(widgets_fts) VALUES ('integrity-check')",
        [],
    )
    .expect("FTS5 integrity-check must pass after delete-trigger cleanup");
}

// =============================================================
// Foreign key enforcement
// =============================================================

#[test]
fn foreign_keys_are_enforced_on_code_samples() {
    let tmp = temp_db_path();
    let conn = open_and_migrate(tmp.path()).unwrap();

    let result = conn.execute(
        "INSERT INTO code_samples (widget_id, label, kind, code)
         VALUES (999999, 'Bad sample', 'snippet', 'void main() {}')",
        [],
    );

    assert!(
        result.is_err(),
        "inserting a code_samples row with a non-existent widget_id must fail \
         with PRAGMA foreign_keys = ON"
    );
}

#[test]
fn foreign_keys_are_enforced_on_enum_values() {
    let tmp = temp_db_path();
    let conn = open_and_migrate(tmp.path()).unwrap();

    let result = conn.execute(
        "INSERT INTO enum_values (enum_id, name, documentation) VALUES (999999, 'x', '')",
        [],
    );
    assert!(result.is_err());
}

#[test]
fn cascade_delete_removes_child_rows() {
    let tmp = temp_db_path();
    let conn = open_and_migrate(tmp.path()).unwrap();

    let widget_id = insert_test_widget(&conn, "ListView", "A scrollable list.");
    conn.execute(
        "INSERT INTO properties (widget_id, name, type) VALUES (?1, 'scrollDirection', 'Axis')",
        params![widget_id],
    )
    .unwrap();

    conn.execute("DELETE FROM widgets WHERE id = ?1", params![widget_id])
        .unwrap();

    let remaining: i64 = conn
        .query_row(
            "SELECT count(*) FROM properties WHERE widget_id = ?1",
            params![widget_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        remaining, 0,
        "ON DELETE CASCADE must remove orphaned properties"
    );
}

// =============================================================
// catalog_meta round-trip
// =============================================================

#[test]
fn catalog_meta_can_be_written_and_read_back() {
    let tmp = temp_db_path();
    let conn = open_and_migrate(tmp.path()).unwrap();

    conn.execute(
        "INSERT INTO catalog_meta (key, value) VALUES ('schema_version', '1')",
        [],
    )
    .unwrap();

    let value: String = conn
        .query_row(
            "SELECT value FROM catalog_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(value, "1");
}
