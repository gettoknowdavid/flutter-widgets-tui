//! # fwt-domain — Domain Layer
//!
//! This crate contains **pure data types and port trait definitions only**.
//! Per TRD Section 2.1 (dependency rule), this is the innermost layer:
//!
//!   Presentation → Application → Domain ← Infrastructure
//!
//! ## Hard constraints for this crate
//! - **Zero I/O.** No file access, no network, no database, no terminal.
//! - **Zero framework dependencies.** Do NOT add `ratatui`, `crossterm`,
//!   `tokio`, `rusqlite`, or `reqwest` to this crate's `Cargo.toml` — ever,
//!   directly or transitively through a dependency you control.
//! - This crate must be usable in a plain `#[test]` with no async runtime
//!   and no real filesystem/terminal, by design (TRD NFR-12: testability).
//!
//! If you are a future session picking up an Epic 2+ ticket and you're
//! tempted to add an I/O-bearing dependency here to make something
//! "convenient" — don't. Put that logic in `fwt-infra` and expose a trait
//! (a "port") here instead. An automated test (`tests/architecture.rs` in
//! the workspace root) will fail CI if this rule is violated.
