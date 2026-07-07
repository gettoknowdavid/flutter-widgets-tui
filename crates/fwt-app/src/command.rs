/// A side effect requested by [`update`], to be executed asynchronously
/// by the command executor (`crate::executor`) — `update()` itself never
/// performs I/O directly.
#[derive(Debug, Clone)]
pub enum Command {
    /// No side effect. Exists so `UpdateOutcome::commands` can be
    /// exercised in tests without wiring up a real command.
    None,
    /// Ticket 003's dummy command: sleep, then send back
    /// `Message::DebugTaskCompleted`, proving the Command-out/Message-in
    /// round trip works end-to-end.
    SimulatedDelay(std::time::Duration),
}
