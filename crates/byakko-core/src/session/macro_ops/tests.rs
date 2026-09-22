use crate::{
    Action, Descriptor, Layer, PhysicalKey, State,
    macros::{self, Content, Edit, Program, Snapshot},
    session::*,
};
use std::collections::BTreeMap;

fn ready() -> Session {
    let descriptor = Descriptor {
        backend_id: "memory".into(),
        device_name: "Test".into(),
        keys: vec![PhysicalKey {
            id: "key".into(),
            label: "Key".into(),
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            visible: true,
            writable: true,
        }],
        layers: vec![Layer {
            id: "layer".into(),
            label: "Layer".into(),
        }],
        actions: vec![],
    };
    let capabilities = macros::Capabilities {
        byte_budget: None,
        backend_id: "memory".into(),
        slots: vec![macros::Choice {
            id: "scene".into(),
            label: "Scene".into(),
        }],
        repeat_counts: 1..=10,
        delays_ms: 0..=100_000,
        keys: Some(4..=300),
        buttons: vec![],
        movement: None,
        backend_actions: vec![],
        bindings: vec![macros::Binding {
            slot: "scene".into(),
            id: "play".into(),
            label: "Play scene".into(),
            action: Action::Macro { slot: 3, mode: 1 },
            required_repeat_count: Some(1),
        }],
    };
    let mut session = Session::new(descriptor)
        .unwrap()
        .with_macros(capabilities)
        .unwrap();
    session.connect().unwrap();
    let Command::Read {
        generation,
        operation,
    } = session.request_read().unwrap()
    else {
        unreachable!()
    };
    session.accept(Completion::Read {
        generation,
        operation,
        result: Ok(State {
            revision: vec![1],
            bindings: BTreeMap::from([(
                "layer".into(),
                BTreeMap::from([("key".into(), Action::Key(4))]),
            )]),
        }),
    });
    read(&mut session, snapshot(1, 1));
    session
}

fn snapshot(revision: u8, count: u32) -> Snapshot {
    Snapshot {
        backend_id: "memory".into(),
        slot: "scene".into(),
        revision: vec![revision],
        content: Content::Editable(Program {
            repeat_count: count,
            events: vec![],
        }),
    }
}

fn read(session: &mut Session, value: Snapshot) {
    let Command::ReadMacro {
        generation,
        operation,
        slot,
    } = session.request_macro_read().unwrap()
    else {
        unreachable!()
    };
    assert_eq!(
        session.accept(Completion::ReadMacro {
            generation,
            operation,
            slot,
            result: Ok(value)
        }),
        Acceptance::Accepted
    );
}

#[test]
fn shared_activity_excludes_other_feature_and_rejects_wrong_slot_then_verifies() {
    let mut session = ready();
    session.edit_macro(Edit::Repeat(2)).unwrap();
    let command = session.request_macro_apply().unwrap();
    let json = serde_json::to_vec(&command).unwrap();
    assert_eq!(serde_json::from_slice::<Command>(&json).unwrap(), command);
    let Command::ApplyMacro {
        generation,
        operation,
        expected,
        desired,
    } = command
    else {
        unreachable!()
    };
    assert!(operation > 2);
    assert_eq!(expected, snapshot(1, 1));
    assert_eq!(desired.repeat_count, 2);
    assert!(session.request_read().is_err());
    assert!(session.request_macro_read().is_err());
    assert!(session.edit_macro(Edit::Clear).is_err());
    assert!(session.revert().is_err());
    assert!(session.revert_macro().is_err());
    assert!(session.select_macro("scene").is_err());
    assert!(session.connect().is_err());
    assert_eq!(
        session.accept(Completion::ApplyMacro {
            generation,
            operation,
            slot: "wrong".into(),
            result: Ok(snapshot(2, 2))
        }),
        Acceptance::IgnoredStale
    );
    assert!(session.busy());
    assert_eq!(
        session.accept(Completion::ApplyMacro {
            generation,
            operation,
            slot: "scene".into(),
            result: Ok(snapshot(2, 2))
        }),
        Acceptance::Accepted
    );
    assert!(!session.busy());
    assert!(!session.dirty());
    assert_eq!(session.macros().unwrap().baseline(), Some(&snapshot(2, 2)));
    assert_eq!(
        session.status(),
        &Status::Unverified {
            problem: Problem::ReadRequired
        }
    );
    assert!(session.request_apply().is_err());
}

#[test]
fn disconnected_macro_reply_is_stale_and_changed_reconnect_keeps_draft() {
    let mut session = ready();
    session.edit_macro(Edit::Repeat(2)).unwrap();
    let Command::ReadMacro {
        generation,
        operation,
        slot,
    } = session.request_macro_read().unwrap()
    else {
        unreachable!()
    };
    session.disconnect();
    session.connect().unwrap();
    assert_eq!(
        session.accept(Completion::ReadMacro {
            generation,
            operation,
            slot,
            result: Ok(snapshot(1, 1))
        }),
        Acceptance::IgnoredStale
    );
    assert_eq!(session.macros().unwrap().draft().unwrap().repeat_count, 2);
    read(&mut session, snapshot(2, 3));
    assert!(matches!(
        session.macros().unwrap().status(),
        macros::editor::Status::Conflict { .. }
    ));
    assert!(session.request_macro_apply().is_err());
    assert_eq!(session.macros().unwrap().baseline(), Some(&snapshot(1, 1)));
    session.revert_macro().unwrap();
    read(&mut session, snapshot(2, 3));
    assert_eq!(session.macros().unwrap().draft().unwrap().repeat_count, 3);
}

#[test]
fn apply_failure_retains_macro_and_keymap_drafts_but_neither_is_writable() {
    let mut session = ready();
    session
        .stage(crate::Change {
            layer: "layer".into(),
            key: "key".into(),
            action: Action::Key(5),
        })
        .unwrap();
    session.edit_macro(Edit::Repeat(2)).unwrap();
    let Command::ApplyMacro {
        generation,
        operation,
        ..
    } = session.request_macro_apply().unwrap()
    else {
        unreachable!()
    };
    session.accept(Completion::ApplyMacro {
        generation,
        operation,
        slot: "scene".into(),
        result: Err(ApplyFailure {
            message: "readback failed".into(),
            recovery: Recovery::Unverified,
        }),
    });
    assert_eq!(session.changes().len(), 1);
    assert!(session.macros().unwrap().dirty());
    assert!(session.request_apply().is_err());
    assert!(session.request_macro_apply().is_err());
    assert!(!session.busy());
    read(&mut session, snapshot(1, 1));
    assert_eq!(session.macros().unwrap().draft().unwrap().repeat_count, 2);
}

#[test]
fn binding_uses_advertised_action_and_preserves_drafts_on_rejection() {
    let mut session = ready();
    let action = Action::Macro { slot: 3, mode: 1 };
    assert!(
        session
            .stage_macro_binding("layer", "key", "unknown")
            .is_err()
    );
    assert!(session.changes().is_empty());
    session.stage_macro_binding("layer", "key", "play").unwrap();
    assert_eq!(session.changes()[0].action, action);
    assert_eq!(session.macros().unwrap().draft().unwrap().repeat_count, 1);
    session.edit_macro(Edit::Repeat(2)).unwrap();
    let before = session.changes();
    assert!(session.stage_macro_binding("layer", "key", "play").is_err());
    assert_eq!(session.changes(), before);
    session.revert_macro().unwrap();
    let Command::Read {
        generation,
        operation,
    } = session.request_read().unwrap()
    else {
        unreachable!()
    };
    assert!(session.stage_macro_binding("layer", "key", "play").is_err());
    session.accept(Completion::Read {
        generation,
        operation,
        result: Ok(State {
            revision: vec![1],
            bindings: BTreeMap::from([(
                "layer".into(),
                BTreeMap::from([("key".into(), Action::Key(4))]),
            )]),
        }),
    });
    // A keymap read invalidates macro trust until its own read completes.
    assert!(session.stage_macro_binding("layer", "key", "play").is_err());
    assert_eq!(session.changes(), before);
    read(&mut session, snapshot(1, 1));
    session.stage_macro_binding("layer", "key", "play").unwrap();
}
