//! Translate window-local Iced input into portable macro actions.
use byakko_core::macros::Action;
use iced::{Event, keyboard, mouse};
use keyboard::key::{Code, Physical};

pub(super) fn action(event: &Event) -> Option<Action> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            physical_key: Physical::Code(code),
            repeat: false,
            ..
        }) => Some(Action::Key {
            usage: key_usage(*code)?,
            pressed: true,
        }),
        Event::Keyboard(keyboard::Event::KeyReleased {
            physical_key: Physical::Code(code),
            ..
        }) => Some(Action::Key {
            usage: key_usage(*code)?,
            pressed: false,
        }),
        Event::Mouse(mouse::Event::ButtonPressed(button)) => Some(Action::Button {
            button: button_usage(*button)?,
            pressed: true,
        }),
        Event::Mouse(mouse::Event::ButtonReleased(button)) => Some(Action::Button {
            button: button_usage(*button)?,
            pressed: false,
        }),
        _ => None,
    }
}

fn button_usage(button: mouse::Button) -> Option<u16> {
    Some(match button {
        mouse::Button::Left => 1,
        mouse::Button::Right => 2,
        mouse::Button::Middle => 3,
        mouse::Button::Back => 4,
        mouse::Button::Forward => 5,
        mouse::Button::Other(_) => return None,
    })
}

fn key_usage(code: Code) -> Option<u16> {
    KEY_USAGES
        .iter()
        .find_map(|(physical, usage)| (*physical == code).then_some(*usage))
}

const KEY_USAGES: &[(Code, u16)] = &{
    use Code::*;
    [
        (KeyA, 0x04),
        (KeyB, 0x05),
        (KeyC, 0x06),
        (KeyD, 0x07),
        (KeyE, 0x08),
        (KeyF, 0x09),
        (KeyG, 0x0a),
        (KeyH, 0x0b),
        (KeyI, 0x0c),
        (KeyJ, 0x0d),
        (KeyK, 0x0e),
        (KeyL, 0x0f),
        (KeyM, 0x10),
        (KeyN, 0x11),
        (KeyO, 0x12),
        (KeyP, 0x13),
        (KeyQ, 0x14),
        (KeyR, 0x15),
        (KeyS, 0x16),
        (KeyT, 0x17),
        (KeyU, 0x18),
        (KeyV, 0x19),
        (KeyW, 0x1a),
        (KeyX, 0x1b),
        (KeyY, 0x1c),
        (KeyZ, 0x1d),
        (Digit1, 0x1e),
        (Digit2, 0x1f),
        (Digit3, 0x20),
        (Digit4, 0x21),
        (Digit5, 0x22),
        (Digit6, 0x23),
        (Digit7, 0x24),
        (Digit8, 0x25),
        (Digit9, 0x26),
        (Digit0, 0x27),
        (Enter, 0x28),
        (Escape, 0x29),
        (Backspace, 0x2a),
        (Tab, 0x2b),
        (Space, 0x2c),
        (Minus, 0x2d),
        (Equal, 0x2e),
        (BracketLeft, 0x2f),
        (BracketRight, 0x30),
        (Backslash, 0x31),
        (Semicolon, 0x33),
        (Quote, 0x34),
        (Backquote, 0x35),
        (Comma, 0x36),
        (Period, 0x37),
        (Slash, 0x38),
        (CapsLock, 0x39),
        (F1, 0x3a),
        (F2, 0x3b),
        (F3, 0x3c),
        (F4, 0x3d),
        (F5, 0x3e),
        (F6, 0x3f),
        (F7, 0x40),
        (F8, 0x41),
        (F9, 0x42),
        (F10, 0x43),
        (F11, 0x44),
        (F12, 0x45),
        (PrintScreen, 0x46),
        (ScrollLock, 0x47),
        (Pause, 0x48),
        (Insert, 0x49),
        (Home, 0x4a),
        (PageUp, 0x4b),
        (Delete, 0x4c),
        (End, 0x4d),
        (PageDown, 0x4e),
        (ArrowRight, 0x4f),
        (ArrowLeft, 0x50),
        (ArrowDown, 0x51),
        (ArrowUp, 0x52),
        (NumLock, 0x53),
        (NumpadDivide, 0x54),
        (NumpadMultiply, 0x55),
        (NumpadSubtract, 0x56),
        (NumpadAdd, 0x57),
        (NumpadEnter, 0x58),
        (Numpad1, 0x59),
        (Numpad2, 0x5a),
        (Numpad3, 0x5b),
        (Numpad4, 0x5c),
        (Numpad5, 0x5d),
        (Numpad6, 0x5e),
        (Numpad7, 0x5f),
        (Numpad8, 0x60),
        (Numpad9, 0x61),
        (Numpad0, 0x62),
        (NumpadDecimal, 0x63),
        (IntlBackslash, 0x64),
        (ContextMenu, 0x65),
        (NumpadEqual, 0x67),
        (F13, 0x68),
        (F14, 0x69),
        (F15, 0x6a),
        (F16, 0x6b),
        (F17, 0x6c),
        (F18, 0x6d),
        (F19, 0x6e),
        (F20, 0x6f),
        (F21, 0x70),
        (F22, 0x71),
        (F23, 0x72),
        (F24, 0x73),
        (IntlRo, 0x87),
        (IntlYen, 0x89),
        (ControlLeft, 0xe0),
        (ShiftLeft, 0xe1),
        (AltLeft, 0xe2),
        (SuperLeft, 0xe3),
        (ControlRight, 0xe4),
        (ShiftRight, 0xe5),
        (AltRight, 0xe6),
        (SuperRight, 0xe7),
    ]
};

#[cfg(test)]
mod tests {
    use super::*;
    use keyboard::key::NativeCode;
    use keyboard::{Key, Location, Modifiers};
    use std::collections::HashSet;

    #[test]
    fn key_table_has_unique_physical_codes() {
        let mut seen = HashSet::new();
        for (code, usage) in KEY_USAGES {
            assert!(seen.insert(code), "duplicate physical code: {code:?}");
            assert_eq!(key_usage(*code), Some(*usage));
        }
    }

    fn key(code: Physical, repeat: bool) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Unidentified,
            modified_key: Key::Unidentified,
            physical_key: code,
            location: Location::Standard,
            modifiers: Modifiers::empty(),
            text: None,
            repeat,
        })
    }

    #[test]
    fn physical_keypad_and_modifier_sides_stay_distinct() {
        assert_eq!(
            action(&key(Physical::Code(Code::Digit1), false)),
            Some(Action::Key {
                usage: 0x1e,
                pressed: true
            })
        );
        assert_eq!(
            action(&key(Physical::Code(Code::Numpad1), false)),
            Some(Action::Key {
                usage: 0x59,
                pressed: true
            })
        );
        assert_eq!(
            action(&key(Physical::Code(Code::ShiftLeft), false)),
            Some(Action::Key {
                usage: 0xe1,
                pressed: true
            })
        );
        assert_eq!(
            action(&key(Physical::Code(Code::ShiftRight), false)),
            Some(Action::Key {
                usage: 0xe5,
                pressed: true
            })
        );
    }

    #[test]
    fn release_and_mouse_buttons_keep_edges() {
        let released = Event::Keyboard(keyboard::Event::KeyReleased {
            key: Key::Unidentified,
            modified_key: Key::Unidentified,
            physical_key: Physical::Code(Code::NumpadEnter),
            location: Location::Numpad,
            modifiers: Modifiers::empty(),
        });
        assert_eq!(
            action(&released),
            Some(Action::Key {
                usage: 0x58,
                pressed: false
            })
        );
        for (button, usage) in [
            (mouse::Button::Left, 1),
            (mouse::Button::Right, 2),
            (mouse::Button::Middle, 3),
            (mouse::Button::Back, 4),
            (mouse::Button::Forward, 5),
        ] {
            assert_eq!(
                action(&Event::Mouse(mouse::Event::ButtonPressed(button))),
                Some(Action::Button {
                    button: usage,
                    pressed: true
                })
            );
            assert_eq!(
                action(&Event::Mouse(mouse::Event::ButtonReleased(button))),
                Some(Action::Button {
                    button: usage,
                    pressed: false
                })
            );
        }
    }

    #[test]
    fn ignores_repeat_and_unidentified_inputs() {
        assert_eq!(action(&key(Physical::Code(Code::KeyA), true)), None);
        assert_eq!(
            action(&key(
                Physical::Unidentified(NativeCode::Unidentified),
                false
            )),
            None
        );
        assert_eq!(action(&key(Physical::Code(Code::F25), false)), None);
        assert_eq!(
            action(&Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Other(6)
            ))),
            None
        );
    }
}
