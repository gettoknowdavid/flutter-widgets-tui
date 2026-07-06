# Ticket 003: Tokio Runtime Initialization & Cross-Thread Event Loop

**Epic:** EPIC-01 — Core Project Setup, Architecture & TUI Foundation
**Complexity:** Medium-High
**Depends on:** ticket-001-project-scaffold, ticket-002-terminal-lifecycle-and-panic-safety
**Blocks:** ticket-004-app-state-and-update-loop, ticket-005-base-app-shell-layout

---

## Description

Implement the `tokio`-powered async runtime and the central event loop described in TRD Section 2.3, wiring together
three concurrent input sources — `crossterm`'s async terminal event stream, a `tokio::mpsc` channel carrying results
from background async tasks, and a tick timer — into a single `tokio::select!`-driven loop. This loop is the literal
embodiment of TRD Section 2.2's unidirectional data flow: raw inputs become `Message`s, `Message`s are the *only* thing
that can mutate state (via `update()`, implemented fully in Ticket 004), and any side effects are dispatched as
non-blocking background tasks whose results re-enter via the same channel.

This ticket is explicitly scoped to the **plumbing**, not the business logic: the `Message` enum defined here will be
extended by every later epic, but its input-source-to-channel wiring, and the non-blocking guarantee that the render/UI
thread is never blocked by a background task, must be fully correct and tested now.

## Acceptance Criteria

1. `fwt-cli::main()` constructs a `tokio` multi-threaded runtime with an explicitly bounded worker thread count (not the
   unbounded default) — e.g., `tokio::runtime::Builder::new_multi_thread().worker_threads(N).enable_all().build()`, with
   `N` a small, documented constant (e.g., 2–4) justified by NFR-5's memory footprint budget and the fact that this app'
   s background work (future DB writes, HTTP calls) is I/O-bound, not CPU-bound, and does not need a large thread pool.
2. A `tokio::sync::mpsc::channel::<Message>(capacity)` is constructed with a bounded, documented capacity (not
   `unbounded_channel`, to provide natural backpressure per NFR-4's render-budget concerns — a flood of background
   messages must not be able to unboundedly queue and later overwhelm a single render pass).
3. The event loop's core structure is a `tokio::select!` block polling, at minimum: (a) the `crossterm` event stream (
   `crossterm::event::EventStream`, via its `futures::Stream` implementation) for terminal input/resize/paste events; (
   b) the `mpsc::Receiver<Message>` for background-task-originated messages; (c) a `tokio::time::interval` tick (
   frequency documented and justified — e.g., 250ms, sufficient for spinner/connectivity-status animation per TRD
   Section 2.4 without meaningfully impacting NFR-5).
4. Every branch of the `select!` converts its raw input into a `Message` enum variant (defined in `fwt-app::state`, per
   Ticket 004, but the *conversion functions* — e.g., `Message::from_crossterm_event`, `Message::Tick` — are established
   in this ticket as the seam between raw I/O and domain messaging) before any further processing occurs; no branch
   performs business logic inline inside the `select!` block itself.
5. A `CommandExecutor` (or equivalently named dispatch mechanism) exists that accepts `Command` values (the side-effect
   descriptions `update()` will emit, per TRD Section 2.2) and spawns them as `tokio::task::spawn`'d async tasks, each
   of which sends its eventual `Message` result back through the same `mpsc::Sender<Message>` clone — proving the full
   round-trip (`Command` out, `Message` back in) works, exercised in this ticket via a dummy `Command::Noop` or
   `Command::SimulatedDelay(Duration)` that just sleeps and returns a `Message::DebugTaskCompleted` variant.
6. The render/UI thread (i.e., the task running the `select!` loop and calling `view()`) is **never blocked** by a
   `Command`'s execution — proven by a test that dispatches a deliberately slow (e.g., 2-second) simulated `Command` and
   asserts the event loop continues processing new terminal input events (simulated) during that window, rather than
   stalling.
7. A dirty-flag or equivalent change-detection mechanism exists so that `view()`/render only occurs when `AppState`
   actually changed as a result of the most recent `Message`, not on every tick — directly implementing TRD Section 2.3
   step "e" and supporting NFR-4. A `Message::Tick` that causes no state change (e.g., no active spinner/animation) must
   **not** trigger a re-render.
8. `crossterm::event::Event::Resize(width, height)` is handled as a first-class `Message` variant that updates a
   `terminal_size: (u16, u16)` field in `AppState`, always marking the dirty flag (a resize always necessitates a
   re-render regardless of other state) — laying groundwork for Ticket 005's layout to consume this.
9. Graceful shutdown (triggered by a `Message::Quit` variant, itself produced by the configured quit keybinding,
   `Ctrl+C`/`SIGINT`/`SIGTERM` per Ticket 002's signal-handling note, or an unrecoverable startup error) causes the
   `select!` loop to exit cleanly, allowing `main()` to proceed to `TerminalGuard`'s drop and any final flush logic (
   flush logic itself is a stub/no-op in this epic, since there's no SQLite yet — but the shutdown *sequencing* must be
   correct and extensible).
10. The event loop and its message-conversion functions are unit-testable **without spawning a real terminal or real
    tokio runtime thread pool** where feasible — i.e., the pure conversion logic (`crossterm::Event` → `Message`) and
    the `select!` loop's *decision logic* (excluding the actual OS-level polling) are factored so `#[tokio::test]` (
    single-threaded test runtime) can drive them with synthetic inputs.
11. All background task spawning goes through the one `CommandExecutor`/dispatch point — no ad hoc `tokio::spawn` calls
    scattered through presentation code — so that future instrumentation (tracing spans, cancellation-on-quit) has a
    single choke point to hook into.

## Implementation Details & Design Notes

- **Non-blocking channel discipline is the core architectural guarantee of this ticket**, per the epic's explicit
  directive. Concretely this means: the `select!` loop task must never `.await` a `Command`'s actual work directly — it
  only ever `.await`s the *next available message from any of the three sources*, and dispatches new work via
  `tokio::spawn` (fire-and-forget from the loop's perspective, with results returning asynchronously via the channel).
  Reviewers should treat any `.await` inside the `select!` arms that isn't one of the three polled sources themselves as
  a defect.
- **Channel capacity choice:** document the reasoning inline — a bounded channel (e.g., capacity 32) is preferred over
  unbounded specifically because it gives a concrete, testable backpressure point; if background tasks (in later epics —
  AI streaming chunks, DB writes) could plausibly produce messages faster than the render loop drains them, a bounded
  channel surfaces that as an observable `send().await` delay on the producer side rather than unbounded memory growth.
  Revisit this constant, not the unbounded-vs-bounded architecture, if a later epic's profiling shows the capacity is
  wrong.
- **`crossterm::event::EventStream` integration:** this requires the `crossterm` `event-stream` feature flag (update
  `fwt-tui/Cargo.toml` accordingly) and pulls in `futures-core`/`futures-util` as a transitive necessity for `Stream`
  combinators used in the `select!` macro — acceptable, but note this dependency addition explicitly in the PR/ticket
  close-out so it doesn't appear as an unexplained drive-by dependency bump to a reviewer.
- **Tick interval as a `Message`, not a special case:** resist any temptation to special-case the tick timer outside the
  `Message`/`update()` flow (e.g., directly triggering animation state mutation from inside the `select!` arm) — route
  it through `Message::Tick(Instant)` and let `update()` (Ticket 004) decide what, if anything, changes, preserving
  the "single function mutates state" invariant that makes the architecture testable (TRD Section 2.2).
- **Dirty-flag implementation:** the simplest correct approach is for `update()` to return an explicit `bool` (or a
  small `UpdateOutcome { commands: Vec<Command>, redraw: bool }` struct) rather than inferring "did state change" via
  `PartialEq` on the whole (potentially large, later-on) `AppState` — an explicit signal from `update()` is cheaper and
  avoids requiring every future state field to be cheaply comparable. Establish this return-shape convention now since
  Ticket 004 and every later epic's `update()` extensions depend on it.
- **Signal handling integration (carried from Ticket 002):** `tokio::signal::ctrl_c()` and, on `#[cfg(unix)]`,
  `tokio::signal::unix::signal(SignalKind::terminate())`, should be additional arms in the same top-level `select!` (or
  a nested task funneling into the same `mpsc` sender) producing `Message::Quit`, ensuring there is exactly one shutdown
  code path, not two divergent ones (keyboard-quit vs. signal-quit).
- **Where this code lives:** the `select!` loop itself belongs in `fwt-tui/src/app.rs` per TRD Section 7's directory
  listing ("event loop, terminal guard, panic hook"); the `Message`/`Command` enum *definitions* belong in
  `fwt-app/src/state.rs` (domain-adjacent application types, per TRD Section 2.2), meaning `fwt-tui` depends on
  `fwt-app` for these types — consistent with Ticket 001's dependency rule. The `CommandExecutor` dispatch logic is
  arguably application-layer orchestration; place it in `fwt-app/src/state.rs` or a new `fwt-app/src/executor.rs`,
  taking a generic `mpsc::Sender<Message>` and a set of injected port-trait objects (even if those ports are still stubs
  in this epic) so later epics can extend it without restructuring.

## Folders / Files Impacted

    crates/fwt-app/
    ├── Cargo.toml              # MODIFIED — tokio dependency confirmed
    └── src/
    ├── lib.rs                  # MODIFIED — expose state, executor modules
    ├── state.rs                # NEW — Message, Command enums (minimal variants for this epic)
    └── executor.rs             # NEW — CommandExecutor / dispatch logic
    crates/fwt-tui/
    ├── Cargo.toml              # MODIFIED — crossterm "event-stream" feature, futures-util
    └── src/
    ├── app.rs                  # NEW — the select! event loop, ties terminal.rs + fwt-app together
    └── lib.rs                  # MODIFIED — expose app::run_event_loop or similar

## Testing Plan

- **Unit tests (`fwt-app/src/state.rs`):** table-driven (`rstest`) tests converting synthetic `crossterm::Event` values
  into expected `Message` variants; tests asserting `Message::Tick` with no active animation state yields
  `redraw: false` from a stub `update()`, while `Message::Resize` always yields `redraw: true`.
- **Unit tests (`fwt-app/src/executor.rs`):** using a fake in-memory channel pair, dispatch a `Command::SimulatedDelay`
  and assert the corresponding `Message` arrives on the receiver after the expected delay, using `tokio::time::pause()`/
  `advance()` (Tokio's test-time utilities) rather than real wall-clock sleeps, keeping the test fast and deterministic.
- **Integration test (`fwt-tui`, `#[tokio::test]`):** construct the event loop with a mocked/synthetic input source (a
  test-only `Stream` of fabricated `crossterm::Event`s fed in via an in-memory channel instead of a real TTY)
  interleaved with a long-running dummy `Command`, asserting that synthetic "keypress" messages are still processed (
  i.e., `update()` is still invoked, and a redraw flag set) *during* the dummy command's simulated execution window —
  this is the concrete test for acceptance criterion 6 (non-blocking guarantee).
- **Manual verification:** run the compiled binary, hold down a key or resize rapidly while a debug-only artificial slow
  `Command` (a `--simulate-slow-task` debug flag, optional but recommended for manual QA) is in flight, and visually
  confirm the UI remains responsive.

## Potential Risks / Edge Cases

- **Risk: `tokio::select!`'s inherent cancellation-safety pitfalls.** Not all futures are safe to use inside `select!`
  if they're re-polled from scratch on each loop iteration (a classic Tokio footgun — e.g., a `Sleep` future recreated
  every iteration never actually elapses). Mitigate by holding long-lived futures (the `EventStream`, the `interval`,
  the channel receiver) as `pin!`'d/owned locals *outside* the loop body and only `.await`ing references to them inside
  `select!`, per standard Tokio idioms — call this out explicitly in code comments since it's a subtle, easy-to-regress
  correctness property for a future session to break unknowingly.
- **Risk: unbounded background task growth.** Even with a bounded *channel*, nothing in this ticket currently bounds the
  number of concurrently in-flight `tokio::spawn`'d tasks themselves (only the results queue is bounded). For Epic 1's
  scope (dummy/no real background work) this is not yet a real risk, but flag explicitly in code comments as a concern
  to revisit once Epic 5 (AI chat, potentially long-lived streaming tasks) lands — e.g., consider a `Semaphore`-based
  concurrency cap at that point.
- **Edge case: terminal resize storms.** Rapid, repeated resize events (some terminals emit many in quick succession
  during a drag-resize) could cause excessive re-renders. Consider (implementation optional in this ticket, but at least
  discussed/documented) a debounce on `Message::Resize` if manual testing reveals visible flicker — do not over-engineer
  this preemptively without evidence it's needed, per the epic's "boring and conservative" architecture philosophy (TRD
  Section 2.1).
- **Edge case: `Ctrl+C` racing with an in-progress `Command`.** Confirm (and test) that a `Message::Quit` arriving while
  a background task is still in flight does not panic or deadlock the executor — the simplest correct behavior for this
  epic is "let in-flight tasks be dropped/cancelled implicitly by process exit," explicitly documented as the MVP
  behavior, with graceful in-flight-task draining deferred as a Future Feature note if it later proves necessary (e.g.,
  for in-progress SQLite writes in Epic 2+, where a documented flush-before-quit step will need real handling, not just
  this ticket's stub).

---