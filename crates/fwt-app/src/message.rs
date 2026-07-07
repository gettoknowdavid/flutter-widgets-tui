/// Everything that can happen, from the event loop's perspective, reduced to
/// a single closed set of variants. This is the ONLY input to `update()`.
///
/// Why an enum and not, say, trait objects or closures? Because an enum is
/// exhaustively matchable — the compiler forces you to handle every variant
/// at every match site, which is exactly the safety net we want as this
/// enum grows across five more epics. A `Box<dyn Fn(...)>` gives up that
/// guarantee entirely.
#[derive(Debug, Clone)]
pub enum Message {
    /// A raw key event, not yet interpreted. `update()` decides what a key
    /// means (quit? navigate? type a character?) — this variant just carries
    /// the fact that *a* key was pressed.
    Key(crossterm::event::KeyEvent),

    /// The terminal was resized to (width, height), in terminal cells.
    Resize(u16, u16),

    /// A periodic tick, carrying the instant it fired. Ticket 004 will use
    /// this to drive spinners/animations; for now, nothing in `update()`
    /// reacts to it (see Ticket 004's note on `redraw: false` for Tick).
    Tick(std::time::Instant),

    /// The dummy round-trip message this ticket introduces to PROVE that
    /// a background task can talk back to the loop. Ticket 004+ will add
    /// real variants like `Message::ChatChunk(String)`,
    /// `Message::SearchResults(Vec<Widget>)`, etc. — this is the pattern
    /// they'll all follow.
    DebugTaskCompleted,

    /// Graceful shutdown requested (quit key, Ctrl+C, SIGTERM — all funnel
    /// here, see app.rs section 3.3).
    Quit,
}
