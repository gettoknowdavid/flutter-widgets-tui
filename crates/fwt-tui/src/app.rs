use std::time::Duration;

use crossterm::event::{Event as CtEvent, EventStream};
use futures_util::StreamExt;
use fwt_app::executor::CommandExecutor;
use fwt_app::message::Message;
use tokio::sync::mpsc;

/// How often the tick fires. 250ms is frequent enough for a "thinking..."
/// spinner (Epic 5) or a connectivity-status blink (TRD Section 2.4)
/// without meaningfully affecting NFR-5's memory/CPU budget — this app is
/// not a game; it doesn't need 60Hz ticks.
const TICK_INTERVAL: Duration = Duration::from_millis(250);

/// Bounded channel capacity. 32 is a deliberately conservative starting
/// point: small enough to surface backpressure quickly in testing/profiling
/// if a future epic's background work (e.g., AI streaming chunks) produces
/// messages faster than the render loop drains them, but large enough that
/// Epic 1's trivial dummy command never comes close to filling it.
const MESSAGE_CHANNEL_CAPACITY: usize = 32;

pub async fn run_event_loop() -> Result<(), crate::TuiError> {
    // --- Set up the three input sources ---
    let (tx, mut rx) = mpsc::channel::<Message>(MESSAGE_CHANNEL_CAPACITY);
    let executor = CommandExecutor::new(tx.clone());
    let mut crossterm_events = EventStream::new();
    let mut tick_interval = tokio::time::interval(TICK_INTERVAL);

    // ------------------------------------------------------------------
    // CANCELLATION SAFETY — READ THIS BEFORE TOUCHING THE LOOP BELOW.
    //
    // `tokio::select!` re-polls whichever branches it's given EVERY time
    // the loop body runs. If you constructed a future INSIDE the loop
    // (e.g. `tokio::time::sleep(TICK_INTERVAL).await` written directly as
    // a select! branch), a fresh Sleep would be created every iteration —
    // and since select! only awaits ONE branch to completion per
    // iteration, that Sleep would frequently be dropped before it ever
    // finished, meaning it might NEVER actually elapse. This is a classic,
    // easy-to-miss Tokio footgun.
    //
    // The fix: construct each long-lived source ONCE, above the loop (as
    // done here — `crossterm_events`, `tick_interval`, `rx` are all owned
    // locals declared before `loop { ... }` begins), and only reference
    // them (via `.next()`, `.tick()`, `.recv()`) INSIDE each select! arm.
    // Because these are stateful objects that remember "where they left
    // off," repeatedly polling the SAME instance across loop iterations is
    // correct — polling a freshly-constructed one each time is not.
    // ------------------------------------------------------------------

    let mut state = fwt_app::state::AppState::default();

    loop {
        let message: Message = tokio::select! {
            // Branch A: a terminal event arrived.
            maybe_event = crossterm_events.next() => {
                match maybe_event {
                    Some(Ok(CtEvent::Key(key_event))) => Message::Key(key_event),
                    Some(Ok(CtEvent::Resize(w, h))) => Message::Resize(w, h),
                    Some(Ok(_other_event)) => {
                        // Mouse/paste/focus events: not handled in Epic 1
                        // (NFR-8: keyboard-only operation). Loop again
                        // without producing a Message by `continue`-ing —
                        // see note below on why this is safe inside
                        // select!.
                        continue;
                    }
                    Some(Err(err)) => {
                        tracing::error!(error = %err, "crossterm event stream error");
                        continue;
                    }
                    None => {
                        // Stream ended (stdin closed). Treat as a request
                        // to shut down gracefully rather than looping
                        // forever on a dead stream.
                        Message::Quit
                    }
                }
            }

            // Branch B: a background task's result arrived.
            maybe_message = rx.recv() => {
                maybe_message.unwrap_or(Message::Quit)
            }

            // Branch C: the tick fired.
            _ = tick_interval.tick() => Message::Tick(std::time::Instant::now()),

            // Branch D: Ctrl+C / SIGINT.
            _ = tokio::signal::ctrl_c() => Message::Quit,

            // Branch E (Unix only): SIGTERM, e.g. from `kill` without -9.
            // Gated behind #[cfg(unix)] since Windows has no equivalent
            // signal semantics that `tokio::signal::unix` can express.
            _ = sigterm_signal(), if cfg!(unix) => Message::Quit,
        };

        // --- The ONLY place update() is called. Pure, synchronous. ---
        let outcome = fwt_app::state::update(&mut state, message);

        // --- Dispatch any side effects (non-blocking) ---
        for command in outcome.commands {
            executor.dispatch(command);
        }

        // --- Render, but ONLY if something actually changed ---
        if outcome.redraw {
            // Ticket 005 wires this up to a real `view(&state)` call that
            // writes into a ratatui Frame. In this ticket, a stub call
            // is enough to prove the wiring — e.g. `tracing::trace!`.
            tracing::trace!("redraw requested (view() wiring lands in Ticket 005)");
        }

        if state.should_quit {
            break;
        }
    }

    Ok(())
}

/// Thin wrapper so the `#[cfg(unix)]`-gated signal stream has a single,
/// reusable `.await`-able future to reference from the `select!` above.
/// On non-Unix targets this simply never resolves (`std::future::pending`),
/// which is fine because the `if cfg!(unix)` guard on the select! arm
/// means it's never actually polled to meaningful effect on Windows.
async fn sigterm_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        // unwrap() here is acceptable: failure to install a signal handler
        // is a genuine programmer/environment invariant violation (e.g.
        // called twice, OS resource exhaustion), not a recoverable runtime
        // condition — consistent with TRD 2.5's "panics reserved for
        // invariant violations."
        let mut term = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        term.recv().await;
    }
    #[cfg(not(unix))]
    {
        std::future::pending::<()>().await;
    }
}
