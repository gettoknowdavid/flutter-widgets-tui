# Ticket 007: Search Tab UI & Catalog Category Grid

**Epic:** EPIC-02 — Catalog Data & Search Architecture

**Complexity:** Low-Medium

**Depends on:** `ticket-006-search-spike-and-engine`

**Blocks:** Epic 3 (detail-view navigation extends `Screen`/`SearchState` selection handling established here)

---

## Description

Wire `SearchService` (Ticket 006) and `CatalogRepository` (Ticket 005) into real, interactive views rendered
through the `AppShell` established in Epic 1 Ticket 005 — replacing that ticket's static "Epic 2+ content renders
here" placeholder with the project's first genuinely dynamic, data-driven screens. This ticket covers the
`[2] Search` tab (live fuzzy search with match-highlighted results) and a functional-but-minimal `[1] Catalog`
tab (the category grid, per the wireframe's `catGrid`), both following the `render_*(frame, area, state, theme) ->
Rect`-style convention Ticket 005 (Epic 1) established as the project's rendering pattern.

This ticket does **not** implement tab-switching itself if that machinery doesn't already exist from Epic 1 —
confirm Epic 1 Ticket 004/005's scope boundary; if real `Screen`-to-view dispatch was deferred, this ticket is
where `Screen::Catalog` and `Screen::Search` variants are first added to `fwt-app::navigation::Screen` and
`update()`'s key-handling is extended (per Epic 1 Ticket 004's explicit note: *"Epic 2's tab/navigation
handling"*) to route number-key presses (`1`/`2`) to these tabs.

## Acceptance Criteria

1. `fwt-app::navigation::Screen` gains `Screen::Catalog` and `Screen::Search` variants (extending, not replacing,
   `Screen::Shell` from Epic 1); `update()`'s key-handling extends to route `KeyCode::Char('1')` and
   `KeyCode::Char('2')` to push the corresponding screen via `NavigationStack::push`, per the wireframe's
   `[1] Catalog` / `[2] Search` hotkey labels — every existing exhaustive `match` on `Screen` in the codebase
   (per Epic 1 Ticket 004's explicit non-exhaustive-by-convention design intent) is updated to handle the new
   variants, causing a compiler error at any site that would otherwise silently ignore them.
2. `SearchStatePlaceholder` is replaced by a real `SearchState` struct (`fwt-app::state`) with, at minimum:
   `query: String`, `results: Vec<SearchResult>` (from Ticket 006), `index_ready: bool` (reflecting whether
   `Message::SearchIndexReady` has arrived yet), and `selected_index: Option<usize>` (for keyboard navigation of
   the result list) — `CatalogStatePlaceholder` is similarly replaced by a real `CatalogState` with
   `categories: Vec<CategorySummary>` and `selected_index: Option<usize>`.
3. Typing a character while `Screen::Search` is active appends to `SearchState.query` and dispatches a
   `Command::Search(query)` (new `Command` variant, extending Ticket 006's `Command::BuildSearchIndex`); the
   executor calls `SearchService::search()` (itself synchronous/CPU-bound once the index is built, so this
   `Command`'s handling may run inline in a `spawn_blocking`-free async task, or via `spawn_blocking` if the
   in-memory matcher's per-query cost ever proves non-trivial — measure against Ticket 006's benchmark data
   before deciding) and returns results via a new `Message::SearchResults(Vec<SearchResult>)`.
4. **Debouncing/rate-limiting:** rather than dispatching a `Command::Search` on every single keystroke
   unconditionally (which could, under fast typing, queue more search commands than the bounded `mpsc` channel's
   capacity comfortably absorbs, per Epic 1 Ticket 003's backpressure design intent), the simplest correct
   approach for this epic is: only the *latest* pending `Command::Search` result is applied to `SearchState`
   (any in-flight-but-superseded search result arriving late is discarded by comparing against the query it was
   computed for) — implement this explicitly (e.g., `Message::SearchResults` carries the query it answers, and
   `update()` ignores it if `state.search.query` has since changed) rather than a time-based debounce, since a
   query-staleness check is simpler, fully deterministic, and unit-testable without `tokio::time` machinery.
5. Search results render into the `AppShell`'s content pane (via the `render_app_shell`-returned inner `Rect`,
   per Epic 1 Ticket 005's established convention) as a list, each row showing the widget name (with
   match-highlighted character ranges rendered in a distinct `theme.accent`-styled `Span`, per Ticket 006's
   match-position data) and a truncated summary — matching the wireframe's clean, single-column list aesthetic
   rather than introducing new visual conventions.
6. Before `Message::SearchIndexReady` has arrived, the Search tab renders a clear, non-alarming "index loading…"
   state (not a blank pane, not an error) — matching TRD Section 2.4's "UI must clearly and non-intrusively
   surface... status... rather than blocking dialogs" principle, applied here to search-readiness rather than
   connectivity.
7. Up/Down arrow keys (or `j`/`k`, if a vim-style convention is preferred — decide and document, consistent with
   NFR-8's keyboard-only requirement) move `SearchState.selected_index` through the results list, clamped to
   valid bounds (no panic/wraparound-by-accident at the list's ends) — Enter on a selected result is wired to at
   least push a placeholder navigation event (e.g., logging the selected `WidgetId` or pushing an as-yet-
   unrendered `Screen::Detail(WidgetId)` variant stub) since the real detail view is Epic 3 scope, but the
   selection-to-navigation *plumbing* must exist and be tested here.
8. The Catalog tab renders `CatalogState.categories` as a grid (via `Layout`/`Constraint`, not manual coordinate
   math, per Epic 1 Ticket 005's established discipline), with Cupertino and Material visually promoted in a
   distinct "design systems" section above the general category grid, matching the wireframe's
   `┌─ design systems ─...` / `┌─ base widgets ─...` two-tier layout — category counts (`"· N widgets"`) are
   real, sourced from `CatalogRepository::list_categories()`, not hardcoded.
9. Both new views are covered by `insta` `TestBackend` snapshot tests (per Epic 1 Ticket 005's established
   pattern) at a standard terminal size, with committed baseline snapshots — at least one snapshot each for: an
   empty/initial Search tab, a Search tab with populated match-highlighted results, an index-loading Search tab
   state, and a populated Catalog tab grid.
10. No terminal panic or visual corruption occurs when `SearchState.results` is empty (no matches for the current
    query) or when `CatalogState.categories` is empty (a degenerate/misconfigured catalog) — both render a clear
    "no results"/"no categories available" message rather than an empty pane indistinguishable from a rendering
    bug.

## Implementation Details & Design Notes

- **View function signatures follow Epic 1 Ticket 005's precedent exactly:** `render_search_view(frame, area,
  state: &SearchState, theme: &Theme) -> ()` and `render_catalog_view(frame, area, state: &CatalogState, theme:
  &Theme) -> ()`, called by whatever top-level view-dispatch logic reads `state.navigation.current()` to decide
  which to invoke into the `AppShell`'s returned content `Rect` — do not have these functions call
  `render_app_shell` themselves; the shell owns the chrome, per TRD Section 8.3, and these are exactly the kind
  of views TRD Section 8.3 anticipated when it said "each view renders *into*" the shell.
- **Query-staleness discard logic (criterion 4) belongs in `update()`, not in the executor or the view layer** —
  this keeps the "only `update()` mutates `AppState`" invariant from Epic 1 Ticket 004 intact; the executor's job
  is only to dispatch and relay, never to decide whether a result is still relevant.
- **Match-highlighting rendering:** convert Ticket 006's `Vec<Range<usize>>` byte-offset ranges into a sequence
  of Ratatui `Span`s (alternating unstyled/`theme.accent`-styled runs) via a small, separately unit-testable pure
  function (e.g., `highlight_spans(text: &str, ranges: &[Range<usize>], theme: &Theme) -> Vec<Span>`) — keeping
  this logic out of the main render function body makes it testable without constructing a full `Frame`/
  `TestBackend`, and directly addresses Ticket 006's flagged UTF-8-boundary risk with a dedicated, focused test.
- **Reuse, don't duplicate, `Theme`:** both new views must reference `theme.accent`/`theme.border`/`theme.text`/
  etc. fields exactly as Epic 1 Ticket 005 established — no literal `Color::Rgb(...)` in this ticket's rendering
  code, maintaining the theming discipline from the very first dynamic-content ticket onward, per TRD Section 8.2.
- **Selection state and list scrolling:** for this epic's MVP-scale catalog (~100 widgets, category lists
  correspondingly small), a full virtualized/windowed list rendering is not required — render the full result/
  category list each frame and let Ratatui's own widget clipping handle overflow, revisiting only if manual QA
  or a later, much larger catalog reveals a real performance problem; do not preemptively build list
  virtualization in this ticket.

## Folders / Files Impacted

    crates/fwt-app/
    └── src/
        ├── navigation.rs                   # MODIFIED — Screen::Catalog, Screen::Search
        ├── state.rs                        # MODIFIED — real SearchState, CatalogState; remove placeholders
        ├── command.rs                      # MODIFIED — Command::Search(String)
        ├── message.rs                      # MODIFIED — Message::SearchResults(String, Vec<SearchResult>)
        └── executor.rs                     # MODIFIED — dispatch Command::Search via SearchService

    crates/fwt-tui/
    └── src/
        ├── app.rs                          # MODIFIED — key routing for '1'/'2', view dispatch on Screen
        └── views/
            ├── mod.rs                      # NEW
            ├── search.rs                   # NEW — render_search_view, highlight_spans
            └── catalog.rs                  # NEW — render_catalog_view

    tests/snapshots/
    ├── search_view_empty.snap              # NEW
    ├── search_view_results.snap            # NEW
    ├── search_view_index_loading.snap      # NEW
    └── catalog_view_grid.snap              # NEW

## Testing Plan

- **`update()` extension unit tests (`fwt-app`):** number-key routing to `Screen::Catalog`/`Screen::Search`;
  `Command::Search` dispatch on query change; the query-staleness discard logic (criterion 4) — a
  `Message::SearchResults` for a stale query is a no-op against current `SearchState.results`, table-driven
  across "still current," "stale," and "index not yet ready" cases.
- **`highlight_spans` unit tests (`fwt-tui`):** ASCII text with simple highlight ranges; a multi-byte
  UTF-8 string (e.g., a summary containing an em dash or accented character) with a highlight range, asserting
  no panic and correct span boundaries — directly covering Ticket 006's flagged UTF-8 risk.
- **Snapshot tests (`insta` + `TestBackend`):** the four states listed in acceptance criterion 9, at a standard
  terminal size; an additional pathological-size panic-safety pass consistent with Epic 1 Ticket 005's precedent
  (though full visual correctness at extreme sizes is still not required, only no-panic).
- **Selection/navigation unit tests:** Up/Down clamping at list boundaries (no underflow/overflow panic on an
  empty or single-item list); Enter on a selected search result correctly identifies the intended `WidgetId` and
  produces the expected (stub) navigation outcome.
- **Manual QA:** visually compare the rendered Search and Catalog tabs against `flutter_widget_catalog_tui.html`
  in a real terminal, confirming the design-systems/base-widgets two-tier grid layout and match-highlighting
  render legibly under at least two terminal emulators, per Epic 1's established compatibility-matrix habit.

## Potential Risks / Edge Cases

- **Risk: `Command::Search` flooding the bounded `mpsc` channel under fast typing.** The query-staleness discard
  (criterion 4) mitigates *stale results being applied*, but does not by itself prevent many in-flight
  `Command::Search` tasks from being spawned in quick succession. If manual QA under fast typing reveals this is
  a real problem (not just a theoretical one), consider a lightweight in-flight-generation counter (increment on
  each new query, cancel/ignore superseded generations at the executor level) as a documented fast-follow —
  avoid building this preemptively without evidence, consistent with the epic's "boring and conservative"
  philosophy, but flag the risk explicitly in code comments at the `Command::Search` dispatch site.
- **Risk: match-highlighting visual noise.** Over-aggressive highlighting (e.g., highlighting every matched
  character in a long fuzzy-matched summary) can look worse than no highlighting at all. Not a correctness risk,
  but flag as a manual-QA judgment call — if Ticket 006's chosen matcher's raw match positions look visually
  noisy in practice, consider limiting highlighting to the `name` field only as a pragmatic scope reduction,
  documented as a deliberate choice rather than a bug if adopted.
- **Edge case: `Screen::Search` re-entered with a previously-typed query still present.** Decide and document
  explicitly whether `SearchState.query` persists across tab switches (recommend: yes, persists — matches most
  users' expectation of "my search is still there when I come back") or resets — either is defensible, but must
  be a deliberate, tested choice, not an accident of whatever `NavigationStack::push`/`pop` happens to do to
  `AppState`'s other fields (which, per Epic 1 Ticket 004's design, it should not touch at all, since
  `SearchState` is a sibling field, not something the navigation stack owns).
- **Edge case: extremely long widget summaries in the result list.** Truncation behavior (criterion 5's
  "truncated summary") must not panic on multi-byte UTF-8 boundaries — reuse the same boundary-safety discipline
  established for `highlight_spans`, ideally via a shared, tested `safe_truncate(&str, max_chars) -> &str`
  utility rather than ad hoc byte-slicing at the truncation call site.

---