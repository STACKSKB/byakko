//! Translation between portable macro values and the Nia87 simple macro store.
use crate::nia87::{actions, device, macros as native};
use byakko_core::{
    macros::{
        self, Action, Binding, ButtonChoice, ByteBudget, Capabilities, Choice, Content, Event,
        Program, Snapshot,
    },
    session::{ApplyFailure, Recovery},
};
use std::path::Path;

pub const BACKEND_ID: &str = "nia87";

const WHEELS: [(&str, &str, u8); 4] = [
    ("wheel-left", "Wheel left", 245),
    ("wheel-right", "Wheel right", 246),
    ("wheel-forward", "Wheel forward", 247),
    ("wheel-back", "Wheel back", 248),
];

pub fn slot_id(slot: u8) -> String {
    format!("slot-{slot:02}")
}

fn slot_number(slot: &str) -> Result<u8, String> {
    let number = slot
        .strip_prefix("slot-")
        .filter(|digits| digits.len() == 2 && digits.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|digits| digits.parse::<u8>().ok())
        .filter(|number| *number < 50)
        .ok_or_else(|| format!("Invalid Nia87 macro slot: {slot}"))?;
    Ok(number)
}

pub fn capabilities() -> Capabilities {
    Capabilities {
        byte_budget: Some(ByteBudget {
            limit: 248,
            overhead: 2,
            key: 2,
            button: 2,
            movement: 4,
            backend: 2,
            inline_delays: 1..=127,
            extended_delay: 2,
        }),
        bindings: (0..50)
            .flat_map(|slot| {
                [
                    (
                        "counted",
                        "Play stored count",
                        actions::MACRO_MODE_REPEAT_TIMES,
                        None,
                    ),
                    (
                        "toggle",
                        "Toggle playback",
                        actions::MACRO_MODE_ON_OFF,
                        Some(1),
                    ),
                    (
                        "hold",
                        "Repeat while held",
                        actions::MACRO_MODE_TOUCH_REPEAT,
                        Some(1),
                    ),
                ]
                .into_iter()
                .map(move |(id, label, mode, required_repeat_count)| Binding {
                    slot: slot_id(slot),
                    id: id.into(),
                    label: label.into(),
                    action: byakko_core::Action::Macro {
                        slot: slot.into(),
                        mode,
                    },
                    required_repeat_count,
                })
            })
            .collect(),
        backend_id: BACKEND_ID.into(),
        slots: (0..50)
            .map(|slot| Choice {
                id: slot_id(slot),
                label: format!("Macro {}", slot + 1),
            })
            .collect(),
        repeat_counts: 0..=u16::MAX as u32,
        delays_ms: 0..=u16::MAX as u32,
        keys: Some(4..=239),
        buttons: ["Left", "Right", "Middle", "Back", "Forward"]
            .into_iter()
            .enumerate()
            .map(|(index, label)| ButtonChoice {
                button: index as u16 + 1,
                label: label.into(),
            })
            .collect(),
        movement: Some(-128..=127),
        backend_actions: WHEELS
            .into_iter()
            .map(|(id, label, _)| Choice {
                id: id.into(),
                label: label.into(),
            })
            .collect(),
    }
}

fn from_native(value: &native::Macro) -> Program {
    Program {
        repeat_count: u32::from(value.repeat_count),
        events: value
            .events
            .iter()
            .map(|event| match *event {
                native::MacroEvent::Key {
                    usage,
                    down,
                    delay_ms,
                } => Event {
                    action: Action::Key {
                        usage: u16::from(usage),
                        pressed: down,
                    },
                    delay_ms: u32::from(delay_ms),
                },
                native::MacroEvent::MouseButton {
                    button,
                    down,
                    delay_ms,
                } if button <= 244 => Event {
                    action: Action::Button {
                        button: u16::from(button - 239),
                        pressed: down,
                    },
                    delay_ms: u32::from(delay_ms),
                },
                native::MacroEvent::MouseButton {
                    button,
                    down,
                    delay_ms,
                } => Event {
                    action: Action::Backend {
                        backend_id: BACKEND_ID.into(),
                        id: WHEELS
                            .iter()
                            .find(|(_, _, byte)| *byte == button)
                            .expect("native decoder restricts mouse actions to 240..=248")
                            .0
                            .into(),
                        pressed: down,
                    },
                    delay_ms: u32::from(delay_ms),
                },
                native::MacroEvent::Move { dx, dy, delay_ms } => Event {
                    action: Action::Move {
                        dx: i32::from(dx),
                        dy: i32::from(dy),
                    },
                    delay_ms: u32::from(delay_ms),
                },
            })
            .collect(),
    }
}

fn to_native(value: &Program) -> Result<native::Macro, String> {
    macros::validate_program(&capabilities(), value)?;
    Ok(native::Macro {
        repeat_count: u16::try_from(value.repeat_count).map_err(|_| "Nia87 count exceeds u16")?,
        events: value
            .events
            .iter()
            .map(|event| {
                let delay_ms =
                    u16::try_from(event.delay_ms).map_err(|_| "Nia87 delay exceeds u16")?;
                Ok(match &event.action {
                    Action::Key { usage, pressed } => native::MacroEvent::Key {
                        usage: u8::try_from(*usage).map_err(|_| "Nia87 key exceeds u8")?,
                        down: *pressed,
                        delay_ms,
                    },
                    Action::Button { button, pressed } => native::MacroEvent::MouseButton {
                        button: u8::try_from(*button).map_err(|_| "Nia87 button exceeds u8")? + 239,
                        down: *pressed,
                        delay_ms,
                    },
                    Action::Move { dx, dy } => native::MacroEvent::Move {
                        dx: i8::try_from(*dx).map_err(|_| "Nia87 dx exceeds i8")?,
                        dy: i8::try_from(*dy).map_err(|_| "Nia87 dy exceeds i8")?,
                        delay_ms,
                    },
                    Action::Backend { id, pressed, .. } => native::MacroEvent::MouseButton {
                        button: WHEELS
                            .iter()
                            .find(|(choice, _, _)| choice == id)
                            .ok_or("Unsupported Nia87 wheel action")?
                            .2,
                        down: *pressed,
                        delay_ms,
                    },
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    })
}

pub fn from_bytes(slot: &str, raw: &[u8]) -> Result<Snapshot, String> {
    slot_number(slot)?;
    if raw.len() != 256 {
        return Err(format!("Expected 256 Nia87 macro bytes, got {}", raw.len()));
    }
    let content = match native::decode(raw) {
        Ok(value) => Content::Editable(from_native(&value)),
        Err(error) => Content::Opaque { reason: error },
    };
    Ok(Snapshot {
        backend_id: BACKEND_ID.into(),
        slot: slot.into(),
        revision: raw.into(),
        content,
    })
}

pub fn draft(expected: &Snapshot, desired: &Program) -> Result<native::Macro, String> {
    if expected.backend_id != BACKEND_ID {
        return Err("Macro snapshot belongs to another backend".into());
    }
    slot_number(&expected.slot)?;
    let projected = from_bytes(&expected.slot, &expected.revision)?;
    if projected != *expected {
        return Err("Macro snapshot differs from its revision; reload before editing".into());
    }
    if !matches!(expected.content, Content::Editable(_)) {
        return Err("Unrecognized Nia87 macro is available only as a raw backup".into());
    }
    let value = to_native(desired)?;
    native::encode(&value)?;
    Ok(value)
}

pub fn read(slot: &str) -> Result<Snapshot, String> {
    read_with(&device::Access::unique(), slot)
}

pub(super) fn read_with(access: &device::Access, slot: &str) -> Result<Snapshot, String> {
    let number = slot_number(slot)?;
    let raw = access
        .read_macro(number)
        .map_err(|error| error.to_string())?;
    from_bytes(slot, &raw)
}

pub fn apply(
    expected: &Snapshot,
    desired: &Program,
    backup: &Path,
) -> Result<Snapshot, ApplyFailure> {
    apply_with(&device::Access::unique(), expected, desired, backup)
}

pub(super) fn apply_with(
    access: &device::Access,
    expected: &Snapshot,
    desired: &Program,
    backup: &Path,
) -> Result<Snapshot, ApplyFailure> {
    let value = draft(expected, desired).map_err(|message| ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    })?;
    let number = slot_number(&expected.slot).map_err(|message| ApplyFailure {
        message,
        recovery: Recovery::NotAttempted,
    })?;
    let raw = access.apply_macro_detailed(number, &expected.revision, &value, backup)?;
    from_bytes(&expected.slot, &raw).map_err(|message| ApplyFailure {
        message,
        recovery: Recovery::Unverified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_budget_matches_native_encoding_boundaries() {
        let caps = capabilities();
        let actions = [
            Action::Key {
                usage: 4,
                pressed: true,
            },
            Action::Button {
                button: 1,
                pressed: false,
            },
            Action::Move { dx: -128, dy: 127 },
            Action::Backend {
                backend_id: BACKEND_ID.into(),
                id: "wheel-left".into(),
                pressed: true,
            },
        ];
        for action in actions {
            for delay_ms in [0, 1, 127, 128, 65535] {
                for count in [1, 40, 41, 60, 61, 62, 120, 122, 123, 124] {
                    let program = Program {
                        repeat_count: 1,
                        events: vec![
                            Event {
                                action: action.clone(),
                                delay_ms
                            };
                            count
                        ],
                    };
                    let native_value = native::Macro {
                        repeat_count: 1,
                        events: program
                            .events
                            .iter()
                            .map(|event| match &event.action {
                                Action::Key { usage, pressed } => native::MacroEvent::Key {
                                    usage: *usage as u8,
                                    down: *pressed,
                                    delay_ms: event.delay_ms as u16,
                                },
                                Action::Button { button, pressed } => {
                                    native::MacroEvent::MouseButton {
                                        button: *button as u8 + 239,
                                        down: *pressed,
                                        delay_ms: event.delay_ms as u16,
                                    }
                                }
                                Action::Move { dx, dy } => native::MacroEvent::Move {
                                    dx: *dx as i8,
                                    dy: *dy as i8,
                                    delay_ms: event.delay_ms as u16,
                                },
                                Action::Backend { pressed, .. } => {
                                    native::MacroEvent::MouseButton {
                                        button: 245,
                                        down: *pressed,
                                        delay_ms: event.delay_ms as u16,
                                    }
                                }
                            })
                            .collect(),
                    };
                    assert_eq!(
                        macros::validate_program(&caps, &program).is_ok(),
                        native::encode(&native_value).is_ok(),
                        "{action:?} delay={delay_ms} count={count}"
                    );
                }
            }
        }
    }

    #[test]
    fn binding_capabilities_retain_wire_modes_and_explicit_count_policy() {
        let caps = capabilities();
        macros::validate_capabilities(&caps).unwrap();
        assert_eq!(caps.bindings.len(), 150);
        for (slot, id, raw, required) in [
            ("slot-00", "counted", [9, 0, 0, 0], None),
            ("slot-00", "toggle", [9, 1, 0, 0], Some(1)),
            ("slot-49", "hold", [9, 2, 49, 0], Some(1)),
        ] {
            let binding = caps
                .bindings
                .iter()
                .find(|b| b.slot == slot && b.id == id)
                .unwrap();
            assert_eq!(
                crate::nia87::adapter::raw_from_action(&binding.action).unwrap(),
                raw
            );
            assert_eq!(binding.required_repeat_count, required);
        }
    }

    fn empty() -> Snapshot {
        from_bytes("slot-49", &[0; 256]).unwrap()
    }

    #[test]
    fn accepted_long_encoding_retains_exact_before_image() {
        let mut raw = vec![0; 256];
        // A one millisecond wait stored using the valid extended delay form.
        raw[..6].copy_from_slice(&[1, 0, 4, 0x80, 1, 0]);
        let snapshot = from_bytes("slot-00", &raw).unwrap();
        let Content::Editable(program) = &snapshot.content else {
            panic!("valid native program")
        };
        assert_eq!(program.events[0].delay_ms, 1);
        assert_eq!(snapshot.revision, raw);
        let native = draft(&snapshot, program).unwrap();
        assert_eq!(native::decode(&raw).unwrap(), native);
        assert_ne!(native::encode(&native).unwrap(), raw);
        assert_eq!(snapshot.revision, raw);
    }

    #[test]
    fn mixed_events_preserve_order_zero_wait_and_wheel_edges() {
        let program = Program {
            repeat_count: 0,
            events: vec![
                Event {
                    action: Action::Key {
                        usage: 4,
                        pressed: true,
                    },
                    delay_ms: 0,
                },
                Event {
                    action: Action::Button {
                        button: 5,
                        pressed: false,
                    },
                    delay_ms: 128,
                },
                Event {
                    action: Action::Move { dx: -128, dy: 127 },
                    delay_ms: 65535,
                },
                Event {
                    action: Action::Backend {
                        backend_id: BACKEND_ID.into(),
                        id: "wheel-left".into(),
                        pressed: true,
                    },
                    delay_ms: 1,
                },
                Event {
                    action: Action::Backend {
                        backend_id: BACKEND_ID.into(),
                        id: "wheel-back".into(),
                        pressed: false,
                    },
                    delay_ms: 0,
                },
            ],
        };
        let raw = native::encode(&draft(&empty(), &program).unwrap()).unwrap();
        assert_eq!(
            &raw[..22],
            &[
                0, 0, 4, 128, 0, 0, 244, 0, 128, 0, 249, 0, 128, 127, 255, 255, 245, 129, 248, 0,
                0, 0,
            ]
        );
        assert_eq!(
            from_bytes("slot-49", &raw).unwrap().content,
            Content::Editable(program)
        );
    }

    #[test]
    fn rejects_width_capacity_and_foreign_actions() {
        let base = empty();
        for action in [
            Action::Key {
                usage: 240,
                pressed: true,
            },
            Action::Button {
                button: 6,
                pressed: true,
            },
            Action::Move { dx: 128, dy: 0 },
            Action::Backend {
                backend_id: "other".into(),
                id: "wheel-left".into(),
                pressed: true,
            },
        ] {
            assert!(
                draft(
                    &base,
                    &Program {
                        repeat_count: 1,
                        events: vec![Event {
                            action,
                            delay_ms: 1
                        }]
                    }
                )
                .is_err()
            );
        }
        assert!(
            draft(
                &base,
                &Program {
                    repeat_count: 65536,
                    events: vec![]
                }
            )
            .is_err()
        );
        assert!(
            draft(
                &base,
                &Program {
                    repeat_count: 1,
                    events: vec![Event {
                        action: Action::Key {
                            usage: 4,
                            pressed: true
                        },
                        delay_ms: 65536
                    }]
                }
            )
            .is_err()
        );
        assert!(
            draft(
                &base,
                &Program {
                    repeat_count: 1,
                    events: (0..124)
                        .map(|_| Event {
                            action: Action::Key {
                                usage: 4,
                                pressed: true
                            },
                            delay_ms: 1
                        })
                        .collect()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn opaque_and_forged_snapshots_cannot_be_drafted() {
        let mut unknown = vec![0; 256];
        unknown[2] = 250;
        let opaque = from_bytes("slot-00", &unknown).unwrap();
        assert!(matches!(opaque.content, Content::Opaque { .. }));
        assert_eq!(opaque.revision, unknown);
        assert!(
            draft(
                &opaque,
                &Program {
                    repeat_count: 1,
                    events: vec![]
                }
            )
            .is_err()
        );
        let mut forged = empty();
        forged.content = Content::Editable(Program {
            repeat_count: 9,
            events: vec![],
        });
        assert!(
            draft(
                &forged,
                &Program {
                    repeat_count: 1,
                    events: vec![]
                }
            )
            .is_err()
        );
        forged = empty();
        forged.backend_id = "other".into();
        assert!(
            draft(
                &forged,
                &Program {
                    repeat_count: 1,
                    events: vec![]
                }
            )
            .is_err()
        );
        assert!(from_bytes("slot-50", &[0; 256]).is_err());
        assert!(from_bytes("slot-1", &[0; 256]).is_err());
    }
}
