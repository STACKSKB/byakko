//! Shared durable backup and guarded apply sequence for Nia87 feature writes.
//!
//! The caller owns the session lock and HID handle. In particular, a failed
//! write restores through that same selected collection; this helper never
//! substitutes another keyboard. Feature-specific reads, comparisons, and
//! recovery plans remain in their feature modules.
use super::Result;
use super::apply_error::{ApplyError, RestoreMismatch};
use serde::Serialize;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Observed firmware 0100 pacing. These delays separate HID requests, not UI
/// edits. Change them only with capture or physical acceptance evidence.
pub(super) mod pacing {
    use super::Duration;

    /// Mixed base/Fn writes crossed maps at 100 ms; 1 s passed write/restore.
    pub const KEYMAP_SETTER: Duration = Duration::from_secs(1);
    /// Five fixed macro pages, with a separate flash-settle period afterward.
    pub const MACRO_PAGE: Duration = Duration::from_millis(30);
    pub const MACRO_SETTLE: Duration = Duration::from_millis(200);
    /// Only retry a complete macro getter after a mismatched first readback.
    pub const MACRO_READBACK_MISMATCH: Duration = Duration::from_millis(200);
    /// The captured official picture upload schedules two 10 ms waits per page.
    pub const PICTURE_PAGE: Duration = Duration::from_millis(20);
    /// Developer archive restore uses individual per-key color reports.
    pub const ARCHIVE_PICTURE_KEY: Duration = Duration::from_millis(100);
    /// Scalar and global LED setters have known 500 ms settling intervals.
    pub const SETTING_SETTER: Duration = Duration::from_millis(500);
    pub const LIGHTING_SETTER: Duration = Duration::from_millis(500);
}

pub(super) struct DurableBackup {
    path: PathBuf,
    stamp: u128,
}

impl DurableBackup {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn stamp(&self) -> u128 {
        self.stamp
    }
}

/// Create a fresh backup and sync it before returning a transaction token.
/// Callers supply only the bytes or serialization; file creation and durability
/// cannot be skipped by a feature implementation.
pub(super) fn save_custom_backup(
    directory: &Path,
    stem: &str,
    write: impl FnOnce(&mut std::fs::File) -> Result<()>,
) -> Result<DurableBackup> {
    std::fs::create_dir_all(directory)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = directory.join(format!("{stem}-{stamp}.json"));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    write(&mut file)?;
    file.sync_all()?;
    Ok(DurableBackup { path, stamp })
}

pub(super) fn save_json_backup<T: Serialize + ?Sized>(
    directory: &Path,
    stem: &str,
    before: &T,
) -> Result<DurableBackup> {
    save_custom_backup(directory, stem, |file| {
        serde_json::to_writer_pretty(file, before)?;
        Ok(())
    })
}

pub(super) fn save_encoded_backup(
    directory: &Path,
    stem: &str,
    encoded: &[u8],
) -> Result<DurableBackup> {
    save_custom_backup(directory, stem, |file| {
        file.write_all(encoded)?;
        Ok(())
    })
}

/// Run the feature's setter and single verification, then its precise
/// restoration plan on failure. The backup must already be durable. The
/// feature supplies its existing error formatter to retain recovery wording.
pub(super) fn apply_with_recovery<T>(
    backup: &DurableBackup,
    apply: impl FnOnce() -> Result<T>,
    restore: impl FnOnce() -> Result<()>,
    error: fn(&dyn fmt::Display, Result<()>, &Path) -> ApplyError,
) -> Result<T> {
    match apply() {
        Ok(value) => Ok(value),
        Err(cause) => {
            let recovery = restore();
            Err(error(cause.as_ref(), recovery, backup.path()).into())
        }
    }
}

/// For features with a direct before/after setter, keep the ordered write,
/// complete readback, comparison, and verified restore in one place. Only a
/// mismatched first read may be retried (the observed macro flash case).
pub(super) struct VerifiedStep<W, M> {
    pub write: W,
    pub matches: M,
    pub mismatch: &'static str,
}

pub(super) fn apply_roundtrip<T, TW, TM, BW, BM>(
    backup: &DurableBackup,
    target: VerifiedStep<TW, TM>,
    before: VerifiedStep<BW, BM>,
    read: impl Fn() -> Result<T>,
    mismatch_retry: Option<Duration>,
    error: fn(&dyn fmt::Display, Result<()>, &Path) -> ApplyError,
) -> Result<T>
where
    TW: FnOnce() -> Result<()>,
    TM: Fn(&T) -> bool,
    BW: FnOnce() -> Result<()>,
    BM: Fn(&T) -> bool,
{
    apply_with_recovery(
        backup,
        || {
            (target.write)()?;
            let mut actual = read()?;
            if let (false, Some(delay)) = ((target.matches)(&actual), mismatch_retry) {
                std::thread::sleep(delay);
                actual = read()?;
            }
            if (target.matches)(&actual) {
                Ok(actual)
            } else {
                Err(target.mismatch.into())
            }
        },
        || {
            (before.write)()?;
            if (before.matches)(&read()?) {
                Ok(())
            } else {
                Err(RestoreMismatch(before.mismatch).into())
            }
        },
        error,
    )
}

#[cfg(test)]
mod tests {
    use super::super::apply_error::{RestoreMismatch, macro_apply_error};
    use super::*;
    use byakko_core::session::Recovery;

    #[test]
    fn verified_path_never_restores_and_failure_preserves_recovery_status() {
        let backup = DurableBackup {
            path: "before.json".into(),
            stamp: 1,
        };
        let mut restored = false;
        let value = apply_with_recovery(
            &backup,
            || Ok(42),
            || {
                restored = true;
                Ok(())
            },
            macro_apply_error,
        )
        .unwrap();
        assert_eq!(value, 42);
        assert!(!restored);

        for (restore, expected) in [
            (Ok(()), Recovery::Verified),
            (
                Err(RestoreMismatch("restore readback mismatch").into()),
                Recovery::Failed,
            ),
            (Err("restore read failed".into()), Recovery::Unverified),
        ] {
            let failure = super::super::apply_error::detailed::<()>(apply_with_recovery(
                &backup,
                || Err("readback mismatch".into()),
                || restore,
                macro_apply_error,
            ))
            .unwrap_err();
            assert_eq!(failure.recovery, expected);
            assert!(failure.message.contains("readback mismatch"));
            assert!(failure.message.contains("before.json"));
        }
    }

    #[test]
    fn backup_error_stops_before_setter() {
        let mut setter_called = false;
        let transaction = (|| -> Result<()> {
            let backup = save_custom_backup(&std::env::current_exe()?, "unused", |_| {
                Err("backup storage failed".into())
            })?;
            apply_with_recovery(
                &backup,
                || {
                    setter_called = true;
                    Ok(())
                },
                || Ok(()),
                macro_apply_error,
            )
        })();
        assert!(!transaction.unwrap_err().to_string().is_empty());
        assert!(!setter_called);
    }

    #[test]
    fn roundtrip_retries_only_a_mismatch_and_restores_after_read_error() {
        let backup = DurableBackup {
            path: "before.json".into(),
            stamp: 1,
        };
        let steps = std::cell::RefCell::new(Vec::new());
        let reads = std::cell::Cell::new(0);
        let actual = apply_roundtrip(
            &backup,
            VerifiedStep {
                write: || {
                    steps.borrow_mut().push("write target");
                    Ok(())
                },
                matches: |value: &i32| *value == 2,
                mismatch: "mismatch",
            },
            VerifiedStep {
                write: || {
                    steps.borrow_mut().push("restore");
                    Ok(())
                },
                matches: |value: &i32| *value == 1,
                mismatch: "restore mismatch",
            },
            || {
                steps.borrow_mut().push("read");
                reads.set(reads.get() + 1);
                Ok(if reads.get() == 1 { 0 } else { 2 })
            },
            Some(Duration::ZERO),
            macro_apply_error,
        )
        .unwrap();
        assert_eq!(actual, 2);
        assert_eq!(*steps.borrow(), ["write target", "read", "read"]);

        steps.borrow_mut().clear();
        reads.set(0);
        let failure = super::super::apply_error::detailed::<i32>(apply_roundtrip(
            &backup,
            VerifiedStep {
                write: || {
                    steps.borrow_mut().push("write target");
                    Ok(())
                },
                matches: |value: &i32| *value == 2,
                mismatch: "mismatch",
            },
            VerifiedStep {
                write: || {
                    steps.borrow_mut().push("restore");
                    Ok(())
                },
                matches: |value: &i32| *value == 1,
                mismatch: "restore mismatch",
            },
            || {
                steps.borrow_mut().push("read");
                reads.set(reads.get() + 1);
                if reads.get() == 1 {
                    Err("read failed".into())
                } else {
                    Ok(1)
                }
            },
            Some(Duration::ZERO),
            macro_apply_error,
        ))
        .unwrap_err();
        assert_eq!(failure.recovery, Recovery::Verified);
        assert!(failure.message.contains("read failed"));
        assert_eq!(*steps.borrow(), ["write target", "read", "restore", "read"]);
    }
}
