# Ticket 005: Base `AppShell` Layout & Static Render

**Epic:** EPIC-01 — Core Project Setup, Architecture & TUI Foundation
**Complexity:** Low-Medium
**Depends on:** ticket-002-terminal-lifecycle-and-panic-safety, ticket-003-async-runtime-and-event-loop,
ticket-004-app-state-and-update-loop
**Blocks:** All Epic 2–6 view implementations

---

## Description

Implement the `view(state: &AppState) -> ()` rendering path (writing into a Ratatui `Frame`) for a single, static
`AppShell` composite widget — the reusable outer chrome (top tab bar, contextual sub-header/breadcrumb placeholder, main
content pane, bottom status/keybinding legend) described in TRD Section 8.3, structurally lifted from
`flutter_widget_catalog_tui.html`'s outer `#term` frame. This ticket produces the **first pixel actually drawn to the
screen** in the whole project — closing the loop from Ticket 002 (terminal enters raw mode) through Ticket 003/004 (
event loop + state) to an actual rendered frame.

Critically, this ticket does **not** implement any real tab content (Catalog/Search/Favorites/Chat panes are all
placeholder/empty in this epic — Epic 2+ populates them) — it implements the *shell that every future view will render
into*, per TRD Section 8.3's explicit design intent that `AppShell` guarantees visual consistency "without duplication."

## Acceptance Criteria

1. An `AppShell` type/function exists in `fwt-tui/src/widgets/` (or `fwt-tui/src/app_shell.rs`) that, given an
   `&AppState` and a `ratatui::Frame`, renders: (a) a top tab bar row showing 4 placeholder tab labels (`[1] Catalog`,
   `[2] Search`, `[3] Favorites`, `[4] AI chat`) matching the wireframe's numbering/labels, with no interactive
   tab-switching logic yet (styling only — the *active* tab can be hardcoded to `Catalog` in this ticket, since real
   tab-switching state is Epic 2's `Screen` enum extension); (b) an empty, bordered main content pane occupying the
   remaining vertical space; (c) a bottom single-line status/keybinding legend bar rendering the static text
   `tab: switch pane · /: search · enter: select · esc: back · q: quit`, matching the wireframe's footer.
2. The layout is computed via Ratatui's `Layout`/`Constraint` system (not manual coordinate math) so that it correctly
   reflows when `state.terminal_size` changes — verified by rendering at two different `TestBackend` sizes and
   snapshotting both (see Testing Plan).
3. All colors/styles used reference a `Theme` struct (even though only a single hardcoded default theme exists per
   Ticket 004's `ThemeId::Default`) rather than literal `ratatui::style::Color::Rgb(...)` calls scattered through the
   shell's rendering code — establishing, from the very first rendered pixel, the theming discipline TRD Section 8.2
   mandates ("all `fwt-tui/src/views/*` code references `theme.accent`, `theme.border`, etc., never literal colors"). A
   minimal `Theme` struct with a handful of semantic fields (`background`, `border`, `accent`, `muted_text`, `text`) is
   defined for this purpose — full theme definitions (Catppuccin, etc.) remain Epic 6 scope.
4. The main content pane, when no real screen content exists yet (this epic's entire scope), renders a clearly-labeled
   placeholder (e.g., centered text: `"Epic 2+ content renders here"`) rather than being left blank — this makes the
   ticket's completion visually unambiguous and gives later epics an obvious, findable spot in the code to replace.
5. Degenerate terminal dimensions (per Ticket 004's flagged edge case — e.g., `(0, 0)` or extremely small sizes like
   `(5, 3)`) do not cause a panic in the layout/render code — verified by a `TestBackend`-driven test at a
   pathologically small size, asserting the render call returns without panicking (visual correctness at such extremes
   is not required, only panic-safety).
6. The `view()` function is called from the event loop (Ticket 003) exactly when `UpdateOutcome.redraw == true`, and is
   **not** called on every loop iteration regardless — this is the concrete wiring-up of Ticket 003's dirty-flag design,
   closing that loop with a real renderer for the first time.
7. At least one `insta` snapshot test exists rendering the full `AppShell` via `ratatui::backend::TestBackend` at a
   standard size (e.g., 100×30) and asserting the buffer contents match a committed baseline snapshot — establishing the
   pattern every future view-rendering ticket will follow per TRD Section 10's presentation-layer testing strategy.
8. Unicode/box-drawing characters used for borders/separators (mirroring the wireframe's `┌─...─` styling) render
   correctly under Ratatui's default `Borders`/`BorderType` primitives — prefer Ratatui's built-in border rendering over
   hand-rolled Unicode string construction, reserving custom Unicode art only for cases the built-ins can't express, to
   reduce the surface area for encoding/alignment bugs.
9. No Nerd Font-specific glyphs (e.g., the wireframe's `ti ti-search`/`ti ti-arrow-left` icon font references, which are
   a web-only convenience) are used in this ticket — TRD Section 8.2's Nerd-Font-with-ASCII-fallback iconography system
   is explicitly Epic 6 scope; this ticket uses plain Unicode/ASCII labels only (e.g., a literal `[?]` or the word "
   search" rather than any icon glyph), avoiding a premature, unreviewed iconography decision.

## Implementation Details & Design Notes

- **This is the first and most important precedent-setting rendering ticket in the project** — every subsequent view (
  Catalog, Search, Detail, Chat, Favorites, Settings) will follow the pattern established here for how a view function
  receives `&AppState`/`&Theme` and writes into a `Frame` region. Prioritize clarity and consistency of this function
  signature/convention over any cleverness, since TRD Section 12's ticket-by-ticket workflow means future isolated
  sessions will pattern-match against this file more than they'll re-read the TRD.
- Suggested function shape (to be treated as the convention, not just this ticket's local choice):

```rust
  pub fn render_app_shell(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) -> Rect {
    // Splits `area` into tab-bar / content / status-bar regions,
    // renders the tab bar and status bar directly,
    // and returns the inner content `Rect` for the caller (or a
    // dispatched view function, from Epic 2 onward) to render into.
}
```

Returning the inner content `Rect` (rather than the shell rendering the content pane's contents itself) is the key
structural decision that fulfills TRD Section 8.3's "each view renders *into*" the shell — `AppShell` owns the chrome,
not the content.

- **Theme struct placement:** define the minimal `Theme` struct in `fwt-tui/src/theme/mod.rs` (per TRD Section 7's
  directory listing) even though only `mod.rs`'s default implementation exists in this ticket — this pre-establishes the
  file Epic 6 will populate with `catppuccin.rs`, `gruvbox.rs`, etc., without requiring a restructure.
- **Snapshot test discipline:** `insta` snapshots should be reviewed and explicitly accepted (`cargo insta review`) as
  part of this ticket's own PR, and the committed baseline snapshot file should be treated as intentionally
  version-controlled documentation of the shell's exact appearance — any future ticket that changes this snapshot's
  output must do so consciously (via `cargo insta review` again), which is precisely the regression-catching value TRD
  Section 10 calls for.
- **Layout constraint choices:** use `Constraint::Length(1)` for the single-line tab bar and status bar rows, and
  `Constraint::Min(0)` for the content pane, via `Layout::vertical([...])` — this is the simplest correct approach and
  avoids hardcoding pixel/cell heights that would fight terminal resizing.
- **Avoid premature abstraction:** do not build a generic "widget registry" or plugin system for tabs/views in this
  ticket just because 4 tabs are visually present — Epic 2 will introduce the real `Screen`-to-view dispatch logic; this
  ticket's tab bar is deliberately static/non-interactive, per the epic's stated scope boundary.

## Folders / Files Impacted

    crates/fwt-tui/
    └── src/
    ├── app.rs                          # MODIFIED — call render_app_shell() when redraw flag set
    ├── theme/
    │   └── mod.rs                      # NEW — minimal Theme struct, Theme::default()
    └── widgets/
    └── app_shell.rs                    # NEW — render_app_shell()
    tests/snapshots/
    └── app_shell_default_100x30.snap   # NEW — insta baseline

## Testing Plan

- **Snapshot tests (`insta` + `TestBackend`):** baseline render at 100×30 (acceptance criterion 7); a second snapshot at
  a narrower/shorter size (e.g., 60×20) to prove reflow behaves sensibly (criterion 2); a third at a pathological size (
  e.g., 5×3) asserting no panic, with the actual visual snapshot content not being meaningfully asserted beyond "did not
  crash" (criterion 5).
- **Unit tests on layout math (if factored separately from rendering):** if the `Constraint`-based split logic is
  extracted into a small pure function returning computed `Rect`s, unit-test it directly against a range of input sizes
  including edge cases, independent of full Ratatui rendering.
- **Manual QA:** visually confirm, across at least two real terminal emulators (per Ticket 002's compatibility matrix),
  that the rendered shell's borders, spacing, and text are legible and undistorted, and that resizing the real terminal
  window live-reflows correctly (extending Ticket 003's resize-handling test with an actual visual check now that there'
  s something to see).

## Potential Risks / Edge Cases

- **Risk: `TestBackend` snapshot brittleness.** Snapshot tests of exact buffer contents are sensitive to any styling
  tweak, meaning routine future changes will require snapshot re-acceptance — this is an accepted, intentional tradeoff
  per TRD Section 10 ("catches unintended visual regressions per-ticket"), but should be called out to reviewers so an
  `insta` diff in a future PR isn't mistaken for a bug when it's an expected, reviewed change.
- **Risk: Unicode box-drawing character width assumptions.** Some terminal/font combinations render certain Unicode
  characters as double-width, which can subtly misalign borders. Prefer Ratatui's built-in `Borders`/`BorderType`
  enums (which are tested against this class of issue upstream) over hand-authored Unicode border strings, per
  implementation note above, specifically to minimize this risk's surface area within this ticket's scope.
- **Edge case: extremely small terminal sizes in real-world use** (a user shrinking their terminal to a sliver). This
  ticket only guarantees *panic-safety* at pathological sizes, not usability — document this explicitly as a known,
  accepted MVP limitation; a future polish ticket (Epic 6 candidate) could add a minimum-size warning message, but that
  is out of scope here.
- **Edge case: color/theme rendering under a `NO_COLOR`-respecting or basic-16-color terminal.** Full
  capability-detection down-mapping is Epic 6/TRD Section 8.2 scope, but since this ticket introduces the *first* actual
  color output, verify manually that the hardcoded default `Theme`'s colors at least degrade non-catastrophically (
  readable, not invisible-on-invisible) on a basic-16-color terminal, flagging any severe legibility issue found as a
  fast-follow note for Epic 6 rather than attempting a full capability-detection system now.

---