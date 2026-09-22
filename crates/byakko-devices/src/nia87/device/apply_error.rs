//! Typed recovery outcomes at the native transaction boundary.
use super::Result;
use byakko_core::session::{ApplyFailure, Recovery};
use std::{fmt, path::Path};

#[derive(Debug)]
pub(super) struct ApplyError(pub(super) ApplyFailure);

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.message)
    }
}
impl std::error::Error for ApplyError {}

/// Untyped errors are pre-write rejections. Every post-write error must be
/// wrapped in ApplyError at its transaction's recovery branch.
pub(super) fn detailed<T>(result: Result<T>) -> std::result::Result<T, ApplyFailure> {
    result.map_err(|error| {
        error
            .downcast_ref::<ApplyError>()
            .map(|typed| typed.0.clone())
            .unwrap_or_else(|| ApplyFailure {
                message: error.to_string(),
                recovery: Recovery::NotAttempted,
            })
    })
}

pub(super) fn keymap_apply_error(
    error: &dyn fmt::Display,
    rollback: Result<()>,
    backup: &Path,
) -> ApplyError {
    let (recovery, restore) = match rollback {
        Ok(()) => (Recovery::Verified, "original keymaps verified".to_owned()),
        Err(error) => (Recovery::Failed, format!("FAILED: {error}")),
    };
    ApplyError(ApplyFailure {
        message: format!(
            "Apply failed: {error}. Restore result: {restore}. Backup: {}",
            backup.display()
        ),
        recovery,
    })
}

pub(super) fn macro_apply_error(
    error: &dyn fmt::Display,
    rollback: Result<()>,
    backup: &Path,
) -> ApplyError {
    let (recovery, restore) = match rollback {
        Ok(()) => (Recovery::Verified, "verified".to_owned()),
        Err(error) => (Recovery::Failed, error.to_string()),
    };
    ApplyError(ApplyFailure {
        message: format!("{error}; restore: {restore}; backup {}", backup.display()),
        recovery,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn macro_recovery_keeps_diagnostic_and_typed_outcome() {
        let path = Path::new("before.json");
        let restored = macro_apply_error(&"write failed", Ok(()), path);
        assert_eq!(restored.0.recovery, Recovery::Verified);
        assert_eq!(
            restored.0.message,
            "write failed; restore: verified; backup before.json"
        );
        let failed = macro_apply_error(&"write failed", Err("readback mismatch".into()), path);
        assert_eq!(failed.0.recovery, Recovery::Failed);
        assert_eq!(
            failed.0.message,
            "write failed; restore: readback mismatch; backup before.json"
        );
        let failure = detailed::<()>(Err(failed.into())).unwrap_err();
        assert_eq!(failure.recovery, Recovery::Failed);
        assert_eq!(
            detailed::<()>(Err("invalid input".into()))
                .unwrap_err()
                .recovery,
            Recovery::NotAttempted
        );
    }
}
