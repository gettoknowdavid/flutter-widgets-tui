# Ticket 001: Project Scaffold — Cargo Workspace & Crate Boundaries

**Epic:** EPIC-01 — Core Project Setup, Architecture & TUI Foundation

**Complexity:** Low-Medium

**Depends on:** None (first ticket)

**Blocks:** All subsequent tickets in this epic and all later epics

---

## Description

Create the Cargo workspace skeleton exactly as specified in TRD Section 7: a root workspace manifest and five member
crates (`fwt-domain`, `fwt-app`, `fwt-infra`, `fwt-tui`, `fwt-cli`), each with the correct, minimal `Cargo.toml`
dependency graph so that the architectural dependency rule (TRD Section 2.1: Presentation → Application → Domain ←
Infrastructure) is enforced by the compiler, not by convention. Also establish the base directory layout for `assets/`,
`migrations/`, `docs/`, and `tests/` per TRD Section 7, even though most will remain empty until later epics populate
them.

This ticket produces no runnable behavior — it produces a `cargo build --workspace` that succeeds with five crates, each
containing only a placeholder `lib.rs`/`main.rs` and correct dependency declarations, plus a CI-style automated check
that a forbidden dependency was not accidentally added to `fwt-domain`.

## Acceptance Criteria

1. Root `Cargo.toml` declares a `[workspace]` with
   `members = ["crates/fwt-domain", "crates/fwt-app", "crates/fwt-infra", "crates/fwt-tui", "crates/fwt-cli"]` and a
   `[workspace.package]` table for shared metadata (`edition = "2021"`, `version`, `authors`, `license`).
2. `[workspace.dependencies]` is used to centrally pin shared crate versions (e.g., `serde`, `thiserror`) so member
   crates reference them via `{ workspace = true }` rather than re-declaring version numbers — preventing version drift
   across crates as the project grows.
3. `fwt-domain/Cargo.toml` contains **only**: `serde` (with `derive` feature, for eventual (de)serialization of domain
   types), `thiserror`, and `uuid`. It must **not** depend on `ratatui`, `crossterm`, `tokio`, `rusqlite`, `reqwest`, or
   any other I/O-bearing crate, directly or transitively through a dependency it controls.
4. `fwt-app/Cargo.toml` depends on `fwt-domain` (path dependency) plus `tokio` (for async trait signatures / `Stream`
   types used in ports) but **not** on `fwt-infra` or `fwt-tui` — application code must depend only on domain-defined
   port traits, never on concrete infrastructure or presentation types.
5. `fwt-infra/Cargo.toml` depends on `fwt-domain` (to implement its port traits) and may **not** depend on `fwt-app` or
   `fwt-tui`. It declares stub dependencies for crates it will use in later epics (`rusqlite`, `reqwest`, etc.) as
   version-pinned but the actual modules (`db/`, `ai/`, etc.) are empty placeholders in this ticket — just enough
   structure to compile.
6. `fwt-tui/Cargo.toml` depends on `fwt-app`, `fwt-domain`, `ratatui`, and `crossterm`. It must **not** depend on
   `fwt-infra` directly — presentation wires to infrastructure only indirectly via dependency injection performed in
   `fwt-cli` (see Ticket 002/003 for how concrete adapters are constructed and passed down).
7. `fwt-cli/Cargo.toml` is the only crate permitted to depend on **all four** other crates (`fwt-domain`, `fwt-app`,
   `fwt-infra`, `fwt-tui`) — it is the composition root, per Section 7's rationale.
8. An automated check exists (a small `xtask`-style script, or a `#[test]` in a `tests/architecture.rs` integration test
   using `cargo metadata` parsing) that fails the build if `fwt-domain`'s resolved dependency graph ever includes
   `ratatui`, `rusqlite`, `tokio`, `reqwest`, or `crossterm`. This is the automated enforcement referenced in Epic
   acceptance criterion #2 — it must not be a manual-inspection-only check.
9. Each crate has a minimal, compiling placeholder: `fwt-domain`, `fwt-app`, `fwt-infra` are libraries (`src/lib.rs`
   with a doc comment stating the crate's role per TRD Section 2.1, and no other code); `fwt-tui` is a library exposing
   a stub `pub fn run() -> Result<(), TuiError>` that is not yet called; `fwt-cli` is a binary (`src/main.rs`) that
   calls `fwt_tui::run()` and does nothing else.
10. Top-level directories `assets/catalog_seed/`, `migrations/catalog/`, `migrations/user/`, `docs/epics/`, `docs/adr/`,
    `tests/integration/`, `tests/snapshots/` exist (with a `.gitkeep` or minimal `README.md` placeholder each) so the
    full TRD Section 7 layout is visible in the repo from day one, even though most are unpopulated.
11. `cargo build --workspace` and `cargo test --workspace` both succeed with zero warnings (`#![deny(warnings)]` is *
    *not** mandated at this stage — full lint strictness is a Ticket 002+/Epic 6 concern — but no warnings should
    currently be emitted regardless).
12. A root `README.md` (or `docs/adr/adr-000-workspace-structure.md`) documents the dependency rule and directs future
    contributors to the automated check in criterion 8 rather than relying on memory.

## Implementation Details & Design Notes

- Use path dependencies (`fwt-domain = { path = "../fwt-domain" }`) between workspace members; do not publish these
  crates to crates.io at this stage (no `[package.publish]` needed beyond the default `false` — set `publish = false`
  explicitly on every member crate to prevent accidental `cargo publish` of internal-only crates).
- Favor `[workspace.dependencies]` centralization aggressively even in this scaffolding ticket — it is far cheaper to
  establish now than to retrofit once 5 crates each have divergent transitive version pins.
- The `tests/architecture.rs` dependency-boundary check (criterion 8) should shell out to
  `cargo metadata --format-version 1` and parse the JSON dependency graph rather than trying to hand-roll `Cargo.toml`
  parsing; this is more robust to transitive dependencies sneaking in a forbidden crate indirectly (e.g., some unrelated
  crate pulling in `tokio` as a feature-gated dep). If `cargo metadata` proves awkward inside a `#[test]`, an `xtask`
  binary crate (a common Rust monorepo pattern: a 6th, dev-tooling-only workspace member) invoked via
  `cargo xtask check-boundaries` is an acceptable alternative — document the choice in an ADR either way.
- Because this project is explicitly designed for ticket-by-ticket, session-isolated AI-assisted development (TRD
  Section 12), the placeholder `lib.rs` files in `fwt-domain`/`fwt-app`/`fwt-infra` should include a prominent
  module-level doc comment restating their layer's responsibility and dependency constraints verbatim from TRD Section
  2.1 — this is deliberate redundancy so a fresh session opening just this one file has enough context to avoid an
  architectural violation without needing to re-read the full TRD.
- Do not add `rusqlite`, `nucleo`/`fuzzy-matcher`, `reqwest`, `oauth2`, `copypasta`, or `syntect` as *active*
  dependencies yet in `fwt-infra` beyond declaring them in `Cargo.toml` if convenient — actual usage begins in later
  epics. If it simplifies review, these can be deferred entirely to Epic 2/3/4/5 tickets instead; do not let speculative
  dependency declarations cause `cargo build` slowdowns or unused-dependency warnings in this ticket. **Recommendation:
  declare only what Epic 1 tickets actually compile against; leave later crates' `[dependencies]` sections minimal or
  empty.**

## Folders / Files Impacted

    flutter-widgets-tui/
    ├── Cargo.toml                          # NEW — workspace root
    ├── crates/
    │   ├── fwt-domain/
    │   │   ├── Cargo.toml                  # NEW
    │   │   └── src/lib.rs                  # NEW — placeholder + doc comment
    │   ├── fwt-app/
    │   │   ├── Cargo.toml                  # NEW
    │   │   └── src/lib.rs                  # NEW — placeholder + doc comment
    │   ├── fwt-infra/
    │   │   ├── Cargo.toml                  # NEW
    │   │   └── src/lib.rs                  # NEW — placeholder + doc comment
    │   ├── fwt-tui/
    │   │   ├── Cargo.toml                  # NEW
    │   │   └── src/lib.rs                  # NEW — stub pub fn run()
    │   └── fwt-cli/
    │       ├── Cargo.toml                  # NEW
    │       └── src/main.rs                 # NEW — calls fwt_tui::run()
    ├── assets/catalog_seed/README.md       # NEW — placeholder
    ├── migrations/catalog/README.md        # NEW — placeholder
    ├── migrations/user/README.md           # NEW — placeholder
    ├── docs/adr/adr-000-workspace-structure.md  # NEW
    ├── tests/integration/README.md         # NEW — placeholder
    ├── tests/snapshots/README.md           # NEW — placeholder
    └── tests/architecture.rs               # NEW — dependency boundary check

## Testing Plan

- **Boundary enforcement test (`tests/architecture.rs`):** integration test invoking `cargo metadata`, parsing the
  dependency graph for `fwt-domain`, and asserting the forbidden crate list (criterion 8) is absent. This is the single
  most important test in this ticket — it is the automated guardrail the rest of the project's architectural integrity
  leans on for the entire lifetime of the codebase.
- **Compile check:** `cargo build --workspace` and `cargo check --workspace --all-targets` as a CI smoke test; no
  functional unit tests are meaningful yet since there is no logic, but the placeholder `fwt_tui::run()` stub should
  have a trivial `#[test]` asserting it returns `Ok(())` (or a defined stub error) to prove the crate wiring compiles
  and links.
- **CLI smoke test:** `assert_cmd`-based test in `fwt-cli/tests/` that runs the compiled binary with no arguments and
  asserts it exits (immediately, since `run()` is a stub) without panicking — a trivial placeholder proving the binary
  target itself is wired correctly, to be expanded in Ticket 003.

## Potential Risks / Edge Cases

- **Risk: dependency boundary check gives false negatives** if a forbidden crate is pulled in only under a non-default
  feature flag that isn't exercised in the default `cargo metadata` resolution. Mitigate by running the check against
  `cargo metadata --all-features` in addition to the default feature set.
- **Risk: `cargo metadata` JSON schema changes** across Cargo versions could break the boundary-check test. Pin a
  `--format-version 1` explicitly (already Cargo's stable default) and note in the ADR that this test must be revisited
  if the workspace's minimum-supported Rust version (MSRV) is bumped significantly.
- **Edge case: workspace-level lint/deny configuration.** Resist the temptation to add `#![deny(warnings)]`
  workspace-wide in this ticket — doing so prematurely can cause unrelated, unfixable-yet friction (e.g., `unused`
  warnings in intentionally-stubbed placeholder crates) that blocks later tickets for reasons unrelated to their own
  scope. Defer strict lint gating to Epic 6 (Theming and Polish) or a dedicated CI-hardening ticket.
- **Edge case: `publish = false` omission.** If forgotten, a future `cargo publish --workspace` (unlikely but possible
  via a misconfigured CI job) could attempt to publish internal-only crates to crates.io. Explicitly set
  `publish = false` on every member crate now, cheaply, to foreclose this entirely.

---