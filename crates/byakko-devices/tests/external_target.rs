//! A tablet-shaped descriptor exercises the portable binding contract without
//! claiming support for Wacom reports or continuous ring values.
use byakko_core::{
    model::keymap::{Action, ActionChoice, Change, Descriptor, Layer, PhysicalKey, State},
    session::{Outcome, Session},
};
use byakko_devices::{Executor, memory::MemoryDevice};
use std::{collections::BTreeMap, time::Duration};

#[test]
fn discrete_tablet_controls_use_the_same_draft_and_device_contract() {
    let controls = ["express-1", "ring-clockwise", "ring-counterclockwise"];
    let descriptor = Descriptor {
        backend_id: "synthetic-tablet".into(),
        device_name: "Tablet boundary probe".into(),
        keys: controls
            .iter()
            .enumerate()
            .map(|(index, id)| PhysicalKey {
                id: (*id).into(),
                label: (*id).into(),
                x: index as f32,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                visible: true,
                writable: true,
            })
            .collect(),
        layers: vec![Layer {
            id: "default".into(),
            label: "Default".into(),
        }],
        actions: vec![
            ActionChoice {
                label: "A".into(),
                action: Action::Key(4),
                category: Default::default(),
            },
            ActionChoice {
                label: "Scroll up".into(),
                action: Action::Named {
                    id: "scroll-up".into(),
                },
                category: Default::default(),
            },
        ],
        shortcuts: None,
    };
    let state = State {
        revision: vec![1],
        bindings: BTreeMap::from([(
            "default".into(),
            controls
                .iter()
                .map(|id| ((*id).into(), Action::Disabled))
                .collect(),
        )]),
    };
    let device = MemoryDevice::new(descriptor.clone(), state.clone()).unwrap();
    let mut session = Session::new(descriptor).unwrap();
    let generation = session.connect().unwrap();
    let executor = Executor::spawn(device, Default::default()).unwrap();
    executor.set_generation(generation);
    executor.try_submit(session.read().unwrap()).unwrap();
    assert_eq!(
        session.accept(executor.receive(Some(Duration::from_secs(2))).unwrap()),
        Outcome::Loaded
    );
    assert!(session.keymap().baseline().is_some());
    for (control, action) in [
        ("express-1", Action::Key(4)),
        (
            "ring-clockwise",
            Action::Named {
                id: "scroll-up".into(),
            },
        ),
    ] {
        session
            .edit(Change {
                layer: "default".into(),
                key: control.into(),
                action,
            })
            .unwrap();
    }
    executor.try_submit(session.save().unwrap()).unwrap();
    assert_eq!(
        session.accept(executor.receive(Some(Duration::from_secs(2))).unwrap()),
        Outcome::Saved
    );
    assert!(!session.keymap().dirty());
    assert_eq!(
        session.keymap().baseline().unwrap().bindings["default"]["ring-clockwise"],
        Action::Named {
            id: "scroll-up".into()
        }
    );
    assert_eq!(
        session.keymap().baseline().unwrap().bindings["default"]["ring-counterclockwise"],
        Action::Disabled
    );
}
