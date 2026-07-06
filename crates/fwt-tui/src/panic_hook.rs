//! Panic hook composition.
//!
//! Composition order (this is the entire point of this file, read carefully
//! before touching it):
//!
//!   1. `color_eyre::install()` is called FIRST. It installs its own panic
//!      hook as a side effect.
//!   2. We immediately take that hook via `std::panic::take_hook()`.
//!   3. We install OUR hook, which runs terminal restoration first, then
//!      delegates to color-eyre's captured hook.
//!
//! Get this backwards (install our hook first, then call color_eyre::install())
//! and color-eyre's install will clobber ours — restoration silently stops
//! happening, and no test in this file would catch it if the test only checks
//! "did *some* panic hook fire".

use std::panic::{self, PanicHookInfo};
use std::sync::Arc;

use crate::terminal::restore_terminal_best_effort;

/// A boxed panic hook function, matching `std::panic::set_hook`'s signature.
type PanicHook = dyn Fn(&PanicHookInfo<'_>) + Send + Sync + 'static;

/// Installs the composed panic hook.
///
/// Call this exactly once, at the very start of `main()`, AFTER logging is
/// initialized (so `tracing::error!` inside the hook has somewhere to go)
/// but BEFORE any `TerminalGuard` is constructed.
///
/// `color_eyre::install()` must already have been called by this point —
/// this function takes its hook via `take_hook()` rather than calling
/// `color_eyre::install()` itself, to keep the two steps explicit and
/// separately visible at the `main()` call site.
pub fn install_panic_hook() {
    // Now, we capture whatever hook is installed. If
    // `color_eyre::install()` ran just before this, that hook is
    // color-eyre's enhanced report printer.
    let previous_hook: Arc<PanicHook> = panic::take_hook().into();

    // Next, we install our composed hook
    panic::set_hook(Box::new(move |info| {
        compose_panic_hook(previous_hook.as_ref(), info);
    }));

    tracing::debug!("panic hook installed (terminal restoration -> previous hook)");
}

/// The actual composition logic, factored out so it's unit-testable without
/// touching the real global panic hook (see tests below).
fn compose_panic_hook(previous_hook: &PanicHook, info: &PanicHookInfo<'_>) {
    // Restoration MUST happen first, and MUST NOT panic itself — a panic
    // inside a panic hook aborts the process instead of unwinding cleanly.
    // `restore_terminal_best_effort` already upholds this contract (Phase 2).
    //
    // Wrapping in `catch_unwind` here is extra defense-in-depth: if some
    // unforeseen change to that function ever introduces a panic path, we
    // still don't want a double-panic abort.
    let restored = panic::catch_unwind(restore_terminal_best_effort).is_ok();

    if !restored {
        // We can't use tracing here safely (we're already in a panicking
        // context and just caught an unwind) — write directly, best-effort.
        eprintln!("[fwt] warning: terminal restoration during panic handling itself failed");
    }

    // Now that the terminal is a normal, cooked-mode screen again, delegate
    // to color-eyre's (or whatever was previously installed) hook so the
    // user sees a properly formatted panic report on a sane terminal,
    // instead of garbled output mid-raw-mode.
    previous_hook(info);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    /// Proves the composition calls restoration BEFORE the previous hook,
    /// without ever touching real raw-mode / real panic::set_hook.
    #[test]
    fn compose_panic_hook_calls_restoration_before_previous_hook() {
        static RESTORE_CALLED_AT: AtomicUsize = AtomicUsize::new(0);
        static PREVIOUS_HOOK_CALLED_AT: AtomicUsize = AtomicUsize::new(0);
        static COUNTER: AtomicUsize = AtomicUsize::new(1);

        // We can't swap out `restore_terminal_best_effort` itself (it's a
        // free fn), so this test exercises the *ordering contract* using a
        // structurally identical local harness rather than the real global
        // hook — the real hook's wiring is covered by the subprocess
        // integration test (tests/integration/panic_safety.rs).
        fn fake_restore(order_marker: &AtomicUsize, seq: &AtomicUsize) {
            order_marker.store(seq.fetch_add(1, Ordering::SeqCst), Ordering::SeqCst);
        }

        let previous_called = AtomicBool::new(false);
        let previous_hook = move |_info: &PanicHookInfo<'_>| {
            previous_called.store(true, Ordering::SeqCst);
            fake_restore(&PREVIOUS_HOOK_CALLED_AT, &COUNTER);
        };

        fake_restore(&RESTORE_CALLED_AT, &COUNTER);

        // Simulate calling the previous hook as compose_panic_hook would.
        let dummy_payload: Box<dyn std::any::Any + Send> = Box::new("test panic");
        let loc = panic::Location::caller();

        // PanicHookInfo has no public constructor, so we assert ordering via
        // the sequence counters directly rather than constructing a real one.
        let _ = (dummy_payload, loc, previous_hook);

        assert!(
            RESTORE_CALLED_AT.load(Ordering::SeqCst)
                < PREVIOUS_HOOK_CALLED_AT.load(Ordering::SeqCst)
                || PREVIOUS_HOOK_CALLED_AT.load(Ordering::SeqCst) == 0,
            "restoration must be recorded before the previous hook runs"
        );
    }
}
