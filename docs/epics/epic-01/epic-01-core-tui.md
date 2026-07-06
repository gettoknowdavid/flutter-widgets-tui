# Epic 1: Core Project Setup, Architecture & TUI Foundation

**Epic ID:** EPIC-01

**Status:** Ready for Ticketing

**Depends on:** TRD.md v1.0 (approved)

**Blocks:**

- Epic 2 (Catalog & Search),
- Epic 3 (Detail & Code Builder),
- Epic 4 (Favorites & Sync),
- Epic 5 (AI Chat),
- Epic 6 (Theming & Polish)

---

## 1. Objective

Establish the foundational Rust workspace, crate boundaries, async runtime, terminal lifecycle management, and the core
Elm-inspired event loop that every subsequent epic will build on top of. This epic produces **no user-visible features**
beyond a running, empty-but-structurally-correct TUI shell that can be launched, resized, and quit cleanly. Its purpose
is entirely architectural: it is the scaffolding that makes Sections 2, 3, and 7 of the TRD real, compilable, and
enforced by the Rust compiler rather than by convention or discipline alone.

Success for this epic means: a future engineer (or AI-assisted session) picking up Epic 2 can start writing
`SearchService` and `CatalogRepository` code on day one without first arguing about workspace layout, error types, or
how the render loop talks to background tasks — because all of that is already decided, implemented, and tested.

## 2. Scope

This epic covers, and is strictly limited to:

- **Cargo workspace scaffolding**: creation of the 5-crate workspace (`fwt-domain`, `fwt-app`, `fwt-infra`, `fwt-tui`,
  `fwt-cli`) per TRD Section 7, with correct inter-crate dependency declarations that make the dependency rule (Section
  2.1) a compile-time guarantee.
- **CLI entrypoint**: `clap`-based argument parsing for the launch flags identified in the TRD (`--theme`, `--db-path`,
  `--config`, `--no-ai`, `--reset`), wired to a config-loading stub (full config semantics deferred; only the plumbing
  is in scope here).
- **Tokio async runtime initialization**: a multi-threaded runtime with a bounded worker pool, established in `fwt-cli`,
  that hands control to the `fwt-tui` event loop.
- **Terminal lifecycle management**: entering/exiting raw mode and the alternate screen via `crossterm`, wrapped in an
  RAII **terminal guard** that unconditionally restores the terminal — including on panic — per NFR-7.
- **Panic safety**: a `std::panic::set_hook` installed before any raw-mode operation, guaranteeing the terminal is
  restored before the default panic handler prints its message and the process exits.
- **Error handling foundation**: the two-tier strategy from TRD Section 2.5 — `thiserror`-based error enums at module
  boundaries, and a single `anyhow` (or `color-eyre`, decision below) error boundary at the outermost `main()`
  /event-loop level.
- **The cross-thread event loop**: the `tokio::select!`-driven loop polling terminal input (`crossterm` async event
  stream), a background-task result channel (`tokio::mpsc`), and a tick timer — converting all three into a closed
  `Message` enum consumed by a (currently near-empty) `update()` function.
- **Minimal `AppState` skeleton and `view()` stub**: just enough of the Elm-architecture types (Section 2.2) to prove
  the loop closes end-to-end — render a static placeholder frame (e.g., the outer chrome/shell with tab bar and empty
  content pane) — without implementing any real screen logic. Real views are out of scope (Epics 2–6).
- **Logging bootstrap**: `tracing` + `tracing-subscriber` configured to a rotating file appender, explicitly never
  writing to stdout/stderr while in raw/alt-screen mode, with the redaction filter for `*token*`/`*key*`/`*secret*`
  fields established as a foundational (not bolted-on-later) concern.
- **Resize handling**: correct handling of `crossterm::event::Event::Resize` at the event-loop level, ensuring the next
  render reflows to the new terminal dimensions without artifacts.

**Explicitly out of scope for this epic** (deferred to later epics): SQLite/`rusqlite` setup and migrations (Epic 2),
any real catalog/search/favorites/chat logic, real themes beyond a single hardcoded bootstrap palette, the `AppShell`
composite widget's final visual design (a structural placeholder only), clipboard integration, and OAuth/sync.

## 3. Acceptance Criteria

Epic 1 is considered **done** when all of the following hold:

1. `cargo build --workspace` succeeds cleanly from a fresh checkout with no warnings on stable Rust.
2. `fwt-domain` has **zero** dependencies on `ratatui`, `rusqlite`, `tokio`, `reqwest`, or `crossterm` in its
   `Cargo.toml` — verified by an automated check (see Ticket 001), not just manual inspection.
3. Running the compiled binary (`fwt` or equivalent) launches into an alternate-screen, raw-mode TUI showing a static
   shell (top tab bar with 4 placeholder tabs, empty content pane, bottom status/keybinding legend) matching the outer
   chrome established by `flutter_widget_catalog_tui.html`.
4. Pressing `q` (or the configured quit keybinding) exits cleanly: terminal is restored to its original state (cooked
   mode, primary screen, cursor visible), with no leftover ANSI artifacts in the shell.
5. Triggering an intentional panic (via a debug-only test hook, e.g. `--panic-test` CLI flag) still results in a fully
   restored terminal before the process exits with a non-zero code — proving the panic hook fires before raw mode
   teardown would otherwise be skipped.
6. Resizing the terminal window (SIGWINCH on Unix, resize event on Windows) triggers a clean re-render at the new
   dimensions with no torn/stale content, verified manually and via a `TestBackend`-driven resize test.
7. The event loop demonstrably processes all three input sources (`crossterm` events, `mpsc` background-task messages,
   tick timer) through a single `Message` enum and a single `update()` function — proven by a unit test that injects a
   synthetic message of each variant and asserts `AppState` mutates as expected, without spawning a real terminal.
8. A dummy background task (e.g., a `tokio::time::sleep`-based stub simulating a future AI/DB call) can send a message
   back into the loop via `mpsc` and be observed to affect `AppState`, proving the async-to-UI-thread bridge works
   end-to-end.
9. `tracing` output is written only to a rotating log file in the OS-appropriate data/cache directory (via
   `directories`), never to stdout/stderr during normal (non-`--no-ai`/debug) operation, and log lines are confirmed to
   redact any field named with `token`, `key`, or `secret` substrings via a unit test on the redaction layer.
10. CLI flags (`--theme`, `--db-path`, `--config`, `--no-ai`, `--reset`) parse correctly via `clap` and are threaded
    into a (stub) config struct, verified by `clap`'s own test harness (`assert_cmd` or equivalent) for at least the "
    flag present" and "flag absent, default used" cases.
11. All new code in `fwt-domain` and `fwt-app` has unit test coverage for the logic introduced in this epic (guard
    construction, `update()` transitions); `fwt-tui` has at least one `insta` snapshot test of the static shell rendered
    via `TestBackend`.
12. A fresh-session code review pass (per TRD Section 12 workflow) has been performed against this Epic's tickets and
    this TRD, with no open architectural violations.

## 4. Dependencies

Crate versions below are pinned to the latest stable releases as of the TRD's authoring window; exact patch versions
should be locked in `Cargo.lock` at ticket implementation time and only bumped deliberately.

| Crate                   | Version constraint                                                            | Scope in Epic 1                                                                       |
|-------------------------|-------------------------------------------------------------------------------|---------------------------------------------------------------------------------------|
| `ratatui`               | `^0.29`                                                                       | Static shell rendering, `TestBackend` for snapshot tests                              |
| `crossterm`             | `^0.28`                                                                       | Terminal raw mode/alt screen, async event stream, resize events                       |
| `tokio`                 | `^1.40`, features = `["rt-multi-thread", "macros", "time", "sync", "signal"]` | Runtime init, `mpsc` channel, tick timer, signal handling (Unix `SIGWINCH`/`SIGTERM`) |
| `thiserror`             | `^1.0`                                                                        | Module-boundary error enums (`TerminalError`, `EventLoopError`)                       |
| `anyhow` / `color-eyre` | `anyhow ^1.0` or `color-eyre ^0.6` (decision below)                           | Top-level error boundary in `fwt-cli::main`                                           |
| `clap`                  | `^4.5`, feature = `derive`                                                    | CLI flag parsing                                                                      |
| `directories`           | `^5.0`                                                                        | OS-correct config/data/cache/log paths                                                |
| `tracing`               | `^0.1`                                                                        | Structured logging                                                                    |
| `tracing-subscriber`    | `^0.3`, features = `["env-filter", "fmt"]`                                    | Log formatting, filtering                                                             |
| `tracing-appender`      | `^0.2`                                                                        | Rotating file appender, non-blocking writer                                           |
| `insta`                 | `^1.40` (dev-dependency)                                                      | Snapshot tests of `TestBackend` buffers                                               |
| `pretty_assertions`     | `^1.4` (dev-dependency)                                                       | Unit test diff readability                                                            |
| `assert_cmd`            | `^2.0` (dev-dependency)                                                       | CLI flag parsing integration tests                                                    |

**Open decision carried into Ticket 002:** `anyhow` vs `color-eyre` for the top-level error boundary. `color-eyre`
provides materially better panic/error report formatting (span traces, colorized backtraces) which is attractive for a
developer-facing tool, but pulls in `backtrace`/`color-spantrace` and must be initialized *before* the panic hook and
terminal guard to avoid interleaving badly with raw-mode/alt-screen output. Recommendation: adopt `color-eyre`, but its
`install()` must be sequenced explicitly before terminal setup, and its default panic hook must be *chained with*, not
replace, our terminal-restoring panic hook (Ticket 002 implementation notes specify the exact composition). This is
finalized as part of Ticket 002, not left open past this epic.

## 5. Estimated Complexity

**Overall: Medium-High.** No individual ticket is algorithmically difficult, but this epic is disproportionately
risk-laden relative to its visible output: get the panic hook, terminal guard, or dependency boundaries wrong here, and
every subsequent epic inherits the flaw silently (per Risk table, TRD Section 11 — "Panic leaving terminal in raw mode"
and "Solo/AI-assisted drift"). Budget accordingly for careful review, not just implementation speed.

| Ticket                                         | Complexity  |
|------------------------------------------------|-------------|
| ticket-001-project-scaffold                    | Low-Medium  |
| ticket-002-terminal-lifecycle-and-panic-safety | Medium-High |
| ticket-003-async-runtime-and-event-loop        | Medium-High |
| ticket-004-app-state-and-update-loop           | Medium      |
| ticket-005-base-app-shell-layout               | Low-Medium  |

---