//! Typed recovery outcomes at the native transaction boundary.
use byakko_core::contract::{ApplyFailure, Recovery};
use std::{fmt, path::Path};

pub(super) type ApplyResult<T> = std::result::Result<T, ApplyFailure>;
pub(super) type RestoreResult = std::result::Result<(), RestoreFailure>;

pub(super) fn not_attempted(error: impl fmt::Display) -> ApplyFailure {
    ApplyFailure {
        message: error.to_string(),
        recovery: Recovery::NotAttempted,
    }
}

#[derive(Debug)]
pub(super) enum RestoreFailure {
    Mismatch(&'static str),
    Unverified(Box<dyn std::error::Error + Send + Sync>),
}
impl From<Box<dyn std::error::Error + Send + Sync>> for RestoreFailure {
    fn from(error: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Self::Unverified(error)
    }
}
impl fmt::Display for RestoreFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mismatch(message) => f.write_str(message),
            Self::Unverified(error) => fmt::Display::fmt(error, f),
        }
    }
}
fn rollback_outcome(rollback: RestoreResult, verified: &str) -> (Recovery, String) {
    match rollback {
        Ok(()) => (Recovery::Verified, verified.to_owned()),
        Err(error) => {
            let recovery = match &error {
                RestoreFailure::Mismatch(_) => Recovery::Failed,
                RestoreFailure::Unverified(_) => Recovery::Unverified,
            };
            let label = if recovery == Recovery::Failed {
                "FAILED"
            } else {
                "UNVERIFIED"
            };
            (recovery, format!("{label}: {error}"))
        }
    }
}

pub(super) fn keymap_apply_error(
    error: &dyn fmt::Display,
    rollback: RestoreResult,
    backup: &Path,
) -> ApplyFailure {
    let (recovery, restore) = rollback_outcome(rollback, "original keymaps verified");
    ApplyFailure {
        message: format!(
            "Apply failed: {error}. Restore result: {restore}. Backup: {}",
            backup.display()
        ),
        recovery,
    }
}

pub(super) fn macro_apply_error(
    error: &dyn fmt::Display,
    rollback: RestoreResult,
    backup: &Path,
) -> ApplyFailure {
    let (recovery, restore) = rollback_outcome(rollback, "verified");
    ApplyFailure {
        message: format!("{error}; restore: {restore}; backup {}", backup.display()),
        recovery,
    }
}

pub(super) fn lighting_apply_error(
    error: &dyn fmt::Display,
    rollback: RestoreResult,
    backup: &Path,
) -> ApplyFailure {
    let (recovery, restore) = rollback_outcome(
        rollback,
        "original setting and reserved response bytes verified",
    );
    ApplyFailure {
        message: format!(
            "Lighting apply failed: {error}. Restore result: {restore}. Backup: {}",
            backup.display()
        ),
        recovery,
    }
}

pub(super) fn picture_submit_error(error: &dyn fmt::Display, backup: &Path) -> ApplyFailure {
    ApplyFailure {
        message: format!(
            "Picture upload stopped after a transport error: {error}. Device state is unknown; no automatic restore sent. Backup: {}",
            backup.display()
        ),
        recovery: Recovery::Unverified,
    }
}

pub(super) fn settings_apply_error(
    error: &dyn fmt::Display,
    rollback: RestoreResult,
    backup: &Path,
) -> ApplyFailure {
    let (recovery, restore) = rollback_outcome(rollback, "original settings verified");
    ApplyFailure {
        message: format!(
            "Settings apply failed: {error}. Restore result: {restore}. Backup: {}",
            backup.display()
        ),
        recovery,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_rollback_path_preserves_classification_and_diagnostics() {
        type Build = fn(&dyn fmt::Display, RestoreResult, &Path) -> ApplyFailure;
        for build in [
            keymap_apply_error as Build,
            macro_apply_error,
            settings_apply_error,
            lighting_apply_error,
        ] {
            for (rollback, expected) in [
                (Ok(()), Recovery::Verified),
                (
                    Err(RestoreFailure::Mismatch("restore mismatch")),
                    Recovery::Failed,
                ),
                (
                    Err(RestoreFailure::Unverified("read failed".into())),
                    Recovery::Unverified,
                ),
            ] {
                let failure = build(&"apply failed", rollback, Path::new("before.json"));
                assert_eq!(failure.recovery, expected);
                assert!(failure.message.contains("apply failed"));
                assert!(failure.message.contains("before.json"));
            }
        }
        assert_eq!(
            not_attempted("invalid input").recovery,
            Recovery::NotAttempted
        );
        let failure = picture_submit_error(&"write failed", Path::new("picture-before.json"));
        assert_eq!(failure.recovery, Recovery::Unverified);
        assert!(failure.message.contains("no automatic restore"));
    }
}
