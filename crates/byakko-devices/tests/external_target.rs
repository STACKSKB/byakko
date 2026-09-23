//! A tablet-shaped descriptor exercises the portable binding contract without
//! claiming support for Wacom reports or continuous ring values.
use byakko_core::{
    Action, ActionChoice, Change, Descriptor, Layer, PhysicalKey, State,
    session::{Acceptance, Command, Completion, Session, Status},
};
use byakko_devices::{Device, memory::MemoryDevice};
use std::{collections::BTreeMap, path::Path};

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
            },
            ActionChoice {
                label: "Scroll up".into(),
                action: Action::Named {
                    id: "scroll-up".into(),
                },
            },
        ],
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
    let mut device = MemoryDevice::new(descriptor.clone(), state.clone()).unwrap();
    let mut session = Session::new(descriptor).unwrap();
    let generation = session.connect().unwrap();
    let Command::Read { operation, .. } = session.request_read().unwrap() else {
        unreachable!()
    };
    assert_eq!(
        session.accept(Completion::Read {
            generation,
            operation,
            result: device.read(),
        }),
        Acceptance::Accepted
    );
    assert_eq!(session.status(), &Status::Ready);

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
            .stage(Change {
                layer: "default".into(),
                key: control.into(),
                action,
            })
            .unwrap();
    }
    let Command::Apply {
        operation,
        expected,
        changes,
        ..
    } = session.request_apply().unwrap()
    else {
        unreachable!()
    };
    let result = device.apply(&expected, &changes, Path::new("unused"));
    assert_eq!(
        session.accept(Completion::Apply {
            generation,
            operation,
            result
        }),
        Acceptance::Accepted
    );
    assert!(!session.dirty());
    assert_eq!(
        device.read().unwrap().bindings["default"]["ring-clockwise"],
        Action::Named {
            id: "scroll-up".into()
        }
    );
    assert_eq!(
        device.read().unwrap().bindings["default"]["ring-counterclockwise"],
        Action::Disabled
    );
}
