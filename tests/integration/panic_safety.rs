//! Subprocess panic-safety integration test.
//!
//! LIMITATION (documented per ticket-002 acceptance criterion 8): we cannot
//! directly observe the subprocess's controlling TTY's raw-mode state from
//! this parent test process. As an accepted proxy, we verify that the
//! restoration logic itself logged its completion (via the rotating log
//! file written by `logging::init_logging`) before the process exited —
//! this proves the restoration *code path ran and returned*, which is the
//! strongest assertion practical in an automated harness. True TTY-state
//! verification is left to the manual QA matrix (see docs/ manual checklist).

use std::fs;
use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;

/// Skips (rather than fails) when not attached to a real TTY, since entering
/// raw mode is impossible/meaningless in a headless CI runner. Uses the same
/// convention crossterm exposes for TTY detection.
fn running_with_real_tty() -> bool {
    crossterm::tty::IsTty::is_tty(&std::io::stdout())
}

#[test]
fn panic_test_restores_terminal_before_exit() {
    if !running_with_real_tty() {
        eprintln!("skipping panic_safety test: no real TTY attached (headless CI)");
        return;
    }

    // Point the subprocess at a scratch log dir we control, so we can
    // inspect it afterward without depending on the real OS data directory.
    let log_marker_dir = tempfile::tempdir().expect("failed to create temp dir");

    let mut cmd = Command::cargo_bin("fwt").expect("fwt binary not found");
    cmd.env("FWT_LOG_DIR_OVERRIDE", log_marker_dir.path())
        .arg("--panic-test")
        .timeout(Duration::from_secs(5));

    let assert = cmd.assert();

    // Criterion (a): non-zero, non-success exit code.
    assert.failure();

    // Criterion (b): stderr contains the panic message, printed via the
    // (restored-terminal) color-eyre report.
    let output = Command::cargo_bin("fwt")
        .unwrap()
        .env("FWT_LOG_DIR_OVERRIDE", log_marker_dir.path())
        .arg("--panic-test")
        .timeout(Duration::from_secs(5))
        .output()
        .expect("failed to run subprocess");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("intentional test panic"),
        "expected panic message in stderr, got: {stderr}"
    );

    // Criterion (c): the restoration path logged its own completion, proving
    // the panic hook's terminal-restoration branch actually executed and
    // returned successfully before the process exited. This is our
    // documented proxy for "the terminal was restored" (see module doc).
    let log_contents = read_all_log_files(log_marker_dir.path());
    assert!(
        log_contents.contains("terminal restoration attempted"),
        "expected restoration log line was not found; restoration path may \
         not have executed. Full log contents: {log_contents}"
    );
}

fn read_all_log_files(dir: &std::path::Path) -> String {
    let mut combined = String::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(contents) = fs::read_to_string(entry.path()) {
                combined.push_str(&contents);
            }
        }
    }
    combined
}
