//! # fwt-cli — Composition Root
//!
//! The only crate permitted to depend on all four other crates. Parses
//! CLI flags, then (Ticket 004) builds the bounded-worker tokio runtime
//! and hands control to `fwt_tui::app::run_event_loop`.

use clap::Parser;

pub mod logging;
pub mod reset;

/// Small, documented worker pool: this app's background work (Epic 2+
/// DB/HTTP calls) is I/O-bound, not CPU-bound, so a large thread pool
/// buys nothing and would work against NFR-5's memory footprint budget.
const TOKIO_WORKER_THREADS: usize = 2;

#[derive(Parser, Debug)]
#[command(name = "fwt", version, about)]
struct Cli {
    /// Path to a TOML config file (overrides the default OS config location).
    #[arg(long)]
    config: Option<std::path::PathBuf>,

    /// Path to the SQLite database file (overrides the default OS data location).
    #[arg(long)]
    db_path: Option<std::path::PathBuf>,

    /// Theme name to use for this session (does not persist unless saved).
    #[arg(long)]
    theme: Option<String>,

    /// Disable the AI chat feature for this session.
    #[arg(long)]
    no_ai: bool,

    /// Reset local user data (favorites/history/settings) after confirmation.
    #[arg(long)]
    reset: bool,

    /// DEBUG-ONLY: trigger an intentional panic after the terminal guard is
    /// active, to exercise the panic-hook restoration path end-to-end.
    /// Gated so it can never appear in a release build.
    #[cfg(debug_assertions)]
    #[arg(long)]
    panic_test: bool,
}

fn main() -> color_eyre::Result<()> {
    // Logging FIRST. Nothing above this line may call tracing::*!.
    let _logging_guard = logging::init_logging()?;

    // Color-eyre next. Its install() installs its own panic hook
    // as a side effect — we deliberately let that happen before we touch
    // panic::set_hook ourselves.
    color_eyre::install()?;

    // Capture color-eyre's hook and wrap it with terminal
    // restoration. See panic_hook.rs's module doc for why this exact
    // order (color_eyre::install() BEFORE this call) is load-bearing.
    fwt_tui::panic_hook::install_panic_hook();

    let cli = Cli::parse();

    if cli.reset {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        return match reset::confirm_reset(stdin.lock(), stdout.lock()) {
            Ok(reset::ResetOutcome::Confirmed) => {
                reset::perform_reset_stub();
                Ok(())
            }
            Err(reset::ResetError::NotConfirmed) => {
                println!("Reset cancelled; no data was touched.");
                Ok(())
            }
            Err(e) => Err(e.into()),
        };
    }

    #[cfg(debug_assertions)]
    if cli.panic_test {
        // ONLY NOW do we construct the TerminalGuard, and ONLY
        // NOW is it safe/meaningful to panic — everything that should
        // catch and restore is already installed.
        let _guard = fwt_tui::TerminalGuard::enter()?;
        tracing::warn!("--panic-test invoked: triggering intentional panic");
        panic!("intentional test panic (--panic-test)");
    }

    let guard = fwt_tui::TerminalGuard::enter()?;

    tracing::info!(
        theme = ?cli.theme,
        db_path = ?cli.db_path,
        no_ai = cli.no_ai,
        "fwt starting"
    );

    // Ticket 004: hand off to the real Elm-style event loop. The runtime
    // is built here, in the composition root, rather than via
    // `#[tokio::main]`, so the worker-thread count stays an explicit,
    // documented constant rather than an implicit macro default.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(TOKIO_WORKER_THREADS)
        .enable_all()
        .build()
        .map_err(|e| color_eyre::eyre::eyre!("failed to build tokio runtime: {e}"))?;

    runtime.block_on(fwt_tui::app::run_event_loop(guard))?;

    Ok(())
}
