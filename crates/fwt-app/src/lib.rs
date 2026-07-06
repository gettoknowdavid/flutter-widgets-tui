//! # fwt-app — Application Layer
//!
//! Use-case / orchestration logic: `SearchService`, `FavoritesService`,
//! `ChatService`, `AppState`, and the `update()` function (Elm-style).
//!
//! ## Dependency rule
//! This crate depends on `fwt-domain` for data types and port traits.
//! It must NEVER depend on `fwt-infra` (concrete adapters) or `fwt-tui`
//! (presentation) — those are supplied via dependency injection from
//! `fwt-cli`, the composition root. Depending on concretes here would
//! defeat the entire point of the ports-and-adapters architecture: the
//! ability to test this crate against fakes/mocks with zero real I/O.
