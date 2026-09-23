//! Deliberately different from the Nia87: three keys, three named layers.
use byakko_core::{
    Action, ActionChoice, Descriptor, Layer, PhysicalKey, State,
    macros::{
        Action as MacroAction, Binding, ButtonChoice, Capabilities, Choice, Content, Event,
        Program, Snapshot,
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
        byte_budget: None,
        bindings: ["intro", "pointer"]
            .into_iter()
            .flat_map(|slot| {
                [
                    ("play", "Play sequence", None),
                    ("hold", "Hold sequence", Some(1)),
                ]
                .into_iter()
                .map(move |(id, label, required_repeat_count)| Binding {
                    slot: slot.into(),
                    id: id.into(),
                    label: label.into(),
                    action: Action::Named {
                        id: format!("sequence/{slot}/{id}"),
                    },
                    required_repeat_count,
                })
            })
            .collect(),
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
    let lighting = lighting_capabilities();
    let initial = byakko_core::lighting::Snapshot {
        backend_id: "memory".into(),
        revision: vec![0xCB],
        content: byakko_core::lighting::Content::Editable(byakko_core::lighting::default_setting(
            &lighting, "steady",
        )?),
    };
    let picture = byakko_core::picture::Capabilities {
        backend_id: "memory".into(),
        keys: vec!["Alpha".into(), "Fixed".into()],
    };
    let colors = std::collections::BTreeMap::from([
        ("Alpha".into(), [12, 34, 56]),
        ("Fixed".into(), [200, 10, 20]),
    ]);
    let picture_snapshot = byakko_core::picture::Snapshot {
        backend_id: "memory".into(),
        revision: vec![0xF1],
        content: byakko_core::picture::Content::Editable(colors),
    };
    let settings = byakko_core::settings::Capabilities {
        backend_id: "memory".into(),
        fields: vec![
            byakko_core::settings::Field {
                id: "studio_mode".into(),
                label: "Studio mode".into(),
                kind: byakko_core::settings::Kind::Toggle,
            },
            byakko_core::settings::Field {
                id: "repeat_delay".into(),
                label: "Repeat delay".into(),
                kind: byakko_core::settings::Kind::Number {
                    min: 2,
                    max: 20,
                    step: 2,
                    unit: "ms".into(),
                    disabled_zero: false,
                },
            },
        ],
    };
    let settings_snapshot = byakko_core::settings::Snapshot {
        backend_id: "memory".into(),
        revision: vec![0xE1],
        content: byakko_core::settings::Content::Editable(std::collections::BTreeMap::from([
            (
                "studio_mode".into(),
                byakko_core::settings::Value::Toggle(false),
            ),
            (
                "repeat_delay".into(),
                byakko_core::settings::Value::Number(8),
            ),
        ])),
    };
    let archive_caps = byakko_core::archive::ArchiveCapabilities {
        backend_id: "memory".into(),
        format_id: "memory/native-v1".into(),
        max_bytes: 4096,
    };
    let archive = byakko_core::archive::NativeArchive {
        backend_id: "memory".into(),
        format_id: "memory/native-v1".into(),
        bytes: br#"{"kind":"memory-native","version":1}"#.to_vec(),
    };
    MemoryDevice::new(descriptor, state)?
        .with_macros(capabilities, snapshots)?
        .with_lighting(lighting, initial)?
        .with_picture(picture, picture_snapshot)?
        .with_settings(settings, settings_snapshot)?
        .with_archive(archive_caps, archive)
}

fn lighting_capabilities() -> byakko_core::lighting::Capabilities {
    use byakko_core::lighting::{Capabilities, Choice, ColorCapability, Effect};
    Capabilities {
        backend_id: "memory".into(),
        host_modes: vec![],
        effects: vec![
            Effect {
                id: "off".into(),
                label: "Off".into(),
                brightness: None,
                speed: None,
                options: vec![],
                color: None,
            },
            Effect {
                id: "steady".into(),
                label: "Steady".into(),
                brightness: Some(10..=100),
                speed: None,
                options: vec![],
                color: Some(ColorCapability::Fixed),
            },
            Effect {
                id: "sweep".into(),
                label: "Sweep".into(),
                brightness: Some(20..=80),
                speed: Some(1..=10),
                color: Some(ColorCapability::FixedOrRainbow),
                options: [("out", "Outward"), ("in", "Inward")]
                    .into_iter()
                    .map(|(id, label)| Choice {
                        id: id.into(),
                        label: label.into(),
                    })
                    .collect(),
            },
        ],
    }
}
