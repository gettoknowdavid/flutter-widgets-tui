/// These are the side effects `update()` wants performed. `update()` never
/// performs I/O itself — it only *describes* what it wants done, as data, and
/// hands that description to the `CommandExecutor` (see executor.rs).
///
/// Why describe effects as data instead of just calling `tokio::spawn`
/// directly inside `update()`? Two reasons:
///   1. `update()` must stay synchronous and side-effect-free so it's
///      trivially unit-testable (no tokio runtime needed in tests).
///   2. Centralizing all spawning in ONE place (the executor) means we have
///      exactly one choke point for things like "cancel everything on
///      quit" or "add a tracing span to every background task" later.
#[derive(Debug, Clone)]
pub enum Command {
    /// Does nothing. Useful as a default/no-op return value.
    Noop,

    /// Sleeps for the given duration, then sends back
    /// `Message::DebugTaskCompleted`. This is Epic 1's stand-in for a real
    /// future command like `Command::FetchWidget(id)` or
    /// `Command::SendChatMessage(text)`
    SimulatedDelay(std::time::Duration),
}
