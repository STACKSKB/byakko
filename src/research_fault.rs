//! Scoped, single-shot setter fault injection for transport recovery research.

use std::cell::RefCell;
use std::io;

struct Arm {
    opcode: u8,
    after_delivery: bool,
    fired: bool,
}

thread_local! {
    static ARM: RefCell<Option<Arm>> = const { RefCell::new(None) };
}

const SETTER_OPCODES: &[u8] = &[0x16, 0x13, 0x15, 0x14, 0x11, 0x17, 0x12, 0x06, 0x07];

/// Run `work` with one setter transmission armed to fail on this thread.
///
/// A nested scope is invalid. The arm is cleared even if `work` panics.
pub fn with_fault<T>(opcode: u8, after_delivery: bool, work: impl FnOnce() -> T) -> (T, bool) {
    assert!(
        SETTER_OPCODES.contains(&opcode),
        "fault injection requires an observed setter opcode"
    );
    ARM.with(|slot| {
        let mut arm = slot.borrow_mut();
        assert!(arm.is_none(), "nested fault injection scope");
        *arm = Some(Arm {
            opcode,
            after_delivery,
            fired: false,
        });
    });

    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ARM.with(|slot| *slot.borrow_mut() = None);
        }
    }
    let reset = Reset;
    let value = work();
    let fired = ARM.with(|slot| slot.borrow().as_ref().is_some_and(|arm| arm.fired));
    drop(reset);
    (value, fired)
}

fn injected_error<T>() -> crate::hid::Result<T> {
    Err(io::Error::other("injected configuration fault").into())
}

/// Wrap one outgoing setter report. A report starts with its HID report ID.
pub(crate) fn send<T>(
    report: &[u8],
    transmit: impl FnOnce() -> crate::hid::Result<T>,
) -> crate::hid::Result<T> {
    let mode = ARM.with(|slot| {
        let mut guard = slot.borrow_mut();
        let arm = guard.as_mut()?;
        if arm.fired || report.get(1).copied() != Some(arm.opcode) {
            return None;
        }
        if !arm.after_delivery {
            arm.fired = true;
        }
        Some(arm.after_delivery)
    });

    if mode == Some(false) {
        return injected_error();
    }
    let result = transmit();
    if mode == Some(true) && result.is_ok() {
        ARM.with(|slot| {
            if let Some(arm) = slot.borrow_mut().as_mut() {
                arm.fired = true;
            }
        });
        return injected_error();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn before_delivery_is_single_shot_and_opcode_specific() {
        let calls = Cell::new(0);
        let ((), fired) = with_fault(0x16, false, || {
            assert!(send(&[0, 0x13], || Ok(())).is_ok());
            assert!(
                send(&[0, 0x16], || {
                    calls.set(calls.get() + 1);
                    Ok(())
                })
                .is_err()
            );
            assert!(
                send(&[0, 0x16], || {
                    calls.set(calls.get() + 1);
                    Ok(())
                })
                .is_ok()
            );
        });
        assert!(fired);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn after_delivery_only_fires_on_success() {
        let calls = Cell::new(0);
        let ((), fired) = with_fault(0x14, true, || {
            assert!(
                send(&[0, 0x14], || Err::<(), _>(
                    io::Error::other("transport").into()
                ))
                .is_err()
            );
            assert!(
                send(&[0, 0x14], || {
                    calls.set(calls.get() + 1);
                    Ok(())
                })
                .is_err()
            );
            assert!(
                send(&[0, 0x14], || {
                    calls.set(calls.get() + 1);
                    Ok(())
                })
                .is_ok()
            );
        });
        assert!(fired);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn panic_resets_scope() {
        let panic = std::panic::catch_unwind(|| with_fault(0x11, false, || panic!("work")));
        assert!(panic.is_err());
        assert!(!with_fault(0x11, false, || ()).1);
    }

    #[test]
    fn arm_is_local_to_the_calling_thread() {
        let ((), fired) = with_fault(0x07, false, || {
            let other = std::thread::spawn(|| send(&[0, 0x07], || Ok(())));
            assert!(other.join().unwrap().is_ok());
        });
        assert!(!fired);
    }
}
