# Ticket 004: `catalog.db` Schema, Embedded Migrations & Seed Pipeline

**Epic:** EPIC-02 — Catalog Data & Search Architecture

**Complexity:** Medium

**Depends on:** Epic 1 (all tickets — specifically the workspace boundary enforcement from Ticket 001)

**Blocks:** `ticket-005-catalog-repository`, `ticket-006-search-spike-and-engine`

---

## Description

Implement the `catalog.db` SQLite schema exactly as specified in TRD Section 4.2, as a set of versioned, embedded
migrations, plus the minimal build-time tooling needed to seed a real `catalog.db` file from human-authored source
data in `assets/catalog_seed/`. This ticket produces **no repository trait, no domain types beyond what's needed
to validate the seed data at build time, and no search logic** — it produces a correct, migrated, populated
SQLite file and the pipeline that builds it, which Ticket 005 then reads through a proper port/adapter.

This is deliberately sequenced first because every later ticket in this epic (and, transitively, Epic 3's detail
view and Epic 5's AI-chat grounding) depends on `catalog.db`'s schema being final and correct; getting the FTS5
virtual table's sync-trigger setup or a foreign key wrong here would otherwise surface as a confusing bug two or
three tickets downstream.

## Acceptance Criteria

1. A `migrations/catalog/` directory contains versioned SQL migration files (naming convention:
   `V{n}__{description}.sql`, e.g. `V1__initial_schema.sql`) implementing, verbatim per TRD Section 4.2: the
   `widgets` table (with `id`, `name` UNIQUE NOT NULL, `category`, `design_system` DEFAULT `'base'`, `summary`,
   `overview`, `use_when`, `avoid_when`, `related_widget_id` self-referencing FK, `flutter_stable_since`,
   `flutter_channel` DEFAULT `'stable'`, `created_at`), its two indexes (`idx_widgets_category`,
   `idx_widgets_design_system`), the `widgets_fts` FTS5 virtual table (`name`, `category`, `summary`,
   `content='widgets'`, `content_rowid='id'`), `code_samples`, `properties`, `methods`, and `catalog_meta`.
2. The FTS5 virtual table is kept in sync with `widgets` via SQLite triggers (`AFTER INSERT`, `AFTER UPDATE`,
   `AFTER DELETE` on `widgets`) — even though `catalog.db` is read-only at the *application* level (ADR-1), the
   *seeding* process writes through normal INSERT statements and must not produce a stale/desynced FTS index; this
   is exactly the kind of subtlety a migration-only, trigger-less implementation would silently get wrong.
3. `PRAGMA foreign_keys = ON` is set for every connection that applies migrations or seeds data (per TRD Section
   4.1), and at least one migration-time integration test verifies a foreign-key violation (e.g., a
   `code_samples` row referencing a non-existent `widget_id`) is rejected, not silently accepted.
4. Migrations are embedded into the binary at compile time (via `rusqlite_migration`'s `include_dir!`-style
   embedding or equivalent — final crate choice per Epic-level "Open decision carried into Ticket 004") and
   applied automatically against `catalog.db` on first access, idempotently — running the migration set twice
   against the same file is a no-op the second time, verified by an integration test.
5. `assets/catalog_seed/` contains structured source data (recommend: one TOML or JSON file per widget, e.g.
   `assets/catalog_seed/widgets/list_view.toml`, rather than one giant file — this keeps future community/PR-based
   catalog contributions reviewable as small, isolated diffs, anticipating the TRD Section 11 "community
   contribution tooling" post-MVP stretch without building it now) for a curated MVP subset of **at least 100
   widgets**, spanning `design_system` values `cupertino`, `material`, and `base`, and covering **at least 6** of
   the 12 categories listed in the reference wireframe's `catGrid` script (`Accessibility`, `Animation and
   motion`, `Assets, images, and icons`, `Async`, `Basics`, `Input`, `Interaction models`, `Layout`, `Painting and
   effects`, `Scrolling`, `Styling`, `Text`), including, verbatim, a `ListView` entry under `Scrolling` matching
   the wireframe's example content (summary text, `use_when`/`avoid_when` guidance referencing `GridView`, and the
   `ListView.builder`/`ListView.separated` methods) so manual QA can cross-check the real app against the
   wireframe directly.
6. A `cargo xtask seed-catalog` subcommand (extending the existing `xtask` dev-tooling crate from Epic 1, per its
   own doc comment's forward-looking note: *"expect more subcommands here... a future `cargo xtask seed-catalog`"*)
   reads `assets/catalog_seed/`, validates it (schema-shape checks: required fields present, `enum_options` is
   valid JSON when `input_kind='enum'`, `related_widget_id` references resolve to another seed entry by name),
   applies migrations to a fresh SQLite file, and inserts the validated data — failing loudly with a
   file-and-field-specific error message on any validation failure, never silently skipping a malformed entry.
7. `catalog_meta` is populated by the seeding process with at minimum `('schema_version', '<n>')`,
   `('catalog_version', '<date-based version string>')`, and `('flutter_sdk_version', '<target Flutter version>')`
   — read back and asserted in an integration test.
8. The produced `catalog.db` is **not** committed to the repository (consistent with the existing `.gitignore`'s
   `*.db` rule and TRD Section 4.1's "shipped as a pre-built... asset" framing) — instead, `cargo xtask
   seed-catalog` is documented (in this ticket's PR description and a short `assets/catalog_seed/README.md`
   update) as a required local/CI build step, and CI is updated to run it before any test that depends on a
   populated `catalog.db`.
9. Every migration file and the seeding tool's validation logic has at least one integration test that runs
   against a `tempfile`-created temporary SQLite path — no test in this ticket touches a real OS data-directory
   file.

## Implementation Details & Design Notes

- **Read-only enforcement is Ticket 005's job, not this ticket's** — this ticket's `xtask seed-catalog` tool is
  the *only* code path in the entire project permitted to open `catalog.db` for writing, and it does so as a
  standalone build-time binary invocation, never as part of the shipped `fwt` application binary's runtime code
  paths. Do not share connection-construction code between `xtask`'s seeding logic and `fwt-infra`'s (Ticket 005)
  read-only repository adapter beyond the schema/migration definitions themselves — keeping these separate is
  what makes ADR-1's isolation guarantee a structural property rather than a discipline.
- **Where migrations live vs. where they're embedded:** the SQL files live in `migrations/catalog/` at the
  workspace root (per TRD Section 7's directory listing) as the human-readable, version-controlled source of
  truth; `xtask` and `fwt-infra` both embed them at compile time (via `include_str!`/the chosen crate's embedding
  macro) rather than reading them from disk at runtime — this is what makes `catalog.db` regeneration reproducible
  from a specific git commit, and is why the SQL files themselves, not just the produced `.db`, are the tracked
  artifact.
- **JSON-in-TEXT columns (`enum_options`):** per TRD Section 4.2, `properties.enum_options` is a `TEXT` column
  holding a JSON array. The seeding tool must validate this is well-formed JSON (an array of strings) at seed
  time — this is the one place in this ticket where "the schema *looks* untyped SQL but actually has an implicit
  contract" needs explicit, tested validation, since a malformed `enum_options` string would otherwise surface
  much later as a parse error deep in Epic 3's `Dynamic Code Parameter Builder`.
- **`related_widget_id` resolution:** seed source files reference related widgets **by name** (human-authored
  TOML shouldn't require knowing an auto-incremented integer ID in advance), and the seeding tool resolves
  name→id after a first insertion pass, in a second pass — this two-pass approach (insert all widgets first,
  then resolve+update `related_widget_id` FKs) should be called out explicitly in code comments since a
  single-pass approach would hit an unsatisfiable ordering dependency for any pair of widgets that reference each
  other or reference a widget seeded later in file-iteration order.
- **Category naming consistency:** the wireframe's `categories` JS array (in `flutter_widget_catalog_tui.html`) is
  the canonical category-name source of truth for MVP seed data — reuse those exact strings (`"Scrolling"`, not
  `"scrolling"` or `"Scroll"`) in seed TOML files, since `fwt-tui`'s Catalog tab (Ticket 007 is Search-focused, but
  this naming consistency also directly serves Epic 3's category grid) will otherwise show a mismatched/duplicated
  category list.
- **Migration tool decision (`rusqlite_migration` recommended):** confirm compatibility with the `bundled`
  `rusqlite` feature explicitly in this ticket — some migration crates assume a system SQLite install for their
  own tooling; verify `rusqlite_migration`'s embedding mechanism works cleanly against `rusqlite`'s bundled build
  before committing to it over `refinery`.

## Folders / Files Impacted

    migrations/catalog/
    ├── README.md                          # MODIFIED — document versioning convention, xtask seed-catalog usage
    ├── V1__initial_schema.sql             # NEW — widgets, widgets_fts (+triggers), code_samples, properties,
    │                                       #        methods, catalog_meta, indexes
    assets/catalog_seed/
    ├── README.md                          # MODIFIED — document TOML schema, validation rules, contribution flow
    └── widgets/
        ├── list_view.toml                  # NEW — verbatim wireframe-matching entry
        ├── grid_view.toml                  # NEW — referenced by list_view's related_widget_id
        └── ... (≥100 curated MVP entries)  # NEW
    xtask/
    └── src/
        ├── main.rs                         # MODIFIED — dispatch `seed-catalog` subcommand
        └── seed_catalog.rs                 # NEW — TOML parsing, validation, two-pass insertion, migration apply
    crates/fwt-infra/
    ├── Cargo.toml                          # MODIFIED — add rusqlite (bundled), chosen migration crate
    └── src/db/
        └── migrations.rs                   # NEW — embedded migration set, shared by xtask and (read-only-mode)
                                              #        fwt-infra runtime migration-check

## Testing Plan

- **Migration integration tests (`crates/fwt-infra/tests/` or `xtask` internal tests):** apply the full migration
  set against a `tempfile`-created SQLite file; assert the resulting schema matches expectations (table/column
  presence via `PRAGMA table_info`); assert re-applying is a no-op; assert the FTS5 triggers correctly reflect an
  `INSERT`/`UPDATE`/`DELETE` on `widgets` into `widgets_fts` (insert a widget, `SELECT` from `widgets_fts MATCH
  ...`, confirm it's found; delete it, confirm it's gone).
- **Foreign key enforcement test:** attempt to insert a `code_samples` row with a non-existent `widget_id` against
  a migrated temp DB with `PRAGMA foreign_keys = ON`; assert the insert errors.
- **Seed validation unit tests (`xtask/src/seed_catalog.rs`):** table-driven cases for malformed seed entries
  (missing required field, invalid `enum_options` JSON, `related_widget_id` referencing a non-existent name,
  duplicate widget names) — each asserted to produce a specific, file-and-field-identifying error, not a generic
  failure or a silent skip.
- **End-to-end seed test:** run the full `xtask seed-catalog` pipeline against the real `assets/catalog_seed/`
  contents into a `tempfile` path; assert the resulting `catalog.db` contains ≥100 widgets, contains the
  wireframe-matching `ListView` entry with expected field values, and that `catalog_meta` is populated.
- **CI wiring:** confirm (via a CI config note in this ticket's PR, not necessarily a new test file) that
  `cargo xtask seed-catalog` runs before any Epic 2+ test suite that depends on a populated `catalog.db`.

## Potential Risks / Edge Cases

- **Risk: FTS5 trigger desync.** If the `AFTER UPDATE` trigger's column list doesn't exactly match `widgets_fts`'s
  declared columns, updates to `summary` (the most likely field to be revised during content curation) could
  silently fail to propagate into the FTS index, causing search results to reflect stale text. Mitigate via the
  explicit trigger-sync integration test in the Testing Plan — treat any change to `widgets`' or `widgets_fts`'s
  column set in a future migration as requiring a corresponding trigger review, called out in a code comment atop
  the trigger definitions.
- **Risk: seed data authoring bottleneck.** Per TRD Section 11's own risk table, hand-curating even 100 widgets
  with correct properties/methods/code samples is a real content effort. Mitigate by keeping the per-widget TOML
  schema as simple and low-friction as possible (flat fields, no unnecessary nesting) and by not blocking this
  ticket's *technical* completion on 100% content polish — a widget entry with a placeholder `overview` string is
  an acceptable interim state for this ticket; content quality is the ongoing workstream TRD Section 11 already
  flags separately.
- **Edge case: `related_widget_id` cycles.** Two widgets mutually referencing each other (`ListView` → `GridView`
  "for 2D scrolling", `GridView` → `ListView` "for linear scrolling") is valid and expected, not a cycle to
  reject — confirm the two-pass resolution logic handles this correctly (it should, since both rows exist by the
  second pass) and add it as an explicit test case, not just an assumption.
- **Edge case: `xtask seed-catalog` re-run against an existing output path.** Decide and document explicitly
  (recommend: fail with a clear "output file already exists, use `--force` to overwrite" error rather than silently
  appending/duplicating rows) — an accidental double-seed producing duplicate `widgets.name` UNIQUE constraint
  violations mid-run would leave a partially-seeded, confusing intermediate file if not handled as an atomic
  "build to a temp path, then rename into place" operation.

---