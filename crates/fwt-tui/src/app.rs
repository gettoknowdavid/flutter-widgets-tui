//! The cross-thread event loop (TRD Section 2.3, Ticket 003/004).
//!
//! Ties together:
//!   - `TerminalGuard` (terminal.rs) — raw mode / alt screen lifecycle
//!   - `fwt_app::state::{AppState, Message, update}` — the Elm-style core
//!   - `fwt_app::executor` — non-blocking `Command` dispatch
//!
//! The loop polls three input sources via `tokio::select!`: crossterm's
//! terminal event stream, an `mpsc::Receiver<Message>` fed by background
//! tasks, and a tick timer. Every branch converts its raw input into a
//! `Message` *before* any further processing — no business logic lives
//! inside the `select!` arms themselves; that is `update()`'s job alone.
//!
//! Cancellation-safety note: the `EventStream`, the tick `interval`, and
//! the channel receiver are held as owned locals *outside* the loop body
//! and only `.await`ed by reference inside `select!`. Recreating any of
//! these per-iteration (a classic tokio footgun) would silently break —
//! e.g. a `Sleep` recreated every loop never actually elapses.

use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use futures_util::StreamExt;
use fwt_app::executor;
use fwt_app::message::Message;
use fwt_app::state::{update, AppState};
use tokio::sync::mpsc;

use crate::terminal::TerminalGuard;

/// How often the tick timer fires. Sufficient for future spinner/
/// connectivity-status animation (TRD Section 2.4) without meaningfully
/// impacting NFR-5's memory/CPU budget.
const TICK_INTERVAL: Duration = Duration::from_millis(250);

/// Bounded channel capacity for background-task-originated messages.
/// Bounded (not unbounded) deliberately: gives a concrete, testable
/// backpressure point rather than unbounded memory growth if background
/// tasks (Epic 5's AI streaming, in particular) ever outpace the render
/// loop's drain rate.
const MESSAGE_CHANNEL_CAPACITY: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum EventLoopError {
    #[error("terminal I/O error while polling or rendering")]
    TerminalIo(#[from] std::io::Error),
}

/// Runs the event loop to completion (until `Message::Quit` or an
/// unrecoverable error). Consumes the `TerminalGuard` so the guard's
/// `Drop` fires the instant this function returns, restoring the
/// terminal before any caller-side cleanup runs.
pub async fn run_event_loop(mut guard: TerminalGuard) -> Result<(), EventLoopError> {
    let mut state = AppState::default();

    let (tx, mut rx) = mpsc::channel::<Message>(MESSAGE_CHANNEL_CAPACITY);

    // Long-lived futures, constructed once, outside the loop.
    let mut term_events = EventStream::new();
    let mut ticker = tokio::time::interval(TICK_INTERVAL);

    loop {
        let message = tokio::select! {
            // (a) Terminal input: key/resize/paste/mouse events.
            maybe_event = term_events.next() => {
                match maybe_event {
                    Some(Ok(event)) => convert_crossterm_event(event),
                    Some(Err(err)) => {
                        tracing::error!(error = %err, "crossterm event stream error");
                        None
                    }
                    // The stream ending (stdin closed) is treated as a
                    // shutdown request rather than a silent hang.
                    None => Some(Message::Quit),
                }
            }

            // (b) Background-task-originated messages (Command results).
            maybe_msg = rx.recv() => maybe_msg,

            // (c) Tick timer, for future spinner/animation state.
            _ = ticker.tick() => Some(Message::Tick(std::time::Instant::now())),

            // Ctrl+C: the same shutdown path as any other quit trigger,
            // not a separate ad hoc one.
            _ = tokio::signal::ctrl_c() => Some(Message::Quit),

            // SIGTERM on Unix; a no-op-forever future on other platforms,
            // so this arm still type-checks and simply never fires there.
            _ = wait_for_terminate_signal() => Some(Message::Quit),
        };

        let Some(message) = message else {
            // No message this iteration (e.g. a swallowed event-stream
            // error) — loop again without touching state or rendering.
            continue;
        };

        let outcome = update(&mut state, message);
        executor::dispatch_all(outcome.commands, &tx);

        if outcome.redraw {
            render(&mut guard, &state)?;
        }

        if state.should_quit {
            break;
        }
    }

    Ok(())
}

/// Converts a raw `crossterm::event::Event` into the corresponding
/// `Message`, or `None` if this event doesn't map to one the app cares
/// about yet (e.g. mouse/paste/focus events in this epic).
///
/// Factored out as its own free function (rather than inlined in the
/// `select!` arm) so it is independently unit-testable without a real
/// terminal or tokio runtime.
fn convert_crossterm_event(event: Event) -> Option<Message> {
    match event {
        Event::Key(key_event) => {
            // Only act on presses — some terminals emit repeat/release
            // events under the Kitty keyboard protocol, and acting on
            // those too would double-handle a single physical keypress.
            if key_event.kind == KeyEventKind::Press {
                Some(Message::Key(key_event))
            } else {
                None
            }
        }
        Event::Resize(width, height) => Some(Message::Resize(width, height)),
        // Mouse/paste/focus events: no Message variant yet (NFR-8 makes
        // this app keyboard-only by design, not just by omission).
        _ => None,
    }
}

/// Waits for `SIGTERM` on Unix; on non-Unix platforms this future never
/// resolves, so the corresponding `select!` arm simply never fires there
/// — kept as a real (if permanently pending) future rather than an `if`
/// guard, to sidestep `tokio::select!`'s stricter rules around
/// conditionally-absent branches.
#[cfg(unix)]
async fn wait_for_terminate_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    match signal(SignalKind::terminate()) {
        Ok(mut sig) => {
            sig.recv().await;
        }
        Err(err) => {
            tracing::error!(error = %err, "failed to install SIGTERM handler");
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(not(unix))]
async fn wait_for_terminate_signal() {
    std::future::pending::<()>().await;
}

/// Stub render call — Ticket 005 implements the real `AppShell` via
/// `render_app_shell(...)`. For Ticket 004's scope, this only needs to
/// exist so the dirty-flag wiring (only redraw when `update()` says so)
/// is provably connected to a real `Terminal::draw` call, not just to a
/// TODO comment.
fn render(guard: &mut TerminalGuard, _state: &AppState) -> Result<(), EventLoopError> {
    guard.terminal.draw(|_frame| {
        // Ticket 005 replaces this closure body with
        // `render_app_shell(frame, frame.area(), state, theme)`.
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn key_press_event_converts_to_message_key() {
        let event = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        let message = convert_crossterm_event(event);
        assert!(matches!(message, Some(Message::Key(_))));
    }

    #[test]
    fn key_release_event_is_ignored() {
        let mut key_event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        key_event.kind = KeyEventKind::Release;
        let event = Event::Key(key_event);
        assert!(convert_crossterm_event(event).is_none());
    }

    #[test]
    fn resize_event_converts_to_message_resize() {
        let event = Event::Resize(80, 24);
        let message = convert_crossterm_event(event);
        assert!(matches!(message, Some(Message::Resize(80, 24))));
    }

    #[test]
    fn mouse_event_has_no_corresponding_message_in_epic_1() {
        let event = Event::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Moved,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert!(convert_crossterm_event(event).is_none());
    }
}
