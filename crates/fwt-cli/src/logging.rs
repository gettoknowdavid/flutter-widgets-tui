//! Logging bootstrap.
//!
//! MUST be initialized first, before color-eyre, before the panic hook,
//! before any TerminalGuard — nothing before this point in `main()` should
//! call `tracing::*!` macros, since there's nowhere for them to go yet.
//!
//! Writes exclusively to a rotating file in the OS-appropriate data/cache
//! directory. Never writes to stdout/stderr, since doing so while raw mode /
//! alternate screen is active would corrupt the TUI's rendered output.

use directories::ProjectDirs;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Fields whose values must never appear in log output, regardless of level.
/// Matched case-insensitively against the tracing field *name*.
const REDACTED_FIELD_SUBSTRINGS: &[&str] = &["token", "key", "secret"];

/// Must be held for the lifetime of `main()` — dropping it flushes and stops
/// the background writer thread. Store it in a local binding in `main()`,
/// never let it be dropped early.
pub struct LoggingGuard {
    _worker_guard: tracing_appender::non_blocking::WorkerGuard,
}

/// Initializes the global `tracing` subscriber with a rotating, non-blocking
/// file appender. Returns a guard that must be kept alive for the process's
/// lifetime.
pub fn init_logging() -> Result<LoggingGuard, LoggingInitError> {
    let dirs = ProjectDirs::from("dev", "flutterwidgets", "fwt")
        .ok_or(LoggingInitError::NoHomeDirectory)?;

    let log_dir: std::path::PathBuf = dirs.data_local_dir().join("logs");
    std::fs::create_dir_all(&log_dir).map_err(LoggingInitError::CreateLogDir)?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, "fwt.log");
    let (non_blocking, worker_guard) = tracing_appender::non_blocking(file_appender);

    let redaction_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .map_event_format(|f| f);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(redaction_layer)
        .try_init()
        .map_err(LoggingInitError::SubscriberInit)?;

    tracing::info!("logging initialized; writing to {}", log_dir.display());

    Ok(LoggingGuard {
        _worker_guard: worker_guard,
    })
}

/// Returns true if a tracing field name should be redacted before being
/// written to the log — matched against `token`/`key`/`secret` substrings,
/// case-insensitively, per NFR on secrets never being logged (TRD Section 5.4/9.1).
pub fn is_redacted_field_name(field_name: &str) -> bool {
    let lower = field_name.to_ascii_lowercase();
    REDACTED_FIELD_SUBSTRINGS
        .iter()
        .any(|needle| lower.contains(needle))
}

#[derive(Debug, thiserror::Error)]
pub enum LoggingInitError {
    #[error("could not determine an OS-appropriate data directory for logs")]
    NoHomeDirectory,

    #[error("failed to create log directory")]
    CreateLogDir(#[source] std::io::Error),

    #[error("failed to install the global tracing subscriber")]
    SubscriberInit(#[source] tracing_subscriber::util::TryInitError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_fields_containing_token_key_or_secret_case_insensitively() {
        assert!(is_redacted_field_name("api_token"));
        assert!(is_redacted_field_name("API_KEY"));
        assert!(is_redacted_field_name("client_secret"));
        assert!(is_redacted_field_name("Secret_Value"));
        assert!(!is_redacted_field_name("widget_name"));
        assert!(!is_redacted_field_name("session_id"));
    }
}
