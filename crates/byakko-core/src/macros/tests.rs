use super::*;

fn capabilities() -> Capabilities {
    Capabilities {
        backend_id: "test".into(),
        slots: vec![Choice {
            id: "scene-a".into(),
            label: "Scene A".into(),
        }],
        repeat_counts: 1..=10,
        delays_ms: 0..=100_000,
        keys: Some(4..=300),
        buttons: vec![],
        movement: None,
        backend_actions: vec![],
        bindings: vec![],
    }
}

fn program() -> Program {
    Program {
        repeat_count: 2,
        events: vec![
            Event {
                action: Action::Key {
                    usage: 260,
                    pressed: true,
                },
                delay_ms: 0,
            },
            Event {
                action: Action::Key {
                    usage: 260,
                    pressed: false,
                },
                delay_ms: 90_000,
            },
        ],
    }
}

#[test]
fn edits_preserve_wait_after_and_use_backend_ranges_not_nia_widths() {
    let caps = capabilities();
    let original = program();
    assert!(validate_program(&caps, &original).is_ok());
    let moved = edit(&caps, &original, Edit::Move { from: 0, to: 1 }).unwrap();
    assert_eq!(moved.events[1], original.events[0]);
    assert_eq!(moved.events[0], original.events[1]);
    let cleared = edit(&caps, &original, Edit::Clear).unwrap();
    assert!(cleared.events.is_empty());
    assert_eq!(cleared.repeat_count, 2);
    assert_eq!(original, program());
}

#[test]
fn malformed_or_unsupported_edits_do_not_replace_original() {
    let caps = capabilities();
    let original = program();
    for change in [
        Edit::Remove { at: 2 },
        Edit::Move { from: 0, to: 2 },
        Edit::Repeat(0),
        Edit::Insert {
            at: 0,
            event: Event {
                action: Action::Move { dx: 1, dy: 1 },
                delay_ms: 1,
            },
        },
        Edit::Replace {
            at: 1,
            event: Event {
                action: Action::Backend {
                    backend_id: "nia87".into(),
                    id: "wheel-left".into(),
                    pressed: true,
                },
                delay_ms: 0,
            },
        },
    ] {
        assert!(edit(&caps, &original, change).is_err());
    }
    assert_eq!(original, program());
}

#[test]
fn opaque_snapshot_round_trips_without_a_decodable_program() {
    let snapshot = Snapshot {
        backend_id: "test".into(),
        slot: "scene-a".into(),
        revision: vec![0, 255, 7],
        content: Content::Opaque {
            reason: "Unknown firmware event".into(),
        },
    };
    let bytes = serde_json::to_vec(&snapshot).unwrap();
    assert_eq!(
        serde_json::from_slice::<Snapshot>(&bytes).unwrap(),
        snapshot
    );
    let mut caps = capabilities();
    caps.slots.push(caps.slots[0].clone());
    assert!(validate_capabilities(&caps).is_err());
}

#[test]
fn binding_catalog_checks_identity_slot_and_repeat_policy() {
    let binding = Binding {
        slot: "scene-a".into(),
        id: "play".into(),
        label: "Play".into(),
        action: crate::Action::Macro { slot: 7, mode: 2 },
        required_repeat_count: Some(1),
    };
    let mut caps = capabilities();
    caps.bindings.push(binding.clone());
    assert!(validate_capabilities(&caps).is_ok());
    let json = serde_json::to_value(&caps).unwrap();
    assert_eq!(serde_json::from_value::<Capabilities>(json).unwrap(), caps);
    caps.bindings.push(binding.clone());
    assert!(validate_capabilities(&caps).is_err());
    caps.bindings.pop();
    for invalid in [
        Binding {
            slot: "absent".into(),
            ..binding.clone()
        },
        Binding {
            id: "".into(),
            ..binding.clone()
        },
        Binding {
            label: "".into(),
            ..binding.clone()
        },
        Binding {
            required_repeat_count: Some(0),
            ..binding
        },
    ] {
        caps.bindings[0] = invalid;
        assert!(validate_capabilities(&caps).is_err());
    }
    let mut old_json = serde_json::to_value(capabilities()).unwrap();
    old_json.as_object_mut().unwrap().remove("bindings");
    assert!(
        serde_json::from_value::<Capabilities>(old_json)
            .unwrap()
            .bindings
            .is_empty()
    );
}
