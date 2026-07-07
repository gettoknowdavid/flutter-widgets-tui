//! # fwt-cli — Composition Root
//!
//! The only crate permitted to depend on all four other crates. Its job
//! (in later tickets) is: parse CLI flags, construct concrete `fwt-infra`
//! adapters, inject them into `fwt-app` services, and hand control to
//! `fwt-tui::run()`. For now it does the bare minimum to prove the whole
//! workspace links correctly end-to-end.

use clap::Parser;

pub mod logging;
pub mod reset;

/// Small, deliberately bounded worker pool. This app's background work
/// (DB queries in Epic 2+, HTTP calls in Epic 5) is I/O-bound, not
/// CPU-bound — it spends its time waiting on the OS/network, not crunching
/// numbers. A large thread pool wouldn't make I/O-bound work finish faster;
/// it would just consume more idle memory (NFR-5's ~60MB budget matters
/// here), so we pick a small, explicit number instead of Tokio's
/// CPU-core-count default.
const RUNTIME_WORKER_THREADS: usize = 4;

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
        // catch and restore is already installed. ----
        let _guard = fwt_tui::TerminalGuard::enter()?;
        tracing::warn!("--panic-test invoked: triggering intentional panic");
        panic!("intentional test panic (--panic-test)");
    }

    // Build the runtime explicitly rather than using the `#[tokio::main]`
    // macro, specifically so we control `worker_threads` directly instead
    // of accepting the macro's default (which uses the number of logical
    // CPU cores — often far more than 4, and NOT justified by this app's
    // I/O-bound workload).
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(RUNTIME_WORKER_THREADS)
        .enable_all()
        .build()
        .map_err(|e| color_eyre::eyre::eyre!("failed to build tokio runtime: {e}"))?;

    let _guard = fwt_tui::TerminalGuard::enter()?;

    tracing::info!("fwt starting; entering event loop");

    // `block_on` hands control to the async event loop and blocks THIS
    // (main OS) thread until it returns — that's fine, this thread's whole
    // job from here on is to drive the async runtime.
    runtime.block_on(fwt_tui::app::run_event_loop())?;

    tracing::info!("event loop exited cleanly; shutting down");

    Ok(())
}
