//! # fwt-tui — Presentation Layer
//!
//! Ratatui widgets, layout, input handling, themes, and the TUI event
//! loop ("the app shell").
//!
//! ## Dependency rule
//! Depends on `fwt-app` (use-cases) and `fwt-domain` (data types). Must
//! NOT depend on `fwt-infra` — this crate never constructs a concrete
//! SQLite repo, HTTP client, etc. directly. `fwt-cli` (the composition
//! root) constructs real `fwt-infra` adapters and injects them into
//! `fwt-app` services, which this crate then calls through trait-bound
//! interfaces only.

pub mod app;
pub mod panic_hook;
pub mod terminal;

pub use terminal::{TerminalError, TerminalGuard};

#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    #[error("stub error — no real implementation yet (Ticket 002+)")]
    Stub,
}

/// Entry point called by `fwt-cli`
pub fn run() -> Result<(), TuiError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_stub_returns_ok() {
        assert!(run().is_ok());
    }
}
