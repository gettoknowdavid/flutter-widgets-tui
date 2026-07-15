# Epic 2: Catalog Data & Search Architecture

**Epic ID:** EPIC-02

**Status:** Ready for Ticketing

**Depends on:** Epic 1 (Core Project Setup, Architecture & TUI Foundation) — complete. Specifically requires: the
5-crate workspace boundary enforcement (`xtask check-boundaries`), the `AppState`/`Message`/`update()` skeleton
(Ticket 004), the `AppShell` rendering convention (Ticket 005), and the non-blocking `Command`/`executor` round-trip
(Ticket 003), all of which this epic's `SearchService` and `CatalogRepository` integrations plug into without
restructuring.

**Blocks:**

- Epic 3 (Detail & Code Builder) — requires `Widget`/`Property`/`Method`/`CodeSample` domain types and
  `CatalogRepository::get_widget_by_id` to exist.
- Epic 4 (Favorites & Sync) — requires `CatalogRepository::widget_exists`/`get_widget_by_name` for favorite
  validation against denormalized `widget_name`.
- Epic 5 (AI Chat) — requires the fuzzy/FTS `SearchService` as the retrieval-augmentation mechanism (TRD Section
  5.2, step 2).

---

## 1. Objective

Establish the **read-only widget catalog** as a real, queryable SQLite database (`catalog.db`), the pure domain
types that model its contents, the `CatalogRepository` port-and-adapter pair that lets the rest of the application
read it without knowing SQL exists, and the two-stage search pipeline (TRD Section 6) that makes the catalog
*fast to search*, not just present. This is the epic where Flutter Widgets TUI stops being an empty shell and
becomes a tool that can answer "what widgets exist and which one do I want" — without yet rendering any of that
data to the screen in its final form (Epic 3 owns the detail view; this epic owns getting correct, fast, tested
data *into* `AppState` and a first, functional Search tab UI wired to it).

Success for this epic means: a future engineer picking up Epic 3 can call
`CatalogRepository::get_widget_by_id(id)` and get a fully-populated `Widget` (with its `Property`, `Method`, and
`CodeSample` children already join-loaded or trivially loadable) without needing to touch SQL, and a future
engineer picking up Epic 5's `ChatService` can call `SearchService::search(query, limit)` and get ranked,
match-highlighted candidates in well under NFR-3's 30ms budget — because both of those seams were designed,
implemented, and load-tested *here*, not improvised later under a different epic's time pressure.

## 2. Scope

This epic covers, and is strictly limited to:

- **`catalog.db` schema and embedded migrations**, exactly as specified in TRD Section 4.2: `widgets`,
  `widgets_fts` (FTS5), `code_samples`, `properties`, `methods`, `catalog_meta`. Migrations are versioned and
  embedded in the binary (via `refinery` or `rusqlite_migration` — decision finalized in this epic, per TRD
  Section 13, item 1 is fuzzy-matcher-related but the migrations-tooling choice is analogous and belongs here).
- **ADR-1 enforcement**: `catalog.db` is opened **read-only** at the connection level wherever the application
  (not a build-time seeding tool) touches it. `user.db` does not exist yet in this epic (Epic 4 scope) — no
  cross-database code is written here, but the `CatalogRepository` trait's shape must not preclude Epic 4's later
  composition of `CatalogRepository` + `UserDataRepository` in `FavoritesService`.
- **A minimal, hand-curated catalog seed** (per TRD Risk table Section 11: "start with a well-scoped subset... top
  100 most-used widgets") sufficient to exercise and validate the schema, search, and (later) detail-view epics —
  full catalog content curation as an ongoing effort is explicitly **out of scope** for this epic's *ticket* work,
  though the seed-authoring *pipeline* (`assets/catalog_seed/`) is in scope.
- **Domain models** (`fwt-domain`): `Widget`, `Property`, `Method`, `CodeSample`, plus supporting value types
  (`DesignSystem`, `InputKind`) — pure data, zero I/O, mirroring the TRD 4.2 schema field-for-field.
- **`CatalogRepository` port** (trait, defined in `fwt-domain::ports`) and its **SQLite adapter**
  (`fwt-infra::db::catalog_repo`) — implementing `rusqlite` (bundled feature) access with connection pooling
  (`r2d2` or `deadpool-sqlite` — decision finalized in this epic).
- **Two-stage search architecture** (TRD Section 6): an SQLite `FTS5` `MATCH` query as a coarse pre-filter, feeding
  a candidate set into an in-memory fuzzy matcher for fine ranking and match-position highlighting. This epic
  finalizes the `nucleo` vs `fuzzy-matcher` decision (TRD Section 13, item 1) via a time-boxed spike ticket, per
  the TRD's explicit instruction.
- **Async, off-render-thread index loading**: the full widget corpus (name/category/summary) is loaded into the
  chosen matcher's in-memory index once at startup, dispatched as a `Command` through the existing
  `executor`/`mpsc` round-trip established in Epic 1 Ticket 003 — **not** a new, parallel async mechanism.
- **A first functional Search tab UI** (`fwt-tui::views::search`) wired to `SearchService`, rendering ranked
  results with match-highlighted characters into the `AppShell`'s content pane (established in Epic 1 Ticket
  005), extending `AppState`'s `SearchStatePlaceholder` into a real `SearchState`.
- **Catalog tab grid** (a minimal, functional version of the wireframe's category grid — `Screen::Catalog`) to the
  extent needed to prove `CatalogRepository` end-to-end from keypress to rendered widget names; the *detail* view
  itself (drilling into a single widget) remains Epic 3 scope.

**Explicitly out of scope for this epic** (deferred to later epics): the widget **detail** view
(overview/code/properties/methods sub-tabs — Epic 3), the Dynamic Code Parameter Builder (Epic 3), favorites/
`user.db`/GitHub sync (Epic 4), AI chat and its retrieval-augmented prompt construction (Epic 5 — though this
epic's `SearchService` is the exact dependency Epic 5 will call), full theming (Epic 6), and clipboard/yank
(Epic 3/6). Full catalog content authoring beyond the MVP seed subset is an ongoing, separately-tracked content
workstream per TRD Section 11's risk mitigation, not a ticket deliverable here.

## 3. Acceptance Criteria

Epic 2 is considered **done** when all of the following hold:

1. `cargo build --workspace` succeeds cleanly; `fwt-domain` remains free of `rusqlite`/`nucleo`/`fuzzy-matcher`
   dependencies (verified by the existing `xtask check-boundaries` check — no new forbidden-crate entries are
   required for this epic, since search/DB concerns live entirely in `fwt-infra`).
2. A `catalog.db` file can be built from `assets/catalog_seed/` source data via a documented, repeatable process
   (a `cargo xtask seed-catalog` subcommand or equivalent script), producing a valid SQLite file matching the TRD
   4.2 schema, containing at least the curated MVP widget subset (target: ~100 widgets spanning Cupertino,
   Material, and base categories, including the wireframe's `ListView` example verbatim so cross-referencing the
   wireframe stays possible in manual QA).
3. Migrations are versioned, embedded in the binary, and applied automatically/idempotently on startup against a
   missing or outdated local `catalog.db` copy — verified by an integration test that runs migrations twice in a
   row against the same temp file without error.
4. `CatalogRepository` (trait in `fwt-domain::ports`) exposes, at minimum: `get_widget_by_id`,
   `get_widget_by_name`, `list_categories`, `list_widgets_by_category`, `search_fts` (the coarse FTS5 pass), and
   `load_search_corpus` (the full name/category/summary set for in-memory index construction) — each returning
   `Result<T, RepositoryError>` (`thiserror`), never panicking on a missing row (returns `Ok(None)`/`Ok(vec![])`
   as appropriate, reserving `Err` for genuine I/O/corruption failures).
5. The SQLite adapter opens `catalog.db` in a mode that makes accidental writes fail loudly (e.g.
   `SQLITE_OPEN_READ_ONLY`, or an app-level connection-pool configuration that never issues `INSERT`/`UPDATE`/
   `DELETE`) — verified by a test that attempts a write through the adapter's connection and asserts it errors
   rather than silently succeeding, protecting ADR-1's isolation guarantee.
6. The two-stage search pipeline is demonstrably wired end-to-end: a query string produces an FTS5-filtered
   candidate set, which is then scored/ranked by the chosen in-memory fuzzy matcher, and the final ordered,
   match-highlighted result set is what `SearchService::search()` returns — proven by an integration test using a
   seeded temp `catalog.db` and asserting a known query (e.g. `"scrollable list"`, mirroring the wireframe's
   placeholder text) returns `ListView` in the top N results.
7. NFR-3 (search latency: first-paint results < 30ms/keystroke) is measured via a `criterion` micro-benchmark
   against the full seeded MVP corpus and the number is recorded in the ticket close-out — not necessarily
   enforced as a hard CI gate in this epic (a full 350–500 widget corpus doesn't exist yet), but the benchmark
   harness itself must exist and be runnable so later catalog-content growth can be checked against it.
8. Index construction (loading the full corpus into the in-memory matcher) happens asynchronously, off the
   render/event-loop thread, dispatched via the existing `Command`/`executor` mechanism from Epic 1 Ticket 003 —
   verified by a test analogous to Ticket 003's "non-blocking guarantee" test: a synthetic slow index-load does
   not prevent the event loop from processing concurrent input.
9. The Search tab (`[2] Search`) is interactive: typing updates `SearchState.query`, dispatches a debounced (or
   otherwise rate-limited, per Implementation Notes below) `Command::Search`, and renders ranked results with
   bold/highlighted matched characters into the content pane — verified by an `insta` `TestBackend` snapshot test
   of a populated result list.
10. The Catalog tab (`[1] Catalog`) renders the real category grid (from `CatalogRepository::list_categories`),
    replacing Epic 1 Ticket 005's static placeholder text, with Cupertino/Material visually promoted above base
    categories per the wireframe — pressing Enter/selecting a category is wired to at least log/set navigation
    state (Epic 3 renders the actual detail view; this epic only needs the category→selection plumbing to exist
    and be tested).
11. All domain types (`fwt-domain`) have unit test coverage for construction/invariants (e.g., a `Widget` cannot be
    constructed with an empty `name`); the SQLite adapter (`fwt-infra`) has integration tests against a real
    temporary SQLite file (via `tempfile`) applying real migrations, per TRD Section 10's infrastructure-layer
    testing strategy — no test in this epic hits a real, non-temp, non-seeded database file.
12. A fresh-session code review pass (per TRD Section 12 workflow) has been performed against this Epic's tickets
    and the TRD, with specific attention to the ADR-1 read-only boundary and the `fwt-domain` purity boundary, with
    no open architectural violations.

## 4. Dependencies

Crate versions below extend, rather than replace, the Epic 1 dependency set. Exact patch versions are locked in
`Cargo.lock` at ticket implementation time.

| Crate                                                    | Version constraint                                               | Scope in Epic 2                                                                                                                                                                               |
| -------------------------------------------------------- | ---------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `rusqlite`                                               | `^0.32`, features = `["bundled"]`                                | `catalog.db` access. `bundled` avoids system libsqlite version drift across contributor machines and CI/release targets (NFR-10).                                                             |
| `r2d2` + `r2d2_sqlite` (or `deadpool-sqlite`)            | latest stable; decision finalized in Ticket 005                  | Connection pooling — avoids lock contention between concurrent reads once `user.db`/Epic 4 introduces a writer, and keeps this epic's pooling pattern consistent with what Epic 4 will reuse. |
| `rusqlite_migration` (or `refinery`)                     | latest stable; decision finalized in Ticket 004                  | Versioned, embedded SQL migrations for `catalog.db`.                                                                                                                                          |
| `nucleo` **or** `fuzzy-matcher`                          | latest stable; decision finalized via Ticket 006's spike         | In-memory fuzzy matching engine for fine-ranking search candidates (TRD Section 6, step 2).                                                                                                   |
| `tantivy`-adjacent: **not used** — FTS5 is SQLite-native | n/a                                                              | Explicitly rejected: TRD Section 6 specifies SQLite `FTS5` as the coarse filter, not a separate full-text engine, to avoid a second storage/index technology for a bounded corpus size.       |
| `criterion`                                              | `^0.5`, dev-dependency                                           | NFR-3 search-latency micro-benchmark (acceptance criterion 7).                                                                                                                                |
| `tempfile`                                               | `^3`, dev-dependency                                             | Real-SQLite-file integration tests (TRD Section 10, infrastructure layer).                                                                                                                    |
| `unicode-segmentation`                                   | latest stable (likely already transitive via `ratatui`/`nucleo`) | Correct match-highlighting across multi-byte/grapheme-cluster widget names/summaries, if the chosen matcher doesn't already guarantee this.                                                   |

**Open decision carried into Ticket 004:** `rusqlite_migration` vs `refinery` for embedded migrations. Both
support compile-time-embedded, versioned SQL migrations; `rusqlite_migration` is the lighter-weight, more
`rusqlite`-idiomatic choice (no separate runtime/connection-abstraction layer), while `refinery` supports multiple
backends the project doesn't need. Recommendation: adopt `rusqlite_migration` for its minimalism, matching the
TRD's general "prefer minimal transitive dependency bloat" crate-selection philosophy (TRD Section 3) — finalized
as part of Ticket 004, not left open past this epic.

**Open decision carried into Ticket 005:** `r2d2`/`r2d2_sqlite` vs `deadpool-sqlite` for connection pooling.
`r2d2` is the more battle-tested, synchronous-pool option (a reasonable fit since `rusqlite` itself is
synchronous and pool checkout happens inside a `spawn_blocking`-wrapped task per Ticket 005's design notes);
`deadpool-sqlite` is async-native but adds a heavier dependency footprint for a benefit (native async checkout)
this app's `spawn_blocking`-wrapped access pattern doesn't actually need. Recommendation: adopt `r2d2` +
`r2d2_sqlite`, finalized in Ticket 005.

**Open decision, spiked in Ticket 006 (not pre-decided here, per TRD Section 13 item 1 and the explicit ask):**
`nucleo` vs `fuzzy-matcher`.

## 5. Estimated Complexity

**Overall: Medium-High.** The schema and repository work (Tickets 004–005) is mechanical and low-risk given how
prescriptively TRD Section 4 already specifies the schema. The search engine work (Ticket 006) carries genuine
uncertainty — it includes a mandated spike, a performance-sensitive async-loading integration point, and the
epic's only NFR-gated acceptance criterion (NFR-3) — and is the epic's risk center of gravity, mirroring how
Epic 1's terminal-lifecycle ticket was disproportionately risk-laden relative to its visible surface area. Ticket
007 (Search UI) is comparatively low complexity, being the first ticket in the project to render *dynamic*,
data-driven content into the `AppShell` established by Epic 1, but otherwise follows a well-trodden Ratatui path.

| Ticket                             | Complexity |
| ---------------------------------- | ---------- |
| ticket-004-catalog-schema          | Medium     |
| ticket-005-catalog-repository      | Medium     |
| ticket-006-search-spike-and-engine | High       |
| ticket-007-search-ui               | Low-Medium |

---