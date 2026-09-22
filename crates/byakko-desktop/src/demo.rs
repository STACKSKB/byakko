//! Deliberately different from the Nia87: three keys, three named layers.
use byakko_core::{
    Action, ActionChoice, Descriptor, Layer, PhysicalKey, State,
    macros::{
        Action as MacroAction, ButtonChoice, Capabilities, Choice, Content, Event, Program,
        Snapshot,
    },
};
use byakko_devices::memory::MemoryDevice;

pub fn device() -> Result<MemoryDevice, String> {
    let descriptor = Descriptor {
        backend_id: "memory".into(),
        device_name: "Memory keyboard · no hardware writes".into(),
        keys: ["Alpha", "Beta", "Fixed"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| PhysicalKey {
                id: name.into(),
                label: name.into(),
                x: index as f32,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                visible: true,
                writable: index != 2,
            })
            .collect(),
        layers: ["Typing", "Navigation", "Studio"]
            .into_iter()
            .map(|name| Layer {
                id: name.into(),
                label: name.into(),
            })
            .collect(),
        actions: vec![
            ActionChoice {
                label: "A".into(),
                action: Action::Key(4),
            },
            ActionChoice {
                label: "B".into(),
                action: Action::Key(5),
            },
            ActionChoice {
                label: "Disabled".into(),
                action: Action::Disabled,
            },
        ],
    };
    let state = State {
        revision: vec![1],
        bindings: descriptor
            .layers
            .iter()
            .map(|layer| {
                (
                    layer.id.clone(),
                    descriptor
                        .keys
                        .iter()
                        .map(|key| {
                            (
                                key.id.clone(),
                                if key.writable {
                                    Action::Key(4)
                                } else {
                                    Action::Opaque {
                                        backend_id: "memory".into(),
                                        data: vec![0xFE, 0x42],
                                        label: "Factory action".into(),
                                    }
                                },
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
    };
    let capabilities = Capabilities {
        backend_id: "memory".into(),
        slots: [
            ("intro", "Intro sequence"),
            ("pointer", "Pointer sequence"),
            ("archive", "Unknown format"),
        ]
        .into_iter()
        .map(|(id, label)| Choice {
            id: id.into(),
            label: label.into(),
        })
        .collect(),
        repeat_counts: 0..=12,
        delays_ms: 0..=2_000,
        keys: Some(4..=40),
        buttons: vec![
            ButtonChoice {
                button: 1,
                label: "Primary".into(),
            },
            ButtonChoice {
                button: 2,
                label: "Secondary".into(),
            },
        ],
        movement: Some(-20..=20),
        backend_actions: vec![Choice {
            id: "dial-clockwise".into(),
            label: "Dial clockwise".into(),
        }],
    };
    let snapshot = |slot: &str, revision: u8, content| Snapshot {
        backend_id: "memory".into(),
        slot: slot.into(),
        revision: vec![revision, 0xDA],
        content,
    };
    let event = |action, delay_ms| Event { action, delay_ms };
    let snapshots = vec![
        snapshot(
            "intro",
            1,
            Content::Editable(Program {
                repeat_count: 1,
                events: vec![
                    event(
                        MacroAction::Key {
                            usage: 4,
                            pressed: true,
                        },
                        75,
                    ),
                    event(
                        MacroAction::Key {
                            usage: 4,
                            pressed: false,
                        },
                        0,
                    ),
                ],
            }),
        ),
        snapshot(
            "pointer",
            2,
            Content::Editable(Program {
                repeat_count: 2,
                events: vec![
                    event(MacroAction::Move { dx: 8, dy: -3 }, 40),
                    event(
                        MacroAction::Button {
                            button: 1,
                            pressed: true,
                        },
                        30,
                    ),
                    event(
                        MacroAction::Button {
                            button: 1,
                            pressed: false,
                        },
                        0,
                    ),
                    event(
                        MacroAction::Backend {
                            backend_id: "memory".into(),
                            id: "dial-clockwise".into(),
                            pressed: true,
                        },
                        20,
                    ),
                ],
            }),
        ),
        snapshot(
            "archive",
            3,
            Content::Opaque {
                reason: "Unrecognized demo macro data; read-only backup".into(),
            },
        ),
    ];
    MemoryDevice::new(descriptor, state)?.with_macros(capabilities, snapshots)
}
