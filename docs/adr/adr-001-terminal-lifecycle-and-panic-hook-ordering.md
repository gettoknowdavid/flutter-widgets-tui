# ADR-001: Terminal Lifecycle, Panic Hook Composition, and Error Boundary Strategy

**Status:** Accepted

**Date:** (Epic 1, Ticket 002)

**Supersedes:** Epic Section 4's open decision ("anyhow vs color-eyre")

**Related:** TRD Section 2.3 (Rendering Loop), 2.5 (Error Handling), NFR-7 (Crash resilience)

---

## Context

TRD Section 2.5 mandates a two-tier error strategy (`thiserror` at module boundaries, a single top-level boundary for
user-facing fatal errors) but left the exact top-level crate — `anyhow` vs `color-eyre` — as an open decision deferred
to Ticket 002 (Epic 1, Section 4). Separately, NFR-7 requires that no panic can leave the terminal in a broken
raw-mode/alt-screen state. These two concerns turned out to be coupled: whichever top-level error crate we choose
installs its own panic hook as a side effect of its `install()` call, and that interacts directly with our own
terminal-restoration panic hook. Getting the *installation order* wrong silently breaks NFR-7 regardless of which
crate is chosen — this is exactly the kind of drift TRD Section 11 warns about for isolated, session-based
development, so it is recorded here rather than left to be re-derived (or gotten wrong) by a future session.

## Decision

1. **Adopt `color-eyre`** over `anyhow` for the top-level error boundary (`fwt-cli::main() -> color_eyre::Result<()>`).
   Rationale: this is a developer-facing tool; its own crash reports should model good Rust error-reporting practice
   (span traces, readable backtraces), which `anyhow` does not provide out of the box.

2. **Installation order is fixed and must not be reordered:**
    1. `init_logging()` — tracing subscriber + rotating file appender
    2. `color_eyre::install()` — installs color-eyre's own panic hook as a side effect
    3. `panic_hook::install_panic_hook()` — takes color-eyre's hook via `take_hook()`, wraps it
    4. `TerminalGuard::enter()` — only now does raw mode / alt screen become active

   The critical property: **our terminal-restoration hook must be the outermost layer**, calling into color-eyre's
   hook, never the reverse. This is why `color_eyre::install()` runs *before* `install_panic_hook()` — if the order
   were flipped, `color_eyre::install()` would silently clobber our hook, since it installs unconditionally, and
   NFR-7 would be violated with no compiler or lint error to catch it.

3. **`TerminalGuard` lives in `fwt-tui`, not `fwt-infra`.** Despite touching raw OS terminal state (which reads as an
   "infrastructure" concern), it is conceptually part of the presentation layer's shell lifecycle per TRD Section
   2.1's architecture diagram ("Presentation Layer... the app shell"). Placing it in `fwt-infra` would force `fwt-tui`
   to depend on `fwt-infra`, violating the Ticket-001-enforced dependency rule (presentation wires to infrastructure
   only via `fwt-cli`'s composition root).

4. **Restoration logic is a single, idempotent free function** (`restore_terminal_best_effort`), called from both
   `TerminalGuard::Drop` and the panic hook — not two independently maintained copies. This directly mitigates the
   Section 11 risk of "Solo/AI-assisted drift," since a future session touching one call site without full context
   cannot silently desync the two paths.

5. **`--reset`'s confirmation prompt runs in cooked mode, before `TerminalGuard::enter()`.** A plain yes/no stdin
   prompt has no reason to run inside raw mode/alt screen, and running it first keeps the "only construct
   `TerminalGuard` once everything that should catch a panic is installed" invariant simple — there's no scenario
   where the reset prompt itself needs terminal-restoration coverage, since it never enters raw mode in the first
   place.

## Consequences

- Any future ticket touching `main()`'s bootstrap sequence must preserve this exact ordering. A reviewer seeing
  `color_eyre::install()` moved after `install_panic_hook()`, or `TerminalGuard::enter()` moved before either, should
  treat it as an architectural regression, not a stylistic change.
- The subprocess integration test (`tests/integration/panic_safety.rs`) is the primary automated guardrail for this
  ordering's correctness, via its documented proxy (a restoration-completion log line checked before asserting
  process exit). This proxy's limitation (no direct TTY-state observation from the parent test process) is accepted
  and documented in that test file directly.
- `color-eyre` pulls in `backtrace`/`color-spantrace` as transitive dependencies — a deliberate, accepted addition to
  `fwt-cli`'s dependency footprint, not a drive-by bump.
- `SIGKILL`/hard crashes remain an accepted, undocumented-away limitation: no userspace panic hook can intercept them,
  and no mitigation is planned (out of proportion to actual risk for this application's use case, per TRD Section 11).

## Alternatives Considered

- **`anyhow` for the top-level boundary:** simpler, fewer transitive deps, but noticeably worse default panic/error
  report formatting for a tool whose own crash reports are part of its developer-facing polish. Rejected.
- **Installing our panic hook before `color_eyre::install()`:** would let us skip the `take_hook()` indirection, but
  `color_eyre::install()`'s unconditional hook installation would immediately overwrite ours, permanently disabling
  terminal restoration on panic. Rejected — this was the central pitfall this ADR exists to prevent recurring.
- **Placing `TerminalGuard` in `fwt-infra`:** more conventional "infrastructure = OS boundary" categorization, but
  violates the Ticket-001 dependency rule (`fwt-tui` would need to depend on `fwt-infra`) for no compensating benefit.
  Rejected.