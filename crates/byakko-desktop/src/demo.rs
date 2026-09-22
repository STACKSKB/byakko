//! Deliberately different from the Nia87: three keys, three named layers.
use byakko_core::{Action, ActionChoice, Descriptor, Layer, PhysicalKey, State};
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
    MemoryDevice::new(descriptor, state)
}
