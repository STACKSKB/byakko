//! User-facing explanations preserve the distinction between recovery outcomes.
use byakko_core::contract::{ApplyFailure, Problem, Recovery};
use byakko_core::editor::Status;

/// Keep unresolved feature problems visible independently of transient notices.
pub fn editor_attention<S>(feature: &str, status: &Status<S>) -> Option<String> {
    match status {
        Status::Ready | Status::Unloaded => None,
        Status::Conflict { .. } => Some(format!(
            "{feature} changed on the keyboard. Edits kept; Revert, then Read to reload."
        )),
        Status::Unverified { problem } => Some(format!("{feature}: {}", problem_text(problem))),
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
        Problem::ReadRequired => "Read / reconnect to load.".into(),
        Problem::Read(_) => "Read failed. Try Read / reconnect.".into(),
        Problem::Apply(failure) => match failure.recovery {
            Recovery::Verified => "Save failed; previous state restored. Edits kept.".into(),
            Recovery::Failed => "Save and recovery failed. Edits kept; Read / reconnect.".into(),
            Recovery::Unverified => {
                "Save failed; recovery unverified. Edits kept; Read / reconnect.".into()
            }
            Recovery::NotAttempted => {
                "Save failed; recovery not attempted. Edits kept; Read / reconnect.".into()
            }
        },
        Problem::InvalidApplyResult(_) => "Save unverified. Edits kept; Read / reconnect.".into(),
        Problem::ApplyReadbackMismatch => "Readback differs. Edits kept; Read / reconnect.".into(),
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
            let message = apply_failure(
                "Could not save to the keyboard",
                &ApplyFailure {
                    message: "The keyboard was disconnected.".into(),
                    recovery: recovery.clone(),
                },
            );
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
