//! User-facing explanations preserve the distinction between recovery outcomes.
use byakko_core::contract::{ApplyFailure, Problem, Recovery};
use byakko_core::editor::Status;

/// Keep unresolved feature problems visible independently of transient notices.
pub fn editor_attention<S>(status: &Status<S>) -> Option<String> {
    match status {
        Status::Ready | Status::Unloaded => None,
        Status::Conflict { .. } => Some(
            "The keyboard changed while you were editing. Your edits are kept. To load the keyboard's changes, revert your edits, then read this feature again.".into(),
        ),
        Status::Unverified { problem } => Some(problem_text(problem)),
    }
}

pub fn apply_failure(action: &str, failure: &ApplyFailure) -> String {
    let recovery = match failure.recovery {
        Recovery::Verified => "The previous settings were restored.",
        Recovery::Failed => {
            "The previous settings could not be restored. Reconnect the keyboard and choose Read / reconnect before trying again."
        }
        Recovery::Unverified => {
            "The keyboard's current settings could not be confirmed. Reconnect the keyboard and choose Read / reconnect before trying again."
        }
        Recovery::NotAttempted => "Choose Read / reconnect before trying again.",
    };
    format!(
        "{action}: {} {recovery} Your edits are kept.",
        failure.message
    )
}

pub fn problem_text(problem: &Problem) -> String {
    match problem {
        Problem::ReadRequired => "Choose Read / reconnect to load the keyboard's settings.".into(),
        Problem::Read(reason) => format!("Could not read the keyboard: {reason}"),
        Problem::Apply(failure) => apply_failure("Could not save to the keyboard", failure),
        Problem::InvalidApplyResult(reason) => format!("Could not confirm the save: {reason}. Your edits are kept. Choose Read / reconnect before trying again."),
        Problem::ApplyReadbackMismatch => "The keyboard's settings did not match the changes you sent. Your edits are kept. Choose Read / reconnect before trying again.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_messages_only_claim_restoration_when_verified() {
        for (recovery, explanation) in [
            (Recovery::Verified, "The previous settings were restored."),
            (
                Recovery::Failed,
                "The previous settings could not be restored.",
            ),
            (
                Recovery::Unverified,
                "The keyboard's current settings could not be confirmed.",
            ),
            (
                Recovery::NotAttempted,
                "Choose Read / reconnect before trying again.",
            ),
        ] {
            let message = problem_text(&Problem::Apply(ApplyFailure {
                message: "The keyboard was disconnected.".into(),
                recovery: recovery.clone(),
            }));
            assert!(message.contains(explanation));
            assert!(message.contains("Your edits are kept."));
            assert!(!message.contains("Recovery:"));
            assert_eq!(
                message.contains("settings were restored"),
                recovery == Recovery::Verified
            );
        }
    }
}
