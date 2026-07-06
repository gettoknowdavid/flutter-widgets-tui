# ADR-000: Workspace Structure & Dependency Boundary Enforcement

## Decision

- Five (5) architectural crates: fwt-domain, fwt-app, fwt-infra, fwt-tui, fwt-cli.
- Dependency rule: Presentation → Application → Domain ← Infrastructure.
  fwt-domain has zero I/O/framework dependencies, enforced at compile
  time by omission from its Cargo.toml, and verified at CI time by an
  automated dependency-graph check.
- The boundary check lives in `xtask/` (a 6th, dev-tooling-only workspace
  member) rather than a root-level `tests/architecture.rs`, because a
  pure virtual workspace's root Cargo.toml has no `[package]` section to
  hang dev-dependencies off. Run via `cargo run -p xtask`.
- Contributors: do not bypass this check. If you need a new dependency
  in fwt-domain, that's a signal to introduce a port trait instead and
  implement it in fwt-infra.