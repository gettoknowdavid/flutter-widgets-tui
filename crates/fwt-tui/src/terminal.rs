//! Terminal lifecycle management.
//!
//! `TerminalGuard` is the single owner of raw-mode / alternate-screen state.
//! Its restoration logic is also reused by the panic hook (see `panic_hook.rs`),
//! so there is exactly one implementation of "how do we put the terminal back",
//! not two that can silently diverge.
//!
//! ORDERING CONSTRAINT (see crates/fwt-cli/src/main.rs for the full sequence):
//! A `TerminalGuard` must only be constructed *after* logging, color-eyre, and
//! the panic hook are already installed. If you move guard construction earlier,
//! a panic during setup will not be caught by our restoration hook.

use crossterm::cursor::Show;
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

/// Errors that can occur while entering or leaving the terminal's
/// raw-mode / alternate-screen state.
///
/// Each variant wraps the underlying `std::io::Error` so the original
/// cause is never lost, per TRD Section 2.5's two-tier error strategy:
/// `thiserror` at this module boundary, `anyhow`/`color-eyre` only at
/// the top-level `main()` boundary.
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("failed to enable raw mode")]
    EnableRawModeFailed(#[source] std::io::Error),

    #[error("failed to disable raw mode")]
    DisableRawModeFailed(#[source] std::io::Error),

    #[error("failed to enter alternate screen")]
    EnterAltScreenFailed(#[source] std::io::Error),

    #[error("failed to leave alternate screen")]
    LeaveAltScreenFailed(#[source] std::io::Error),

    #[error("failed to initialize the ratatui backend/terminal")]
    BackendInitFailed(#[source] std::io::Error),
}

/// RAII guard over the terminal's raw-mode + alternate-screen state.
///
/// Construction (`enter()`) performs, in order:
///   1. `enable_raw_mode()`
///   2. `EnterAlternateScreen`
///   3. Ratatui `Terminal` backend construction
///
/// `Drop` unconditionally attempts to reverse all three steps, regardless
/// of whether they fully succeeded on entry. It **never panics** — any error
/// during restoration is logged via `tracing::error!` and swallowed, since a
/// panic during unwind (e.g. inside another panic's stack unwind) can abort
/// the whole process instead of exiting cleanly.
pub struct TerminalGuard {
    pub terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
}
impl TerminalGuard {
    /// Enters raw mode + the alternate screen and constructs the Ratatui
    /// terminal backend. Returns a typed `TerminalError` on any failure
    /// (never panics, never `.unwrap()`s).
    pub fn enter() -> Result<Self, TerminalError> {
        crossterm::terminal::enable_raw_mode().map_err(TerminalError::EnableRawModeFailed)?;

        // If entering the alt screen fails after raw mode succeeded, we must
        // not leak raw mode. Attempt to undo it before propagating the error.
        if let Err(e) = execute!(std::io::stdout(), EnterAlternateScreen) {
            let _ = crossterm::terminal::disable_raw_mode();
            return Err(TerminalError::DisableRawModeFailed(e));
        }

        let backend = CrosstermBackend::new(std::io::stdout());
        let terminal = Terminal::new(backend).map_err(|e| {
            // Unwind what we already did before propagating.
            let _ = execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
            let _ = crossterm::terminal::disable_raw_mode();
            TerminalError::BackendInitFailed(e)
        })?;

        tracing::debug!("terminal entered: raw mode + alternate screen active");
        Ok(Self { terminal })
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // This function must never panic. Every fallible call below is
        // handled with `if let Err` + `tracing::error!`, never `?`/`unwrap`.
        restore_terminal_best_effort();
    }
}

/// The actual restoration logic, factored out as a free function so it can
/// be:
///   (a) called from `Drop` (the normal-exit path), and
///   (b) called from the panic hook (`panic_hook.rs`) as defense-in-depth,
///       since a panic mid-unwind is not guaranteed to run `Drop` cleanly
///       in every context (e.g. if unwinding is itself aborted).
///
/// Idempotent by design: calling this twice (once from the hook, once from
/// `Drop`) must not panic or produce confusing double-errors — `crossterm`'s
/// underlying calls are tolerant of "already disabled" states in practice,
/// and we treat every step independently rather than short-circuiting on
/// the first failure, since multiplexers (tmux/screen) can leave state
/// slightly inconsistent with what `crossterm` believes.
pub fn restore_terminal_best_effort() {
    if let Err(e) = crossterm::terminal::disable_raw_mode() {
        tracing::error!(error = %e, "failed to disable raw mode during restoration");
    }

    if let Err(e) = execute!(std::io::stdout(), LeaveAlternateScreen, Show) {
        tracing::error!(error = %e, "failed to leave alternate screen during restoration");
    }

    tracing::debug!("terminal restoration attempted");
}
