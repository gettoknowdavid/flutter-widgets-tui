use crate::command::Command;
use crate::message::Message;

/// The single, centralized dispatcher for all `Command`s. Every background
/// task in the whole app is spawned through here — never call
/// `tokio::spawn` directly from `fwt-tui` or anywhere else.
///
/// Why centralize this? Because it gives us ONE place to add
/// cross-cutting behavior later (a concurrency limit via `Semaphore` once
/// Epic 5's AI streaming lands; a tracing span per task; cancellation on
/// quit) without hunting down scattered `tokio::spawn` call sites.
#[derive(Clone)]
pub struct CommandExecutor {
    /// A clone of the SAME sender the event loop is receiving from. Every
    /// spawned task gets its own clone of this, so many tasks can be "in
    /// flight" concurrently, each independently able to send a Message
    /// back when it finishes.
    message_tx: tokio::sync::mpsc::Sender<Message>,
}
impl CommandExecutor {
    #[must_use]
    pub fn new(message_tx: tokio::sync::mpsc::Sender<Message>) -> Self {
        Self { message_tx }
    }

    /// Accepts a `Command`, spawns it as a `tokio::task`, and returns
    /// IMMEDIATELY — it does not wait for the command to finish. This is
    /// the non-blocking guarantee in code form: `dispatch()` itself never
    /// `.await`s the command's actual work, only the (instant) act of
    /// scheduling it.
    pub fn dispatch(&self, command: Command) {
        // Clone the sender for this specific task. `mpsc::Sender` is
        // designed to be cheaply cloneable for exactly this pattern: many
        // producers, one consumer (the event loop holds the single
        // `Receiver`).
        let tx = self.message_tx.clone();

        tokio::spawn(async move {
            match command {
                Command::Noop => {}
                Command::SimulatedDelay(duration) => {
                    tokio::time::sleep(duration).await;

                    // `.send().await` can fail if the receiver has been
                    // dropped (e.g., the app is shutting down mid-task).
                    // That's an EXPECTED race during shutdown, not a bug —
                    // so we log at debug level and move on, we do NOT
                    // panic or `.unwrap()`.
                    if let Err(err) = tx.send(Message::DebugTaskCompleted).await {
                        tracing::debug!(
                            error = %err,
                            "failed to send DebugTaskCompleted; \
                             receiver likely dropped during shutdown"
                        );
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::mpsc;

    #[tokio::test(start_paused = true)]
    async fn simulated_delay_command_sends_debug_task_completed() {
        let (tx, mut rx) = mpsc::channel::<Message>(8);
        let executor = CommandExecutor::new(tx);

        executor.dispatch(Command::SimulatedDelay(Duration::from_secs(2)));

        // Nothing should have arrived yet — virtual time hasn't moved.
        assert!(rx.try_recv().is_err());

        // Fast-forward virtual time instantly, no real wall-clock wait.
        tokio::time::advance(Duration::from_secs(2)).await;

        // Give the spawned task a chance to run now that its sleep resolved.
        let received = rx.recv().await.expect("expected a message");
        assert!(matches!(received, Message::DebugTaskCompleted));
    }
}
