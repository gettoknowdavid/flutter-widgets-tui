//! `--reset` confirmation flow.
//!
//! Scope note (Epic 1 / Ticket 002): the actual clearing of `user.db` is
//! Epic 4 territory (`UserDataRepository` doesn't exist yet). What belongs
//! here, now, is the *safety-critical* part: a confirmation prompt that
//! cannot be accidentally bypassed, structured so it's fully unit-testable
//! without a real terminal or real stdin.
//!
//! This intentionally runs BEFORE `TerminalGuard::enter()` in main.rs — it's
//! a plain cooked-mode prompt, not a TUI screen. Raw mode / alt screen has no
//! business being active for a yes/no confirmation.

use std::io::{self, BufRead, Write};

#[derive(Debug, thiserror::Error)]
pub enum ResetError {
    #[error("failed to read confirmation input")]
    ReadInput(#[source] io::Error),

    #[error("failed to write confirmation prompt")]
    WriteOutput(#[source] io::Error),

    #[error("reset was not confirmed; no data was cleared")]
    NotConfirmed,
}
impl PartialEq for ResetError {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (ResetError::NotConfirmed, ResetError::NotConfirmed)
                | (ResetError::ReadInput(_), ResetError::ReadInput(_))
                | (ResetError::WriteOutput(_), ResetError::WriteOutput(_))
        )
    }
}

/// Outcome of a confirmed reset. Deliberately does *not* claim data was
/// cleared yet — Epic 1 has no data layer to clear. `fwt-cli::main` logs
/// and exits after this; Epic 4 is expected to replace the `Ok(Confirmed)`
/// handling with an actual `UserDataRepository::reset_all()` call.
#[derive(Debug, PartialEq, Eq)]
pub enum ResetOutcome {
    Confirmed,
}

/// Runs the interactive confirmation flow against arbitrary reader/writer,
/// so it's testable with an in-memory `Cursor` instead of real stdin/stdout.
///
/// Requires the user to type the literal string `yes` (case-sensitive, no
/// trimmed-and-lowercased fuzzy matching) — a destructive, irreversible-once-
/// Epic-4-lands operation should not be triggerable by an accidental Enter
/// keypress or a stray "y".
pub fn confirm_reset<R: BufRead, W: Write>(
    mut reader: R,
    mut writer: W,
) -> Result<ResetOutcome, ResetError> {
    writeln!(
        writer,
        "This will permanently delete your favorites, history, chat sessions, \
         and local settings. This cannot be undone.\n\
         Type 'yes' to confirm, or anything else to cancel:"
    )
    .map_err(ResetError::WriteOutput)?;
    writer.flush().map_err(ResetError::WriteOutput)?;

    let mut input = String::new();
    reader
        .read_line(&mut input)
        .map_err(ResetError::ReadInput)?;

    // Deliberately strict: only a trailing newline is stripped, nothing else
    // is normalized. "Yes", " yes", "yes " etc. all count as non-confirmation.
    if input.trim_end_matches(['\n', '\r']) == "yes" {
        Ok(ResetOutcome::Confirmed)
    } else {
        Err(ResetError::NotConfirmed)
    }
}

/// The Epic 1 stub for the actual data-clearing action.
///
/// TODO(Epic 4): replace this body with a real call into
/// `UserDataRepository::reset_all()` once `user.db` and its repository
/// trait exist (TRD Section 4.3 / 4.4). Until then, this function's job is
/// only to make the seam visible and to give `main.rs` a single, obvious
/// call site to update — not to silently pretend data was cleared.
pub fn perform_reset_stub() {
    tracing::warn!(
        "reset confirmed, but no user data store exists yet (Epic 1); \
         this is a no-op until Epic 4 wires up UserDataRepository::reset_all()"
    );
    eprintln!(
        "Note: no local data store exists yet in this build — nothing was \
         actually deleted. (This flag is fully wired for confirmation UX; \
         actual data clearing lands in a later release.)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn run_with_input(input: &str) -> Result<ResetOutcome, ResetError> {
        let reader = Cursor::new(input.as_bytes());
        let mut output = Vec::new();
        let result = confirm_reset(reader, &mut output);
        // Sanity: the prompt itself was always written, regardless of outcome.
        let written = String::from_utf8(output).unwrap();
        assert!(written.contains("Type 'yes' to confirm"));
        result
    }

    #[test]
    fn exact_yes_confirms() {
        assert_eq!(run_with_input("yes\n"), Ok(ResetOutcome::Confirmed));
    }

    #[test]
    fn exact_yes_without_trailing_newline_confirms() {
        // read_line on a Cursor without a final \n still returns the content;
        // guard this explicitly since EOF-without-newline is a real input path.
        assert_eq!(run_with_input("yes"), Ok(ResetOutcome::Confirmed));
    }

    #[test]
    fn bare_enter_does_not_confirm() {
        assert!(matches!(
            run_with_input("\n"),
            Err(ResetError::NotConfirmed)
        ));
    }

    #[test]
    fn lowercase_y_does_not_confirm() {
        assert!(matches!(
            run_with_input("y\n"),
            Err(ResetError::NotConfirmed)
        ));
    }

    #[test]
    fn capitalized_yes_does_not_confirm() {
        // Deliberately strict per the doc comment: no case-folding.
        assert!(matches!(
            run_with_input("Yes\n"),
            Err(ResetError::NotConfirmed)
        ));
    }

    #[test]
    fn yes_with_trailing_whitespace_does_not_confirm() {
        assert!(matches!(
            run_with_input("yes \n"),
            Err(ResetError::NotConfirmed)
        ));
    }

    #[test]
    fn arbitrary_garbage_does_not_confirm() {
        assert!(matches!(
            run_with_input("sure whatever\n"),
            Err(ResetError::NotConfirmed)
        ));
    }

    #[test]
    fn empty_input_does_not_confirm() {
        assert!(matches!(run_with_input(""), Err(ResetError::NotConfirmed)));
    }
}
