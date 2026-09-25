//! Structured readback verification after a configuration operation.
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VerificationFailure<T> {
    Mismatch {
        actual: Box<T>,
        first_read_error: Option<String>,
    },
    Unreadable {
        first_read_error: String,
        retry_error: String,
    },
}

impl<T> fmt::Display for VerificationFailure<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mismatch {
                first_read_error: None,
                ..
            } => f.write_str("configuration readback did not match expected state"),
            Self::Mismatch {
                first_read_error: Some(error),
                ..
            } => write!(
                f,
                "configuration readback failed ({error}); retry did not match expected state"
            ),
            Self::Unreadable {
                first_read_error,
                retry_error,
            } => write!(
                f,
                "configuration readback failed ({first_read_error}); retry failed ({retry_error})"
            ),
        }
    }
}

/// Verify a complete capture, retrying the capture once only if the first read fails.
///
/// The caller supplies a fresh-handle, complete-capture retry. This function never
/// retries a setter and never treats a successful but mismatched capture as a read error.
pub(crate) fn verify<T: PartialEq>(
    expected: &T,
    first: Result<T, String>,
    retry: impl FnOnce() -> Result<T, String>,
) -> Result<(), VerificationFailure<T>> {
    match first {
        Ok(actual) if &actual == expected => Ok(()),
        Ok(actual) => Err(VerificationFailure::Mismatch {
            actual: Box::new(actual),
            first_read_error: None,
        }),
        Err(original_error) => match retry() {
            Ok(actual) if &actual == expected => Ok(()),
            Ok(actual) => Err(VerificationFailure::Mismatch {
                actual: Box::new(actual),
                first_read_error: Some(original_error),
            }),
            Err(retry_error) => Err(VerificationFailure::Unreadable {
                first_read_error: original_error,
                retry_error,
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::verify;
    use std::cell::Cell;

    #[test]
    fn matching_first_capture_does_not_retry() {
        let calls = Cell::new(0);
        assert_eq!(
            verify(&7, Ok(7), || {
                calls.set(calls.get() + 1);
                Ok(7)
            }),
            Ok(())
        );
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn mismatched_first_capture_does_not_retry() {
        let calls = Cell::new(0);
        let error = verify(&7, Ok(8), || {
            calls.set(calls.get() + 1);
            Ok(7)
        })
        .unwrap_err();
        assert!(matches!(
            error,
            super::VerificationFailure::Mismatch {
                first_read_error: None, actual
            } if *actual == 8
        ));
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn failed_first_capture_retries_once_and_accepts_match() {
        let calls = Cell::new(0);
        assert_eq!(
            verify(&7, Err("first read".into()), || {
                calls.set(calls.get() + 1);
                Ok(7)
            }),
            Ok(())
        );
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn failed_first_capture_reports_retry_mismatch() {
        let calls = Cell::new(0);
        let error = verify(&7, Err("first read".into()), || {
            calls.set(calls.get() + 1);
            Ok(8)
        })
        .unwrap_err();
        assert!(
            matches!(error, super::VerificationFailure::Mismatch { first_read_error: Some(message), actual } if message == "first read" && *actual == 8)
        );
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn failed_first_capture_reports_both_read_errors() {
        let calls = Cell::new(0);
        let error = verify(&7, Err("first read".into()), || {
            calls.set(calls.get() + 1);
            Err("second read".into())
        })
        .unwrap_err();
        assert!(
            matches!(error, super::VerificationFailure::Unreadable { first_read_error, retry_error } if first_read_error == "first read" && retry_error == "second read")
        );
        assert_eq!(calls.get(), 1);
    }
}
