# Ticket 002: Terminal Lifecycle Management & Panic Safety

**Epic:** EPIC-01 — Core Project Setup, Architecture & TUI Foundation

**Complexity:** Medium-High

**Depends on:** ticket-001-project-scaffold

**Blocks:** `ticket-003-async-runtime-and-event-loop`, `ticket-005-base-app-shell-layout`

---

## Description

Implement the RAII **terminal guard** and panic hook that together guarantee NFR-7 ("No panic should be able to... leave
the terminal in a broken (raw-mode-stuck) state"). This is arguably the single highest-trust-impact piece of
infrastructure in the entire application per the TRD's own risk table (Section 11: "Panic leaving terminal in raw
mode... high user-trust impact"). It must be implemented, and tested against an *actual* injected panic, in this
ticket — not assumed to "just work" from `crossterm`/`ratatui` defaults.

This ticket also finalizes the `anyhow` vs `color-eyre` decision left open in the Epic (Section 4) and establishes the
two-tier error handling strategy (TRD Section 2.5) at the module level (`thiserror`) and the top-level boundary (
`fwt-cli::main`).

## Acceptance Criteria

1. A `TerminalGuard` type exists in `fwt-tui` (e.g., `fwt-tui/src/terminal.rs`) that, on construction (
   `TerminalGuard::enter()`), enables raw mode (`crossterm::terminal::enable_raw_mode()`) and enters the alternate
   screen (`crossterm::execute!(stdout, EnterAlternateScreen)`), returning a `Result<Self, TerminalError>`.
2. `TerminalGuard` implements `Drop`, and its `drop` implementation unconditionally attempts to disable raw mode and
   leave the alternate screen, swallowing (but `tracing::error!`-logging) any error from the restoration calls
   themselves — a failure to restore must never itself panic during unwind.
3. Before any `TerminalGuard` is constructed, `main()` installs a panic hook (`std::panic::set_hook`) that: (a) performs
   the same terminal-restoration steps as `Drop` (in case the panic occurs in a context where `Drop` won't run cleanly,
   and as defense-in-depth alongside it), and (b) then delegates to the previously-installed hook (captured via
   `std::panic::take_hook()` before overriding) so that the original panic message/backtrace is still printed to the
   now-restored, normal-mode terminal.
4. If `color-eyre` is adopted (per Epic Section 4 recommendation), its `color_eyre::install()` is called **before** the
   custom panic hook is installed, and the composition is such that `color-eyre`'s enhanced panic report still prints
   correctly to a *restored* terminal — verified by the panic-injection test (criterion 8) actually inspecting captured
   stderr output, not just process exit code.
5. A `TerminalError` enum (via `thiserror`) in `fwt-tui` covers at minimum: `EnableRawModeFailed`,
   `DisableRawModeFailed`, `EnterAltScreenFailed`, `LeaveAltScreenFailed`, `BackendInitFailed`, each wrapping the
   underlying `std::io::Error` or `crossterm` error via `#[from]`/`#[source]`.
6. `fwt-cli::main()` returns `color_eyre::Result<()>` (or `anyhow::Result<()>`, per the finalized decision) and is the *
   *only** place in the codebase where the top-level error boundary catches and formats a fatal error for user display —
   all lower layers propagate typed `thiserror` errors upward via `?` and `From` conversions, never calling `.unwrap()`/
   `.expect()` on fallible terminal or I/O operations (panics are reserved strictly for genuine programmer-invariant
   violations, per TRD Section 2.5).
7. A debug-only CLI flag `--panic-test` (gated behind `#[cfg(debug_assertions)]` or a `panic-test` Cargo feature, *
   *never compiled into release builds**) triggers an intentional `panic!("intentional test panic")` *after* the
   `TerminalGuard` has entered raw mode/alt screen, specifically to exercise the panic hook path end-to-end.
8. An integration test (using `assert_cmd`, spawning the actual compiled binary as a subprocess with `--panic-test`)
   asserts: (a) the process exits with a non-zero, non-success code; (b) stderr contains the panic message; (c) —
   critically, this is the hard part — the test verifies terminal state was restored. Since a subprocess's raw-mode
   changes apply to *its* controlling TTY and are not trivially observable from the parent `assert_cmd` process, this is
   verified indirectly: the subprocess, immediately after the panic hook's restoration logic runs (via a `tracing` log
   line or a temp-file marker written by the restoration code itself, *not* by the panic message), confirms the
   restoration function was actually invoked and returned successfully before the process exit — this is an acceptable,
   documented proxy for true TTY-state verification, which is inherently hard to assert from an automated test harness.
   Document this limitation explicitly in the test's doc comment.
9. Resize signal handling (`SIGWINCH` on Unix via `tokio::signal::unix::signal(SignalKind::window_change())`, and
   `crossterm::event::Event::Resize` as the cross-platform primary mechanism) does not require raw-mode re-entry or
   guard reconstruction — the guard's lifetime spans the whole session; resize only affects the next `view()` render's
   computed layout dimensions (verified fully in Ticket 003/005, but the guard design in this ticket must not preclude
   it).
10. All terminal setup/teardown code paths are exercised by at least one non-interactive unit test using
    `ratatui::backend::TestBackend` where possible (i.e., the guard's *logic* — hook installation/removal ordering,
    error propagation — is unit-testable independent of a real TTY; the actual `crossterm` raw-mode calls are
    integration-tested only, per criterion 8, since they require a real terminal or are conditionally skipped in
    headless CI with a documented `#[ignore]`/CI-detection guard).
11. Every fallible terminal operation is logged via `tracing` at an appropriate level (`error!` for restoration
    failures, `debug!` for normal enter/exit) — this logging must itself never write to stdout/stderr while inside raw
    mode/alt screen (per TRD Section 3's `tracing` note), only to the rotating file appender established in this ticket
    or Ticket 003.

## Implementation Details & Design Notes

- **Composition order matters and must be documented inline as code comments, not just in this ticket**, since a future
  isolated session touching this file without full TRD context is exactly the failure mode this file exists to prevent (
  TRD Section 11, "Solo/AI-assisted drift" risk):
    1. Install logging (`tracing-subscriber` + file appender) first — nothing before this point should log anywhere.
    2. Call `color_eyre::install()` (or configure `anyhow`).
    3. Capture the default/existing panic hook via `std::panic::take_hook()`.
    4. Install the custom panic hook, which restores the terminal *then* calls the captured hook.
    5. Only *then* construct `TerminalGuard::enter()`.
    6. On normal or error exit from `main`, `TerminalGuard`'s `Drop` fires as the stack unwinds — this must be
       structured so `Drop` runs even on an early `?`-propagated error return from `main` (straightforward in Rust as
       long as the guard is a local binding in scope, but call this out explicitly since a refactor that moves guard
       construction behind a `Box`/`Option` wrapper could accidentally delay/skip drop timing).
- **Linux/Unix-first robustness, per the ticket's explicit architectural directive:** prioritize correctness against
  `xterm`-family terminals, `tmux`/`screen` multiplexers, and raw Linux VT/console behavior first (these are the
  environments a Flutter/Rust developer audience is statistically most likely to run this tool in during initial
  adoption), while keeping the `crossterm` abstraction itself cross-platform. Concretely:
    - Explicitly test (manually, per the Epic's manual QA note, and via CI where feasible) behavior under `tmux` and
      `screen`, where alternate-screen and raw-mode signaling can interact unexpectedly with the multiplexer's own
      screen management (e.g., nested alternate screens). `crossterm` handles most of this, but the terminal guard's
      restoration logic should be defensive — attempt restoration even if a preceding step reports "already disabled"/"
      already left," rather than short-circuiting, since multiplexers can leave state slightly inconsistent with what
      `crossterm` believes.
    - On Unix, additionally register a handler (or rely on `crossterm`'s built-in behavior, verified not assumed) for
      `SIGTERM`/`SIGINT` to ensure `Ctrl+C`/external kill signals *also* pass through the same restoration path rather
      than bypassing it — an abrupt `SIGKILL` cannot be caught (acceptable, documented limitation) but `SIGTERM`/
      `SIGINT` must not leave a broken shell. Use `tokio::signal::ctrl_c()` and
      `tokio::signal::unix::signal(SignalKind::terminate())`, feeding both into the same event loop `Message` handling
      introduced in Ticket 003 as a graceful-shutdown message, rather than a separate ad hoc exit path.
    - Windows Terminal/ConHost signal semantics differ; `crossterm`'s cross-platform event stream abstracts most of
      this, but do not assume Unix signal-handling code paths are exercised or meaningful on Windows — gate
      Unix-signal-specific code behind `#[cfg(unix)]` explicitly.
- **`color-eyre` vs `anyhow` — final decision for this ticket:** adopt `color-eyre` for its developer-facing diagnostic
  quality (this is a developer tool; its own crash reports should model good Rust error-reporting practice), with the
  explicit caveat from Epic Section 4 that its panic hook must be *chained*, not replacing, the terminal-restoration
  hook — `color_eyre::install()` installs its own panic hook internally, so the correct sequencing is: call
  `color_eyre::install()` **first**, then immediately re-capture and re-wrap *its* installed hook with the
  terminal-restoration wrapper (i.e., the terminal-restoration hook becomes the outermost layer, calling into
  `color-eyre`'s hook, not the reverse). Get this ordering wrong and either the terminal fails to restore before the (
  nicely formatted but garbled by raw-mode) panic report prints, or the report loses its formatting. This exact ordering
  must be covered by criterion 8's test.
- Keep `TerminalGuard` in `fwt-tui`, not `fwt-infra` — despite touching "infrastructure-like" concerns (raw OS terminal
  state), it is conceptually part of the presentation layer's shell lifecycle per TRD Section 2.1's diagram (
  Presentation Layer includes "the app shell"), and depending on `fwt-infra` from `fwt-tui` would violate the dependency
  rule established in Ticket 001.

## Folders / Files Impacted

    crates/fwt-tui/
    ├── Cargo.toml                 # MODIFIED — no new deps beyond ticket-001's
    └── src/
    ├── lib.rs                     # MODIFIED — wire terminal module
    ├── terminal.rs                # NEW — TerminalGuard, TerminalError
    └── panic_hook.rs              # NEW — panic hook composition logic
    crates/fwt-cli/
    ├── Cargo.toml                 # MODIFIED — add color-eyre, tracing-appender
    └── src/
    ├── main.rs                    # MODIFIED — bootstrap sequence, --panic-test flag
    └── logging.rs                 # NEW — tracing-subscriber + rotating file appender setup
    tests/
    └── integration/
    └── panic_safety.rs            # NEW — assert_cmd-based panic-injection test

## Testing Plan

- **Unit tests (`fwt-tui/src/terminal.rs`):** test `TerminalError` variant construction/conversion from underlying
  `crossterm`/`io::Error` types using table-driven cases (`rstest`); test that `TerminalGuard`'s `Drop` logic (extracted
  into a plain function callable without a real guard instance, for testability) is idempotent when called twice (
  simulating both explicit and drop-triggered restoration).
- **Unit tests (`panic_hook.rs`):** test hook-composition logic in isolation by installing a fake/mock "previous hook" (
  a closure incrementing a shared `AtomicBool`/counter) and asserting the composed hook calls both the restoration logic
  and the captured previous hook, in the correct order — this can be tested without ever entering real raw mode, by
  having the restoration logic itself be an injectable trait/closure in tests.
- **Integration test (`tests/integration/panic_safety.rs`):** the `--panic-test` end-to-end subprocess test described in
  acceptance criterion 8, run via `assert_cmd`. Marked to skip gracefully (not fail) in headless/no-TTY CI environments
  where entering raw mode is impossible (detect via `crossterm`'s `is_tty` check or an environment variable CI
  convention), logging a clear skip reason rather than a false failure.
- **Manual QA (documented in `docs/` per TRD Section 10):** run the built binary under iTerm2, Alacritty, Kitty, Windows
  Terminal, GNOME Terminal, and inside `tmux`/`screen` sessions; trigger `--panic-test` and `Ctrl+C` in each; visually
  confirm shell is left in a sane state. This manual matrix is required at least once before this ticket is marked
  complete, even though it cannot be fully automated (NFR-6 compatibility risk, TRD Section 11).

## Potential Risks / Edge Cases

- **Risk: panic during the panic hook itself** (e.g., the terminal-restoration call inside the hook itself panics,
  causing a double-panic abort). Mitigate by wrapping all restoration calls inside the hook in
  `std::panic::catch_unwind` or, more simply, ensuring every fallible call inside the hook uses `let _ = ...;` on
  `Result`s rather than `?`/`unwrap`, since a panic hook has no sensible propagation target anyway.
- **Risk: nested alternate-screen state under `tmux`/`screen`** where the multiplexer itself manages a form of
  alternate-screen semantics, potentially causing the restored "normal" screen to still look wrong from the user's
  perspective (this is a `tmux` config/version-dependent nuance, not something this app can fully control). Document as
  a known limitation in the manual QA checklist rather than attempting exhaustive mitigation.
- **Risk: `SIGKILL`/hard crashes (OOM killer, `kill -9`)** cannot be intercepted by any userspace panic hook — the
  terminal will be left broken in these cases. This is an accepted, explicitly documented limitation (no software-only
  solution exists); do not attempt exotic mitigations (e.g., a watchdog process) as part of this ticket — out of scope
  and disproportionate to the risk's actual likelihood for this application's use case.
- **Edge case: `--panic-test` accidentally shipping in a release build.** Mitigate via `#[cfg(debug_assertions)]` gating
  *and* an explicit compile-time assertion/test in CI that the flag is absent from `--help` output of a `--release`
  build, since debug-assertion gating alone can be inadvertently defeated by certain release-with-debug-assertions build
  profiles.
- **Edge case: resize events arriving during panic unwind.** Not expected to be a realistic race in practice (panic
  unwind is effectively synchronous relative to the single-threaded terminal-guard lifecycle at this stage), but flag
  for re-examination once Ticket 003's async event loop is layered on top, since that introduces genuine concurrency.

---