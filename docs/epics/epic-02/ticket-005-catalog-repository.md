# Ticket 005: Domain Models & `CatalogRepository` Port/Adapter

**Epic:** EPIC-02 — Catalog Data & Search Architecture

**Complexity:** Medium

**Depends on:** `ticket-004-catalog-schema`

**Blocks:** `ticket-006-search-spike-and-engine`, `ticket-007-search-ui`, all of Epic 3

---

## Description

Implement the pure domain types (`fwt-domain`) that model `catalog.db`'s content, the `CatalogRepository` trait
(the *port*, defined in `fwt-domain::ports`) that describes how the application layer reads catalog data without
knowing SQL exists, and its concrete SQLite-backed adapter (`fwt-infra::db::catalog_repo`) that implements that
trait against the schema Ticket 004 established — enforcing, at the connection-configuration level, that
`catalog.db` can never be written to by the running application (ADR-1).

This ticket is the direct architectural analogue of Epic 1's `TerminalGuard`/`terminal.rs`: a single, carefully
reviewed seam that every later epic depends on, where getting the read-only guarantee or the pooling strategy
wrong would silently propagate risk (in this case, of accidental catalog corruption or of a future
`FavoritesService` in Epic 4 attempting to write catalog data instead of user data) into code written much later
by a differently-scoped, differently-contexted session.

## Acceptance Criteria

1. `fwt-domain` gains, in a new `widget.rs` (plus small supporting files per TRD Section 7's listing): `Widget`
   (mirroring the `widgets` table's columns, with `related_widget: Option<WidgetId>` rather than a raw
   `related_widget_id: i64`, using a `WidgetId(i64)` newtype — not a bare `i64` — to prevent accidental
   ID-type confusion with, e.g., a future `FavoriteId`), `Property` (with `InputKind` as a proper Rust enum:
   `Enum(Vec<String>)` carrying its parsed options rather than leaving `enum_options` as an unparsed JSON string
   at the domain layer — JSON parsing happens once, at the infra boundary, not repeated by every consumer),
   `Method` (`kind: MethodKind::{Static, Instance}` enum, not a raw string), `CodeSample`, and `DesignSystem`
   (`Material | Cupertino | Base` enum, not a raw string) — all pure data, `Debug + Clone + PartialEq`, zero I/O,
   zero `rusqlite`/`serde` dependency beyond what's already permitted in `fwt-domain`'s `Cargo.toml` (this ticket
   does **not** need to add new dependencies to `fwt-domain` — construction/validation is plain Rust).
2. `Widget::new(...)`-style constructors (or a builder, if field count makes a positional constructor unwieldy)
   enforce basic domain invariants at construction time — e.g., `name` and `summary` cannot be empty strings —
   returning a `Result<Widget, WidgetValidationError>` (`thiserror`) rather than panicking, consistent with Epic
   1 Ticket 004's established pattern of `update()`-adjacent code never panicking on bad-but-plausible input.
3. A `CatalogRepository` trait is defined in `fwt-domain::ports::catalog_repository` (new `ports/` module per TRD
   Section 7) with, at minimum, these methods, each `async fn` (or returning a boxed future, per whatever async-
   trait mechanism the workspace's Rust edition/MSRV supports — native `async fn` in traits if the edition allows,
   `async-trait` crate otherwise; decide and document explicitly) and each returning
   `Result<_, RepositoryError>`:
   - `get_widget_by_id(&self, id: WidgetId) -> Result<Option<Widget>, RepositoryError>`
   - `get_widget_by_name(&self, name: &str) -> Result<Option<Widget>, RepositoryError>`
   - `list_categories(&self) -> Result<Vec<CategorySummary>, RepositoryError>` (category name + widget count,
     sufficient for the wireframe's `"Accessibility · N widgets"` style grid entries)
   - `list_widgets_by_category(&self, category: &str) -> Result<Vec<WidgetSummary>, RepositoryError>`
     (a lighter-weight projection than full `Widget` — id/name/summary only — for list rendering)
   - `get_properties(&self, widget_id: WidgetId) -> Result<Vec<Property>, RepositoryError>`
   - `get_methods(&self, widget_id: WidgetId) -> Result<Vec<Method>, RepositoryError>`
   - `get_code_samples(&self, widget_id: WidgetId) -> Result<Vec<CodeSample>, RepositoryError>`
   - `search_fts(&self, query: &str, limit: usize) -> Result<Vec<WidgetSummary>, RepositoryError>` (the coarse
     FTS5 pass — Ticket 006 consumes this, does not reimplement it)
   - `load_search_corpus(&self) -> Result<Vec<SearchCorpusEntry>, RepositoryError>` (the full name/category/
     summary set for in-memory index construction — Ticket 006 consumes this)
4. `RepositoryError` (`thiserror`, in `fwt-domain::ports`) covers at minimum `NotFound` (used internally by the
   adapter where relevant, though most "not found" cases return `Ok(None)`/`Ok(vec![])` per criterion 3 above —
   `NotFound` is reserved for cases like a required `catalog_meta` key being absent), `QueryFailed` (wraps the
   underlying error, `#[source]`), and `PoolExhausted`/`ConnectionFailed`.
5. `fwt-infra::db::catalog_repo::SqliteCatalogRepository` implements `CatalogRepository` against a connection
   pool constructed with the finalized pooling crate (per Epic-level open decision), opening `catalog.db` with
   **enforced read-only semantics** — concretely: `rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY` (no
   `SQLITE_OPEN_READ_WRITE`, no `SQLITE_OPEN_CREATE`) on every pooled connection, so that even a future
   programming error attempting an `INSERT` through this adapter fails at the SQLite layer, not just by
   convention or code review.
6. Every trait method's SQL is parameterized (`rusqlite`'s `?`/named-parameter binding) — **no string
   interpolation of user-controlled input into SQL** (the search query string, in particular, is user-typed input
   and must never be concatenated into a `MATCH` clause) — verified by a test asserting an FTS5 query containing
   special characters (`"`, `*`, `-`) does not error or behave as SQL injection, only as a (possibly
   zero-result) FTS5 query.
7. `SqliteCatalogRepository::new(db_path: &Path) -> Result<Self, RepositoryError>` runs the embedded migration set
   (sharing the migration definitions module from Ticket 004's `fwt-infra::db::migrations`, **not** duplicating
   them) against the target path in a **separate, momentarily read-write connection used only for the migration
   check**, which is then closed before the read-only pool is constructed — this ordering (migrate first, then
   downgrade to read-only pool) must be explicit and commented, since reversing it would make the app unable to
   apply a future catalog schema update shipped as a new migration.
8. Blocking `rusqlite` calls made from the async `CatalogRepository` methods are wrapped in `tokio::task::
   spawn_blocking` (since `rusqlite` is synchronous and the pool's checkout/query calls would otherwise block a
   Tokio worker thread) — verified by a test analogous to Epic 1 Ticket 003's non-blocking-guarantee test:
   dispatching a repository call alongside a concurrent, fast-completing async task and asserting the fast task
   isn't starved.
9. `WidgetId`, `CategorySummary`, `WidgetSummary`, and `SearchCorpusEntry` are defined in `fwt-domain` (not
   `fwt-infra`) since they are part of the port's public contract, consumed by `fwt-app` — `fwt-infra`'s adapter
   maps SQLite rows into these `fwt-domain` types at the query boundary, never leaking a `rusqlite::Row` or raw
   SQL type upward.
10. All new `fwt-infra` code has integration test coverage against a real temporary SQLite file (`tempfile`),
    seeded via a small, ticket-local fixture (a handful of hand-inserted rows via direct SQL in the test, **not**
    a dependency on Ticket 004's full `xtask seed-catalog` pipeline, keeping this ticket's tests fast and
    independent) exercising every `CatalogRepository` method against both present and absent data.

## Implementation Details & Design Notes

- **Port location and the dependency rule:** `CatalogRepository`'s trait definition lives in `fwt-domain::ports`,
  per TRD Section 2.1's "trait ports defined *in* the domain or an adjacent `ports` module." This is what lets
  `fwt-app`'s future `SearchService`/`CatalogService` (Ticket 006/007) depend only on `fwt-domain`'s trait, never
  on `fwt-infra`'s concrete `SqliteCatalogRepository` — the concrete type is constructed once, in `fwt-cli`'s
  composition root, and injected downward as `Arc<dyn CatalogRepository>` (or a generic `<R: CatalogRepository>`
  parameter, if trait-object dispatch overhead is a concern per NFR-3 — benchmark before deciding; a `dyn` trait
  object's vtable indirection is very unlikely to be the bottleneck relative to the SQLite query itself, so
  default to `Arc<dyn CatalogRepository>` for simplicity unless Ticket 006's benchmarking says otherwise).
- **Async trait mechanism:** if the workspace's Rust edition (2021, per `Cargo.toml`) and MSRV don't cleanly
  support native `async fn` in traits with object-safety (`dyn CatalogRepository`) at this ticket's implementation
  time, use the `async-trait` crate rather than hand-rolling boxed-future signatures — document whichever choice
  is made directly in `ports/catalog_repository.rs`'s module doc comment, since this is exactly the kind of
  "why does this trait look like this" question a future isolated session would otherwise have to re-derive.
- **`spawn_blocking` placement:** wrap at the *adapter* level (inside each `SqliteCatalogRepository` method), not
  at call sites in `fwt-app` — callers of `CatalogRepository` should never need to know or care that the
  underlying implementation happens to be synchronous SQLite; a future in-memory `FakeCatalogRepository` used in
  `fwt-app` unit tests (per TRD Section 10's testing strategy) has no need for `spawn_blocking` at all, and its
  absence there should not be visible to `fwt-app` code.
- **Connection pool sizing:** a read-only workload with no write contention doesn't need a large pool — start with
  a small, documented constant (e.g., 4 connections) sized similarly in spirit to Epic 1 Ticket 003's
  `TOKIO_WORKER_THREADS` reasoning (I/O-bound, not CPU-bound, small footprint per NFR-5), and note in a code
  comment that this is a starting point to revisit if Epic 5's AI-chat retrieval path or Epic 3's detail-view
  navigation reveals real contention under manual QA.
- **`WidgetSummary` vs full `Widget`:** introduced deliberately as a lighter projection type so that
  `list_widgets_by_category` and `search_fts`/FTS-backed queries don't force a full row (including potentially
  large `overview` markdown text) to be materialized and passed around just to render a list item — this directly
  serves NFR-3/NFR-5 for the Search and Catalog tab UIs (Ticket 007), which only need id/name/summary/category
  until a user drills into a specific widget (Epic 3, which then calls `get_widget_by_id` for the full record).
- **Migration-then-downgrade connection sequencing (criterion 7):** structure `SqliteCatalogRepository::new` as
  two clearly separated blocks with an explicit code comment boundary — `{ /* migrate: RW connection, dropped at
  end of block */ }` followed by `{ /* construct RO pool */ }` — so a future edit that tries to "simplify" by
  reusing one connection for both purposes is an obvious, comment-flagged regression against ADR-1, not a subtle
  one.

## Folders / Files Impacted

    crates/fwt-domain/
    └── src/
        ├── lib.rs                          # MODIFIED — expose widget, ports modules
        ├── widget.rs                       # NEW — Widget, Property, Method, CodeSample, DesignSystem,
        │                                   #        WidgetId, InputKind, MethodKind, WidgetValidationError
        └── ports/
            ├── mod.rs                      # NEW
            └── catalog_repository.rs       # NEW — CatalogRepository trait, RepositoryError,
                                              #        CategorySummary, WidgetSummary, SearchCorpusEntry
    crates/fwt-infra/
    ├── Cargo.toml                          # MODIFIED — r2d2/r2d2_sqlite (or deadpool-sqlite)
    └── src/
        ├── lib.rs                          # MODIFIED — expose db module
        └── db/
            ├── mod.rs                      # NEW
            ├── migrations.rs               # MODIFIED (from Ticket 004) — shared embedded migration set
            └── catalog_repo.rs             # NEW — SqliteCatalogRepository

## Testing Plan

- **Domain unit tests (`fwt-domain/src/widget.rs`):** `Widget`/`Property`/`Method` construction with valid and
  invalid inputs (empty `name`, etc.), asserting the correct `WidgetValidationError` variant; `InputKind`'s
  enum-options parsing from a raw JSON string, table-driven (`rstest`) across valid arrays, empty arrays, and
  malformed JSON.
- **Repository integration tests (`fwt-infra`, `tempfile`-backed):** for each `CatalogRepository` method, a
  present-data case and an absent-data case (e.g., `get_widget_by_id` for an id that exists vs. one that
  doesn't, asserting `Ok(Some(_))` vs `Ok(None)` respectively, never `Err` for a simple absence); a dedicated
  test constructing `SqliteCatalogRepository` against a *migrated* temp file and asserting a direct write attempt
  through a manually-opened read-write connection to the same path doesn't interfere with, and a write attempt
  *through the adapter's own pool* fails (criterion 5).
- **SQL-injection-shaped input test:** `search_fts` called with adversarial strings (`"'; DROP TABLE widgets;
  --"`, unbalanced quotes, FTS5 special syntax characters) against a seeded temp DB, asserting no error and no
  data loss (re-querying `widgets` afterward confirms the table is intact).
- **Non-blocking `spawn_blocking` test:** dispatch a `CatalogRepository` call (against a temp DB with an
  artificially large/slow query, e.g. a `sleep()`-equivalent via a large `GROUP BY`/recursive CTE if needed, or
  more simply a mock/instrumented repository for this specific test) concurrently with a fast, independent async
  task; assert the fast task completes without waiting on the slow query, proving the blocking call isn't
  starving the Tokio runtime.
- **Fixture-seeded tests use direct SQL, not `xtask`:** keep this ticket's test fixtures self-contained (a small
  `insert_test_widget(conn, ...)` helper using raw parameterized SQL) rather than depending on Ticket 004's full
  seed pipeline running first, so this ticket's test suite has no cross-ticket runtime coupling.

## Potential Risks / Edge Cases

- **Risk: `SQLITE_OPEN_READ_ONLY` interacting badly with WAL mode.** If a future ticket (or an external process)
  ever writes to `catalog.db` while it's in WAL journal mode, a strictly read-only connection can, in some SQLite
  versions/configurations, fail to see the writer's committed changes without a `PRAGMA read_uncommitted`-style
  adjustment or without the WAL file being checkpointed. Since ADR-1's model is "the file is replaced wholesale on
  update, never written to by the running app," this should not arise in practice — but explicitly document (code
  comment on `SqliteCatalogRepository::new`) that `catalog.db` is expected to use `journal_mode=DELETE` (SQLite's
  simple default), not WAL, precisely to sidestep this class of issue, distinguishing it from `user.db`'s Epic
  4-scoped WAL usage (TRD Section 11's SQLite-write-contention mitigation, which applies to `user.db`, not this
  read-only file).
- **Risk: trait-object dispatch overhead vs. NFR-3.** Flagged above; not expected to be material, but Ticket 006's
  benchmark should include the real `Arc<dyn CatalogRepository>`-based `search_fts` call in its measured path
  (not a bare SQL micro-benchmark bypassing the trait), so this assumption is validated with real numbers rather
  than asserted from first principles.
- **Edge case: `catalog.db` missing entirely at startup** (fresh install, before any seed/download mechanism —
  itself out of this epic's scope, since "download-and-verify a shipped catalog.db" is implicitly a Future
  Feature per TRD Section 4.1's phrasing "shipped with *or downloaded by* the app"). For this epic, document as a
  known, explicit limitation: `SqliteCatalogRepository::new` against a missing path currently returns a
  `RepositoryError` (from the failed migration-connection step) rather than any graceful "no catalog installed"
  UI state — flag this as a fast-follow note for whichever future epic finalizes the shipped-asset installation
  story, rather than solving distribution in this ticket.
- **Edge case: concurrent pool exhaustion under Epic 5's future retrieval-heavy AI chat load.** Not a realistic
  concern at this epic's read-mostly, single-user-typing-searches scale, but leave the pool size as an easily
  adjustable named constant (not scattered magic numbers) so a future profiling-driven change is a one-line diff.

---