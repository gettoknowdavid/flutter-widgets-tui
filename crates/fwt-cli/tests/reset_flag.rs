//! Verifies the --reset flag's confirmation gate end-to-end via subprocess,
//! complementing reset.rs's in-process unit tests. This does NOT verify any
//! actual data clearing (there is none yet, per the Epic 1 stub) — only that
//! declining confirmation exits cleanly and cannot be bypassed by accident.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn reset_declines_on_non_yes_input_and_exits_cleanly() {
    let mut cmd = Command::cargo_bin("fwt").unwrap();
    cmd.arg("--reset")
        .write_stdin("no\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Reset cancelled"));
}

#[test]
fn reset_confirms_on_exact_yes_input() {
    let mut cmd = Command::cargo_bin("fwt").unwrap();
    cmd.arg("--reset")
        .write_stdin("yes\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("nothing was actually deleted"));
}
