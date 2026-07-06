//! Trivial smoke test proving the binary target links and runs without
//! panicking. Expanded meaningfully in Ticket 003.

use std::process::Command;

#[test]
fn binary_runs_without_panicking() {
    let status = Command::new(env!("CARGO_BIN_EXE_fwt"))
        .status()
        .expect("failed to execute compiled binary");
    assert!(status.success());
}
