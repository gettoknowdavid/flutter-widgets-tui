# Ticket 006: Fuzzy Matcher Spike & Two-Stage Search Engine

**Epic:** EPIC-02 — Catalog Data & Search Architecture

**Complexity:** High

**Depends on:** `ticket-005-catalog-repository`

**Blocks:** `ticket-007-search-ui`, Epic 5 (AI Chat's retrieval-augmentation step)

---

## Description

This ticket has two distinct phases that must both complete before it is considered done: **(a)** a time-boxed
spike comparing `nucleo` against `fuzzy-matcher`, finalizing TRD Section 13's open decision with recorded
evidence rather than a coin-flip, and **(b)** the implementation of `SearchService` — the `fwt-app`-layer
orchestration of TRD Section 6's two-stage pipeline (SQLite FTS5 coarse filter → in-memory fuzzy fine-ranking),
including asynchronous, off-render-thread index construction wired through Epic 1 Ticket 003's existing
`Command`/`executor` mechanism.

This is the epic's highest-risk ticket, per the Epic-level complexity estimate: it is the only ticket in Epic 2
with a genuinely open technical unknown (which matcher performs better against real widget-catalog-shaped
queries), the only one with a hard NFR gate (NFR-3), and the one most likely to reveal an uncomfortable surprise
(e.g., "the in-memory index takes longer to build than expected and NFR-2's startup budget is at risk") that a
less careful ticket breakdown might have discovered only after UI work (Ticket 007) was already built on top of a
wrong assumption.

## Acceptance Criteria

### Phase A — Spike (must complete first, findings recorded before Phase B implementation begins)

1. A small, internal **evaluation set** exists (recommend: `assets/catalog_seed/search_eval.toml` or a test-local
   fixture) of at least 15 representative query→expected-top-result pairs, covering: exact name matches
   (`"ListView"` → `ListView`), typo-tolerant matches (`"Lisview"` → `ListView`), partial/substring matches
   (`"scroll"` → multiple scrolling-category widgets), and natural-language/use-case queries mirroring the
   wireframe's placeholder text (`"scrollable list"` → `ListView`, `"infinite scroll"` → `ListView` and/or a
   lazy-loading-relevant widget) — per TRD Section 11's risk mitigation ("build a small internal eval set of
   query→expected-top-result pairs as a regression test"), this eval set is **not** spike-only scaffolding; it
   is committed and reused as Phase B's regression test.
2. Both `nucleo` and `fuzzy-matcher` are integrated behind a common, ticket-local trait (e.g., a throwaway
   `trait SpikeMatcher` — not the final `SearchService` abstraction, to avoid over-committing the real
   architecture to spike-only code) and run against the evaluation set from criterion 1, against the full seeded
   MVP catalog corpus from Ticket 004 (~100 widgets).
3. The spike records, for both candidates, in this ticket's PR description or a short `docs/adr/adr-00X-fuzzy-
   matcher-selection.md`: (a) eval-set pass rate (top-1 and top-3 accuracy), (b) measured per-query latency
   against the full corpus (using `criterion` or a simple manual-timing harness — full rigor is Phase B's
   `criterion` benchmark, not required at spike granularity), (c) API ergonomics/match-highlighting support
   (does the crate natively expose matched character positions for UI bolding, per TRD Section 6 step 2's "so
   the UI can bold matched characters, à la fzf/telescope.nvim," or does highlighting need to be hand-rolled on
   top of a bare score), and (d) maintenance/dependency-footprint considerations (transitive dependency count,
   last-publish recency).
4. A **final decision is recorded** in an ADR (`docs/adr/adr-00X-fuzzy-matcher-selection.md`) before any Phase B
   implementation code is written against the chosen crate — the ADR explicitly states the runner-up and why it
   was rejected, following the established pattern of `docs/adr/adr-001-terminal-lifecycle-and-panic-hook-
   ordering.md`'s "Alternatives Considered" section.
5. The spike is explicitly time-boxed — if no clear winner emerges after the evaluation (e.g., both perform
   comparably), the ADR documents the tie-break rationale (TRD Section 3 recommends `nucleo` "for best-in-class
   performance and match-highlighting" as the default lean) rather than extending the spike indefinitely.

### Phase B — `SearchService` Implementation

6. `fwt-app::search_service::SearchService` is implemented, taking an injected `Arc<dyn CatalogRepository>` (per
   Ticket 005's port) and owning the in-memory fuzzy index (constructed from the chosen matcher crate, per the
   Phase A decision) — `SearchService::search(&self, query: &str, limit: usize) -> Vec<SearchResult>` where
   `SearchResult` includes the matched `WidgetSummary` plus match-position data sufficient for the UI (Ticket
   007) to bold matched characters.
7. The two-stage pipeline is implemented exactly per TRD Section 6: `search()` first calls
   `CatalogRepository::search_fts(query, coarse_limit)` (a generous coarse limit, e.g. 200, effectively
   "everything plausible" for the current MVP corpus size, documented as a constant to revisit once the corpus
   grows toward NFR-3's 350–500 widget target) to narrow the corpus, then runs the in-memory matcher against
   *only* that candidate set (not the full corpus) for fine-ranking — verified by a test asserting the fine-
   ranking stage never receives a candidate that wouldn't have matched the coarse FTS5 query, and by an
   instrumented/mock-repository test confirming `search_fts` is actually called (not bypassed) on every search.
8. Multi-field weighting (TRD Section 6, step 4) is implemented: name matches rank above category matches, which
   rank above summary matches — implemented as a documented, configurable weight table (not hardcoded magic
   numbers scattered through scoring logic), with a unit test asserting a query matching one widget's name and
   a different widget's summary ranks the name-match first.
9. **Index residency and async loading (TRD Section 6, step 3 / Epic scope item):** `SearchService`'s in-memory
   index is built **once**, from `CatalogRepository::load_search_corpus()`, and is **not** re-queried from SQLite
   per keystroke. Index construction (and rebuilds, if ever triggered — none are in this epic's scope beyond
   startup) is dispatched as a `Command` (extending `fwt-app::command::Command` with a new
   `Command::BuildSearchIndex` variant) through the existing Epic 1 Ticket 003 `executor`/`mpsc` round-trip,
   completing asynchronously and notifying the event loop via a new `Message::SearchIndexReady` variant — **not**
   built synchronously and blockingly inside `SearchService::new()` or anywhere on the render/event-loop thread.
10. A `criterion` benchmark (`benches/search_latency.rs` or similar) measures `SearchService::search()` latency
    against the full seeded MVP corpus for a representative mix of query types (exact, typo, natural-language),
    with the measured p50/p99 numbers recorded in this ticket's close-out notes against NFR-3's < 30ms target —
    if the MVP-scale corpus is too small to meaningfully stress-test NFR-3's "~350–500 widgets" target, the
    benchmark harness must support being re-pointed at a larger synthetic/generated corpus for this validation,
    and this limitation is documented explicitly rather than silently claiming NFR-3 compliance from an
    under-scale test.
11. `SearchService` is unit-testable against a **fake** `CatalogRepository` (an in-memory
    `FakeCatalogRepository` implementing the Ticket 005 trait, per TRD Section 10's application-layer testing
    strategy) with no real SQLite file involved — the eval-set regression test from criterion 1 runs against the
    real seeded catalog (an integration-level test), but `SearchService`'s orchestration logic itself (two-stage
    sequencing, weighting, result assembly) is separately unit-tested against fakes for speed and determinism.

## Implementation Details & Design Notes

- **Where `SearchService` lives and what it depends on:** `fwt-app::search_service`, per TRD Section 7's
  directory listing. It depends on `fwt-domain`'s `CatalogRepository` port and the chosen matcher crate — this
  is the **first** point in the workspace where `fwt-app` gains a dependency beyond `fwt-domain`/`tokio`/
  `crossterm`; update `fwt-app/Cargo.toml` accordingly and note in the PR that this is an intentional, reviewed
  addition (the same "don't let dependency additions look like unexplained drive-by bumps" discipline established
  in Epic 1 Ticket 003's design notes).
- **Async index construction, concretely:** on `SearchService` construction (in `fwt-cli`'s composition root),
  emit `Command::BuildSearchIndex` as part of the initial `Command` set dispatched right after the event loop
  starts (mirroring how Epic 1 Ticket 003's dummy `Command::SimulatedDelay` proved the round-trip) — the
  executor's handling of this command calls `CatalogRepository::load_search_corpus()` (itself already
  `spawn_blocking`-wrapped per Ticket 005) inside a `tokio::spawn`'d task, builds the in-memory matcher index
  from the result, and sends `Message::SearchIndexReady` back. Until that message arrives, `SearchState`
  (Ticket 007) should reflect an explicit "index loading" state — a user typing into Search before the index is
  ready should see a clear, non-broken "warming up" indication, not a silent empty-results screen indistinguishable
  from "no matches."
- **Coarse FTS5 limit tuning:** the "generous coarse limit" constant (criterion 7) exists specifically so the
  in-memory fine-ranking stage's cost scales with *candidate count*, not full corpus size — as the catalog grows
  toward NFR-3's 350–500 widget target, revisit this constant's value against real benchmark data (criterion 10)
  rather than assuming the MVP-scale default remains correct; leave an explicit `// TODO(post-Epic-2, revisit
  against full-corpus benchmark)` comment.
- **Match-highlighting data shape:** `SearchResult`'s match-position data should be expressed as a
  `Vec<Range<usize>>` (or the chosen matcher's native equivalent, mapped into this shape at the `SearchService`
  boundary) over **byte offsets into the matched field**, with an explicit note that Ticket 007's rendering code
  must respect UTF-8/grapheme-cluster boundaries when converting these into Ratatui `Span` styling — most Flutter
  widget names are ASCII, but summaries/use-case text are free-form and must not be assumed ASCII-only.
- **Why FTS5 result caching isn't in scope:** re-running the FTS5 coarse query on every keystroke (rather than
  caching/incrementally narrowing it) is the simplest correct approach and is expected to be well within budget
  at this corpus size (SQLite FTS5 queries against a few hundred rows are sub-millisecond); do not add an
  incremental-narrowing cache layer preemptively without evidence from criterion 10's benchmark that it's needed,
  consistent with the TRD's "boring and conservative" architecture philosophy.

## Folders / Files Impacted

    docs/adr/
    └── adr-002-fuzzy-matcher-selection.md   # NEW — Phase A spike findings and final decision
    assets/catalog_seed/
    └── search_eval.toml                      # NEW — query→expected-top-result evaluation set (committed,
                                                #        reused as Phase B regression test fixture)
    crates/fwt-app/
    ├── Cargo.toml                            # MODIFIED — add chosen matcher crate
    └── src/
        ├── lib.rs                            # MODIFIED — expose search_service module
        ├── command.rs                        # MODIFIED — add Command::BuildSearchIndex
        ├── message.rs                        # MODIFIED — add Message::SearchIndexReady
        ├── executor.rs                       # MODIFIED — handle Command::BuildSearchIndex dispatch
        └── search_service.rs                 # NEW — SearchService, SearchResult, weighting table
    benches/
    └── search_latency.rs                     # NEW — criterion benchmark against seeded corpus (NFR-3)

## Testing Plan

- **Spike evaluation harness (Phase A, not necessarily committed as a permanent test, but its output —
  `search_eval.toml` and the ADR — is):** run both candidate matchers against the evaluation set; this is
  exploratory/comparative, not a pass/fail CI gate in itself.
- **`SearchService` unit tests against `FakeCatalogRepository`:** two-stage sequencing (coarse-then-fine, never
  fine-only), multi-field weighting ordering, empty-query and no-results edge cases, and correct behavior before
  `Message::SearchIndexReady` has been processed (should not panic — should return either an empty result set or
  a distinguishable "not ready" signal, per the Implementation Notes above).
- **Eval-set regression integration test (`fwt-app` or `fwt-infra` integration tests, real seeded `catalog.db`):**
  runs the committed `search_eval.toml` cases against the real, final `SearchService` (chosen matcher, real
  `SqliteCatalogRepository`) and asserts top-1 (or top-3, per the specific case's tolerance) accuracy — this is
  the permanent regression test TRD Section 11 calls for, and must be run in CI on every future change to search
  weighting/scoring logic.
- **Async round-trip test (extends Epic 1 Ticket 003's pattern):** dispatch `Command::BuildSearchIndex` against
  a real (temp, seeded) `CatalogRepository`, using `tokio::time::pause()`/synthetic delay if needed to simulate a
  slow corpus load, and assert the event loop continues processing concurrent synthetic input messages during
  index construction, and that `Message::SearchIndexReady` eventually arrives and is correctly handled by
  `update()`.
- **`criterion` benchmark (criterion 10):** run against the real seeded MVP corpus; record and report p50/p99
  latencies; flag explicitly (in the benchmark's own doc comment and the ticket close-out) if corpus scale is
  insufficient to validate the full NFR-3 target, per criterion 10's documented-limitation requirement.

## Potential Risks / Edge Cases

- **Risk: MVP-scale benchmark under-validates NFR-3.** A ~100-widget corpus benchmark passing comfortably under
  30ms says very little about behavior at 350–500 widgets. Mitigate via the benchmark harness's re-pointable
  design (criterion 10) — consider generating a synthetic 500-entry corpus (procedurally varied names/categories/
  summaries, clearly marked as synthetic, not shipped) purely for this stress-test purpose, documented as a
  fast-follow if not completed within this ticket's time-box.
- **Risk: index construction genuinely exceeds NFR-2's startup budget once full-scale.** If Phase B's async
  dispatch reveals the index build itself (not just per-query search) is slow at scale, this is a real, load-
  bearing finding for later catalog-content growth (TRD Section 11's "Catalog data accuracy/completeness" risk)
  — document any such finding explicitly rather than treating a passing MVP-scale benchmark as proof the design
  scales.
- **Risk: chosen matcher's licensing/maintenance posture changes the crate-selection philosophy's calculus.**
  Explicitly check both candidates' license (MIT/Apache-2.0 compatible, consistent with this project's own MIT
  license) and maintenance recency as part of the Phase A spike's recorded findings (criterion 3d) — not just
  raw performance numbers.
- **Edge case: search index staleness relative to `catalog.db` updates.** Out of scope for this epic (no runtime
  catalog-update mechanism exists yet — Future Feature per TRD Section 4.1/8.1's version-tagging notes), but flag
  explicitly in `SearchService`'s module doc comment that a future catalog hot-swap feature will need an explicit
  `SearchService::rebuild_index()` path reusing this ticket's `Command::BuildSearchIndex` machinery, not a new
  parallel mechanism.
- **Edge case: empty or whitespace-only query strings.** `search_fts` against an empty FTS5 `MATCH` string is
  either a SQLite error or a nonsensical result depending on FTS5 version/configuration — `SearchService::search`
  must explicitly short-circuit on empty/whitespace-only input (returning an empty result set, not forwarding to
  `CatalogRepository`), tested explicitly rather than left to whatever SQLite happens to do.

---