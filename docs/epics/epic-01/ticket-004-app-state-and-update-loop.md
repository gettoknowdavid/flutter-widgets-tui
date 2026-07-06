# Ticket 004: `AppState`, `Message`, and the Elm-Style `update()` Function

**Epic:** EPIC-01 — Core Project Setup, Architecture & TUI Foundation
**Complexity:** Medium
**Depends on:** ticket-003-async-runtime-and-event-loop
**Blocks:** ticket-005-base-app-shell-layout, all Epic 2+ tickets that extend `AppState`/`Message`

---

## Description

Flesh out the `AppState` root struct and the pure `update(state: &mut AppState, message: Message) -> UpdateOutcome`
function referenced structurally in Ticket 003, establishing the concrete skeleton that every later epic's
feature-specific sub-state (`CatalogState`, `SearchState`, `ChatState`, etc.) will plug into. This ticket does **not**
implement any of those feature sub-states' real fields or logic — it establishes the composition pattern (a root struct
of sub-structs) and implements it fully for the cross-cutting state that *does* belong in Epic 1: `NavigationStack` (
skeleton only — real breadcrumb logic lands in Epic 3, but the stack data structure itself belongs here since it's
foundational), `ActiveTheme` (a single hardcoded placeholder theme, real theming is Epic 6), `ConnectivityStatus` (the
enum and a default `Offline`/`Unknown` value; the actual `ConnectivityMonitor` adapter is Epic 5), and terminal
dimensions (from Ticket 003).

This ticket is the concrete fulfillment of TRD Section 2.2's promise that `update()` is "a pure, synchronously-testable
function" — the acceptance criteria are written to make that property provable, not just assumed.

## Acceptance Criteria

1. `AppState` (in `fwt-app/src/state.rs`) is a struct composed of clearly-named sub-fields, with explicit
   placeholder/unit-struct types for sub-states not yet implemented (e.g., `pub catalog: CatalogStatePlaceholder` — a
   zero-field marker type with a doc comment `// Epic 2` — rather than omitting the field entirely), so the *shape* of
   the root state is visible and reviewable now, even though most fields are empty.
2. `AppState` includes, fully implemented (not placeholders) in this ticket: `navigation: NavigationStack` (a `Vec`
   -backed or similar stack of an opaque `Screen` enum — with only a minimal `Screen::Shell` variant defined in this
   epic, extended later), `active_theme: ThemeId` (an enum with a single `ThemeId::Default` variant for now),
   `connectivity: ConnectivityStatus` (enum: `Offline | Online | Degraded | Unknown`, defaulting to `Unknown` at
   startup), `terminal_size: (u16, u16)`, and `should_quit: bool`.
3. `Message` (also in `fwt-app/src/state.rs`, extending Ticket 003's initial definition) includes at minimum:
   `Message::Key(crossterm::event::KeyEvent)`, `Message::Resize(u16, u16)`, `Message::Tick(std::time::Instant)`,
   `Message::Quit`, and `Message::DebugTaskCompleted` (the Ticket 003 dummy round-trip variant) — defined as a
   `#[non_exhaustive]` enum (or clearly documented as "extend here" via a comment) to signal to future epics that this
   is the designated extension point.
4. `update(state: &mut AppState, message: Message) -> UpdateOutcome` is a **synchronous, non-async, pure function** (no
   I/O, no `.await`, no direct channel access) that pattern-matches on `Message` and mutates `AppState` accordingly,
   returning `UpdateOutcome { commands: Vec<Command>, redraw: bool }` per Ticket 003's established convention.
5. `Message::Quit` sets `state.should_quit = true` and returns `redraw: false` (no need to re-render before exiting)
   with no commands.
6. `Message::Resize(w, h)` updates `state.terminal_size` and returns `redraw: true` unconditionally, even if `(w, h)` is
   unchanged from the previous value (a resize event firing implies the terminal wants a redraw regardless).
7. `Message::Tick(_)` returns `redraw: false` in this epic (no animated state exists yet to justify a redraw) — but the
   function's structure must make it obvious (via a comment and/or a currently-empty match arm ready for extension)
   where future epics (e.g., a "thinking" spinner in Epic 5) will change this to conditionally return `true`.
8. `Message::Key(key_event)` handles, at minimum, the configured quit key (default: `q`, per the wireframe's footer
   legend) by internally producing the same effect as `Message::Quit` (either by directly setting `should_quit` or by
   the key-handling arm delegating to shared logic — avoid duplicating the quit logic in two places) — all other key
   events in this epic are no-ops returning `redraw: false`, explicitly reserved for Epic 2+'s tab/navigation handling.
9. The event loop from Ticket 003 is updated to actually call this `update()` function (previously it may have only
   referenced a stub) and to honor `state.should_quit` by breaking out of the `select!` loop.
10. A `NavigationStack` type exists with, at minimum, `push(Screen)`, `pop() -> Option<Screen>`, and
    `current() -> Option<&Screen>` methods, unit-tested for basic push/pop/current-empty-stack behavior — even though no
    real screen ever gets pushed onto it yet beyond the initial `Screen::Shell`.
11. **`update()` is unit-tested exhaustively for every `Message` variant defined in this ticket**, using plain `#[test]`
    functions (no `#[tokio::test]` needed, since `update()` is synchronous) — this is the concrete proof of TRD Section
    2.2's testability claim and must not be skipped or deferred.
12. No `unsafe`, no interior mutability workarounds (`RefCell`/`Mutex` around `AppState` fields) are introduced to make
    this pattern work — `AppState` is owned exclusively by the event loop and passed as `&mut` to `update()`, full stop;
    if a future epic's reviewer sees interior mutability creeping into `AppState` fields, that is a flag for
    architectural review, not a pattern to be quietly established here.

## Implementation Details & Design Notes

- **Placeholder sub-state fields are a deliberate documentation device, not laziness.** The Epic's own design note calls
  for these to be visible in the struct shape now so that Epic 2+ tickets have an obvious, pre-agreed slot to fill
  rather than needing to restructure `AppState` (and every `match` arm in `update()` that destructures it) per epic.
  Name them explicitly with an `Epic N` comment, e.g.:

```rust
  pub struct AppState {
    pub navigation: NavigationStack,
    pub active_theme: ThemeId,
    pub connectivity: ConnectivityStatus,
    pub terminal_size: (u16, u16),
    pub should_quit: bool,
    // -- Placeholders for future epics; do not remove, extend in-place --
    pub catalog: CatalogStatePlaceholder,   // Epic 2
    pub search: SearchStatePlaceholder,     // Epic 2
    pub detail: DetailStatePlaceholder,     // Epic 3
    pub favorites: FavoritesStatePlaceholder, // Epic 4
    pub chat: ChatStatePlaceholder,          // Epic 5
}
```

- **`UpdateOutcome` vs. a bare `bool`:** confirm the struct-based return (established provisionally in Ticket 003)
  rather than a tuple, for readability at call sites and to allow painless future extension (e.g., adding a
  `toast: Option<String>` field later without changing every call site's destructuring pattern) — use named-field
  struct, not a tuple return.
- **Keybinding indirection:** hardcode `q` as the quit key literally in this ticket's `update()` implementation (
  simplest correct thing for Epic 1), but leave an explicit
  `// TODO(Epic 6 / NFR-11): route through configurable keymap.rs` comment, since TRD Section 8/NFR-11 calls for
  user-configurable keybindings — do not attempt to build the full keybinding config system prematurely in this
  foundational ticket; that is correctly scoped to a later theming/settings epic, but the seam must be visible now.
- **Where `NavigationStack`'s `Screen` enum lives:** define a minimal `Screen` enum in `fwt-app/src/state.rs` (or a new
  `fwt-app/src/navigation.rs`, aligning with TRD Section 7's `navigation.rs` file) with only `Screen::Shell` in this
  ticket; Epic 2+ will add `Screen::Catalog`, `Screen::Detail(WidgetId)`, etc. Do not conflate this domain-level
  `Screen` enum with any Ratatui-specific rendering concept — it must remain a plain data enum, importable and testable
  from `fwt-app` without pulling in `ratatui`.
- **Testability emphasis:** because `update()` is the single most important function for the "AI-assisted,
  ticket-by-ticket development" workflow (TRD Section 12) to remain safe against regressions, favor exhaustive, explicit
  `match` arms (no wildcard `_ => {}` catch-alls that could silently swallow a future epic's new `Message` variant
  without a compile error) — prefer the enum to be non-exhaustive-by-convention (a doc comment) over Rust's literal
  `#[non_exhaustive]` attribute if that attribute would cause friction for in-workspace pattern matching; the goal is
  that adding a new `Message` variant in Epic 2 causes a compiler error at every existing exhaustive `match` site that
  needs updating, not a silently-ignored new message.

## Folders / Files Impacted

    crates/fwt-app/
    └── src/
    ├── state.rs           # MODIFIED — AppState, Message, UpdateOutcome, update()
    └── navigation.rs      # NEW — NavigationStack, Screen (minimal)
    crates/fwt-tui/
    └── src/
    └── app.rs             # MODIFIED — call real update(), honor should_quit

## Testing Plan

- **Exhaustive `update()` unit tests (`fwt-app/src/state.rs` `#[cfg(test)] mod tests`):** one test per `Message` variant
  defined in this ticket, asserting both the resulting `AppState` field values and the `UpdateOutcome.redraw`/
  `.commands` values — using `pretty_assertions::assert_eq!` for readable failure diffs, and `rstest` fixtures to reduce
  boilerplate across the near-identical "construct default state, apply message, assert" pattern.
- **`NavigationStack` unit tests:** push/pop/current on empty stack (returns `None`/no panic), push-then-current returns
  the pushed value, push-pop-push-current sequence behaves as a correct LIFO stack.
- **Property-style sanity test (optional but recommended):** using a small hand-rolled sequence of randomized `Message`
  s (not full proptest/quickcheck machinery — likely overkill for this epic's scope), assert `update()` never panics
  regardless of message ordering, as a cheap fuzz-adjacent smoke test.
- **Event loop integration (extends Ticket 003's tests):** confirm that a synthetic `Message::Key` for `q` fed through
  the full (mocked-input) event loop results in the loop actually terminating, not just `should_quit` being set in
  isolation — closing the loop between "unit-correct `update()`" and "the event loop actually respects it."

## Potential Risks / Edge Cases

- **Risk: placeholder sub-state fields create false confidence.** A reviewer skimming `AppState`'s shape might assume
  more is implemented than actually is. Mitigate purely through disciplined comments (`// Epic 2`, etc.) and by ensuring
  placeholder types are visibly trivial (zero-field unit structs), not superficially fleshed-out stubs that look more
  complete than they are.
- **Risk: `update()` growing into a god-function over time.** Not a concern *yet* at Epic 1's scope (5–6 match arms),
  but flag now, in a code comment or ADR, that once per-epic sub-state logic lands, `update()`'s top-level match should
  delegate each `Screen`/feature-scoped message to a feature-specific `update_catalog()`/`update_chat()` helper rather
  than inlining all logic into one enormous function — establishing this *expectation* now (even with nothing yet to
  delegate to) makes it a much smaller, less contentious refactor later.
- **Edge case: quit key conflicting with a future text-input context.** This epic's `q`-quits-globally behav19or will
  very obviously be wrong once Epic 2's search input or Epic 5's chat input need to accept the literal character `q`
  while focused. This is explicitly **not** a defect in this ticket (there is no text input yet) but must be flagged as
  a **known, deliberate temporary behavior** to be superseded by focus-aware key routing in Epic 2/5 — document this via
  a code comment so it is not mistaken for an oversight or, worse, "working as intended" once text inputs exist.
- **Edge case: resize to zero/degenerate terminal dimensions** (some terminals or CI/test harnesses can report `(0, 0)`
  transiently). `update()`'s `Message::Resize` handling should store the value without attempting to validate/clamp it
  in this ticket (validation/clamping is a rendering-layer concern for Ticket 005 to guard against divide-by-zero or
  panics in layout math), but this hand-off point should be noted so Ticket 005 explicitly addresses it rather than
  assuming `state.terminal_size` is always sane.

---