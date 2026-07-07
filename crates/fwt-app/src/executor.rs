//! Command dispatch — the single choke point through which every
//! `Command` returned by `update()` is executed asynchronously.
//!
//! Per TRD Section 2.2/2.3: `update()` never performs I/O directly. It
//! only *describes* side effects as `Command` values; this module spawns
//! the actual async work and feeds the eventual `Message` back into the
//! loop via the shared `mpsc::Sender<Message>`. No ad hoc `tokio::spawn`
//! calls belong anywhere else in the codebase — this is the one place.

use crate::command::Command;
use crate::message::Message;
use tokio::sync::mpsc;

/// Spawns a `Command` as a non-blocking `tokio::task`, whose result is
/// sent back through `sender` as a `Message`. The caller (the event
/// loop) must never `.await` this directly — call it and move on; the
/// render/UI thread must never block on a `Command`'s execution.
pub fn dispatch(command: Command, sender: mpsc::Sender<Message>) {
    match command {
        Command::None => {}
        Command::SimulatedDelay(duration) => {
            tokio::spawn(async move {
                tokio::time::sleep(duration).await;
                // Best-effort send: if the receiver is already gone (the
                // app is shutting down), there's nowhere useful to report
                // the failure, so it's dropped silently rather than
                // panicking a background task.
                let _ = sender.send(Message::DebugTaskCompleted).await;
            });
        }
    }
}

/// Dispatches every command in `commands` via [`dispatch`], cloning the
/// sender once per command since each spawned task needs its own owned
/// clone.
pub fn dispatch_all(commands: Vec<Command>, sender: &mpsc::Sender<Message>) {
    for command in commands {
        dispatch(command, sender.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test(start_paused = true)]
    async fn simulated_delay_sends_debug_task_completed_after_delay() {
        let (tx, mut rx) = mpsc::channel(8);
        dispatch(Command::SimulatedDelay(Duration::from_secs(2)), tx);

        // Nothing should have arrived yet — the delay hasn't elapsed.
        assert!(rx.try_recv().is_err());

        tokio::time::advance(Duration::from_secs(2)).await;
        // Yield so the spawned task actually gets scheduled after the
        // simulated time advance.
        tokio::task::yield_now().await;

        let msg = rx.recv().await.expect("expected a message after the delay");
        assert!(matches!(msg, Message::DebugTaskCompleted));
    }

    #[tokio::test]
    async fn none_command_does_not_send_anything() {
        let (tx, mut rx) = mpsc::channel(8);
        dispatch(Command::None, tx);
        tokio::task::yield_now().await;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn dispatch_all_handles_multiple_commands_independently() {
        let (tx, mut rx) = mpsc::channel(8);
        dispatch_all(
            vec![
                Command::None,
                Command::SimulatedDelay(Duration::from_millis(100)),
                Command::SimulatedDelay(Duration::from_millis(100)),
            ],
            &tx,
        );

        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        let first = rx.recv().await.expect("expected first completion");
        let second = rx.recv().await.expect("expected second completion");
        assert!(matches!(first, Message::DebugTaskCompleted));
        assert!(matches!(second, Message::DebugTaskCompleted));
    }
}
