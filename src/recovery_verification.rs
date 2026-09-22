//! Readback verification after a configuration operation.

/// Verify a complete capture, retrying the capture once only if the first read fails.
///
/// The caller supplies a fresh-handle, complete-capture retry. This function never
/// retries a setter and never treats a successful but mismatched capture as a read error.
pub(crate) fn verify<T: PartialEq>(
    expected: &T,
    first: Result<T, String>,
    retry: impl FnOnce() -> Result<T, String>,
) -> Result<(), String> {
    match first {
        Ok(actual) if &actual == expected => Ok(()),
        Ok(_) => Err("configuration readback did not match expected state".to_owned()),
        Err(original_error) => match retry() {
            Ok(actual) if &actual == expected => Ok(()),
            Ok(_) => Err(format!(
                "configuration readback failed ({original_error}); retry did not match expected state"
            )),
            Err(retry_error) => Err(format!(
                "configuration readback failed ({original_error}); retry failed ({retry_error})"
            )),
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
        assert!(error.contains("did not match"));
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
        assert!(error.contains("first read"));
        assert!(error.contains("retry did not match"));
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
        assert!(error.contains("first read"));
        assert!(error.contains("second read"));
        assert_eq!(calls.get(), 1);
    }
}
