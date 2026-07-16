//! # fwt-infra — Infrastructure Layer
//!
//! Concrete adapters implementing `fwt-domain`'s port traits: SQLite
//! repositories, the fuzzy search index, HTTP AI clients, GitHub OAuth,
//! clipboard, config file I/O.
//!
//! ## Dependency rule
//! Depends on `fwt-domain` (to implement its traits). Must NOT depend on
//! `fwt-app` or `fwt-tui` — infrastructure adapters are wired into the
//! application layer via dependency injection from `fwt-cli`, never the
//! reverse.

pub mod db;
