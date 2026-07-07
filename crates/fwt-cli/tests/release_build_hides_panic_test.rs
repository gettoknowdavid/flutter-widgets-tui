#[test]
#[cfg(not(debug_assertions))]
fn panic_test_flag_absent_in_help_output_for_release_builds() {
    use assert_cmd::Command;
    let mut cmd = Command::cargo_bin("fwt").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("--panic-test").not());
}
