//! # fwt-cli — Composition Root
//!
//! The only crate permitted to depend on all four other crates. Its job
//! (in later tickets) is: parse CLI flags, construct concrete `fwt-infra`
//! adapters, inject them into `fwt-app` services, and hand control to
//! `fwt-tui::run()`. For now it does the bare minimum to prove the whole
//! workspace links correctly end-to-end.

fn main() {
    if let Err(err) = fwt_tui::run() {
        eprintln!("fatal error: {err}");
        std::process::exit(1);
    }
}
