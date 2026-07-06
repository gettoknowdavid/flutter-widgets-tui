//! # xtask — Workspace Dev Tooling
//!
//! This crate is **not** part of the product's architectural layering
//! (Presentation → Application → Domain ← Infrastructure, TRD Section
//! 2.1). It lives at the workspace root, as a sibling to `crates/`, and
//! hosts commands that operate on the workspace as a whole rather than
//! any single crate.
//!
//! Run via the `cargo xtask` alias (see `.cargo/config.toml`), e.g.:
//!
//!     cargo xtask check-boundaries
//!
//! ## Why this exists
//! Per Ticket 001 / ADR-000, `fwt-domain` must never depend — directly
//! or transitively, under any feature flag — on an I/O-bearing crate
//! (`ratatui`, `rusqlite`, `tokio`, `reqwest`, `crossterm`). This is the
//! automated enforcement of that rule: a `cargo build` alone can't catch
//! a transitive leak (some innocuous crate quietly pulling in `tokio`
//! behind a feature), but walking the *resolved* dependency graph can.
//!
//! As the project grows, expect more subcommands here — e.g. a future
//! `cargo xtask seed-catalog` to regenerate `catalog.db` from
//! `assets/catalog_seed/`, or `cargo xtask check-themes` to validate
//! Epic 6's theme `.toml` files. Add new subcommands as new functions
//! dispatched from `main()`, following the same pattern as
//! `check_boundaries` below.

use cargo_metadata::{CargoOpt, MetadataCommand, Package, PackageId};
use std::collections::{HashMap, HashSet, VecDeque};

/// Crates that must never appear anywhere in `fwt-domain`'s dependency
/// closure. Per TRD Section 2.1: fwt-domain is pure data + port traits,
/// zero I/O, zero framework dependency.
const FORBIDDEN_FOR_DOMAIN: &[&str] = &["ratatui", "sqlx", "tokio", "reqwest", "crossterm"];

/// The package name of the crate we're protecting. Kept as a constant
/// in case the crate is ever renamed.
const PROTECTED_CRATE: &str = "fwt-domain";

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("check-boundaries") | None => run_check_boundaries(),
        Some("help") | Some("--help") | Some("-h") => {
            print_help();
            std::process::ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("xtask: unknown command '{other}'\n");
            print_help();
            std::process::ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!(
        "xtask — workspace dev tooling\n\
         \n\
         USAGE:\n\
         \x20\x20cargo xtask [COMMAND]\n\
         \n\
         COMMANDS:\n\
         \x20\x20check-boundaries   Verify fwt-domain has no forbidden dependencies (default if no command given)\n\
         \x20\x20help                Print this message\n"
    );
}

/// Runs the dependency-boundary check against both the default feature
/// resolution and the all-features resolution, printing a clear report
/// either way. Returns a process exit code suitable for CI.
fn run_check_boundaries() -> std::process::ExitCode {
    println!("xtask: checking dependency boundaries for '{PROTECTED_CRATE}'...\n");

    let default_result = check_once(CheckMode::DefaultFeatures);
    let all_features_result = check_once(CheckMode::AllFeatures);

    let mut ok = true;

    match default_result {
        Ok(()) => println!("  [OK]   default features: clean"),
        Err(violations) => {
            ok = false;
            report_violations("default features", &violations);
        }
    }
    match all_features_result {
        Ok(()) => println!("  [OK]   all features: clean"),
        Err(violations) => {
            ok = false;
            report_violations("all features", &violations);
        }
    }

    println!();
    if ok {
        println!("xtask: boundary check passed. '{PROTECTED_CRATE}' remains pure.");
        std::process::ExitCode::SUCCESS
    } else {
        println!(
            "xtask: boundary check FAILED. '{PROTECTED_CRATE}' must not depend on \
             I/O-bearing crates, even transitively. If you need this functionality, \
             define a port trait in '{PROTECTED_CRATE}' and implement it in 'fwt-infra' \
             instead. See docs/adr/adr-000-workspace-structure.md."
        );
        std::process::ExitCode::FAILURE
    }
}

fn report_violations(label: &str, violations: &[String]) {
    println!("  [FAIL] {label}: forbidden crate(s) found in dependency closure:");
    for v in violations {
        println!("           - {v}");
    }
}

enum CheckMode {
    DefaultFeatures,
    AllFeatures,
}

/// Runs `cargo metadata` once (via the `cargo_metadata` crate, which
/// shells out to `cargo metadata --format-version 1` under the hood and
/// gives us a typed result instead of hand-parsed JSON), then walks the
/// resolved graph starting from `fwt-domain` and returns any forbidden
/// crate names found in its transitive closure.
fn check_once(mode: CheckMode) -> Result<(), Vec<String>> {
    let mut cmd = MetadataCommand::new();
    match mode {
        CheckMode::DefaultFeatures => {}
        CheckMode::AllFeatures => {
            cmd.features(CargoOpt::AllFeatures);
        }
    }

    let metadata = cmd
        .exec()
        .expect("failed to run `cargo metadata` — is this being run from within the workspace?");

    let resolve = metadata
        .resolve
        .as_ref()
        .expect("cargo metadata did not return a resolve graph");

    // Build lookup tables: PackageId -> Package (for names), and
    // PackageId -> its direct dependency PackageIds (for graph walking).
    let packages_by_id: HashMap<&PackageId, &Package> =
        metadata.packages.iter().map(|p| (&p.id, p)).collect();

    let deps_by_id: HashMap<&PackageId, &[PackageId]> = resolve
        .nodes
        .iter()
        .map(|node| (&node.id, node.dependencies.as_slice()))
        .collect();

    let domain_id = resolve
        .nodes
        .iter()
        .find(|node| {
            packages_by_id
                .get(&node.id)
                .map(|pkg| pkg.name == PROTECTED_CRATE)
                .unwrap_or(false)
        })
        .map(|node| &node.id)
        .unwrap_or_else(|| {
            panic!(
                "could not find package '{PROTECTED_CRATE}' in cargo metadata output — \
                 has it been renamed or removed from the workspace?"
            )
        });

    // Breadth-first walk of the full transitive dependency closure
    // starting at fwt-domain, collecting every package name reachable
    // from it.
    let mut visited: HashSet<&PackageId> = HashSet::new();
    let mut queue: VecDeque<&PackageId> = VecDeque::new();
    queue.push_back(domain_id);

    let mut closure_names: HashSet<&str> = HashSet::new();

    while let Some(id) = queue.pop_front() {
        if !visited.insert(id) {
            continue;
        }
        if let Some(pkg) = packages_by_id.get(id) {
            closure_names.insert(pkg.name.as_str());
        }
        if let Some(deps) = deps_by_id.get(id) {
            for dep_id in deps.iter() {
                queue.push_back(dep_id);
            }
        }
    }

    let violations: Vec<String> = FORBIDDEN_FOR_DOMAIN
        .iter()
        .filter(|forbidden| closure_names.contains(*forbidden))
        .map(|s| s.to_string())
        .collect();

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This is a smoke test, not a substitute for actually running
    /// `cargo xtask check-boundaries` — it just proves the constant
    /// list itself isn't accidentally emptied by a future edit.
    #[test]
    fn forbidden_list_is_not_empty() {
        assert!(!FORBIDDEN_FOR_DOMAIN.is_empty());
    }

    #[test]
    fn protected_crate_name_matches_expected() {
        assert_eq!(PROTECTED_CRATE, "fwt-domain");
    }
}
