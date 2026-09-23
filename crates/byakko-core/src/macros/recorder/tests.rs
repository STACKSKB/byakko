use super::*;
use crate::macros::{ButtonChoice, ByteBudget, Choice};

fn caps(limit: u32) -> Capabilities {
    Capabilities {
        backend_id: "test".into(),
        slots: vec![Choice {
            id: "one".into(),
            label: "One".into(),
        }],
        repeat_counts: 1..=1,
        editable_repeat_counts: 1..=1,
        delays_ms: 0..=u16::MAX as u32,
        keys: Some(4..=239),
        buttons: vec![ButtonChoice {
            button: 1,
            label: "Left".into(),
        }],
        movement: None,
        backend_actions: vec![],
        bindings: vec![],
        byte_budget: Some(ByteBudget {
            limit,
            overhead: 2,
            key: 2,
            button: 2,
            movement: 4,
            backend: 2,
            inline_delays: 1..=127,
            extended_delay: 2,
        }),
    }
}

fn empty() -> Program {
    Program {
        repeat_count: 1,
        events: vec![],
    }
}
fn key(usage: u16, pressed: bool) -> Action {
    Action::Key { usage, pressed }
}
fn button(pressed: bool) -> Action {
    Action::Button { button: 1, pressed }
}

#[test]
fn measured_waits_follow_previous_edge_and_keep_prefix_unchanged() {
    let caps = caps(248);
    let prefix = Event {
        action: key(9, false),
        delay_ms: 17,
    };
    let mut draft = Program {
        repeat_count: 1,
        events: vec![prefix.clone()],
    };
    let mut recorder =
        Recorder::new(&caps, &draft, DelayPolicy::Measured { terminal_ms: 1 }).unwrap();
    assert_eq!(
        recorder
            .transition(&caps, &mut draft, key(4, true), 100)
            .unwrap(),
        Transition::Recorded
    );
    assert_eq!(
        recorder
            .transition(&caps, &mut draft, key(4, false), 130)
            .unwrap(),
        Transition::Recorded
    );
    assert_eq!(
        recorder.stop(&caps, &mut draft, 200).unwrap(),
        StopOutcome::Complete
    );
    assert_eq!(
        draft.events,
        vec![
            prefix,
            Event {
                action: key(4, true),
                delay_ms: 30
            },
            Event {
                action: key(4, false),
                delay_ms: 1
            },
        ]
    );
}

#[test]
fn duplicate_edges_do_not_change_draft_or_clock() {
    let caps = caps(248);
    let mut draft = empty();
    let mut recorder =
        Recorder::new(&caps, &draft, DelayPolicy::Measured { terminal_ms: 1 }).unwrap();
    assert_eq!(
        recorder
            .transition(&caps, &mut draft, key(4, false), 5)
            .unwrap(),
        Transition::Duplicate
    );
    recorder
        .transition(&caps, &mut draft, key(4, true), 10)
        .unwrap();
    let before = draft.clone();
    assert_eq!(
        recorder
            .transition(&caps, &mut draft, key(4, true), 100)
            .unwrap(),
        Transition::Duplicate
    );
    assert_eq!(draft, before);
    recorder
        .transition(&caps, &mut draft, key(4, false), 20)
        .unwrap();
    assert_eq!(draft.events[0].delay_ms, 10);
}

#[test]
fn initial_idle_is_omitted_and_fixed_terminal_is_stable() {
    let caps = caps(248);
    let mut draft = empty();
    let mut recorder = Recorder::new(&caps, &draft, DelayPolicy::Fixed(7)).unwrap();
    recorder
        .transition(&caps, &mut draft, key(4, true), 1_000)
        .unwrap();
    recorder
        .transition(&caps, &mut draft, key(4, false), 3_000)
        .unwrap();
    assert_eq!(
        recorder.stop(&caps, &mut draft, 9_000).unwrap(),
        StopOutcome::Complete
    );
    assert_eq!(
        draft.events,
        vec![
            Event {
                action: key(4, true),
                delay_ms: 7
            },
            Event {
                action: key(4, false),
                delay_ms: 7
            },
        ]
    );
}

#[test]
fn stop_releases_multiple_held_inputs_in_reverse_order() {
    let caps = caps(248);
    let mut draft = empty();
    let mut recorder =
        Recorder::new(&caps, &draft, DelayPolicy::Measured { terminal_ms: 1 }).unwrap();
    recorder
        .transition(&caps, &mut draft, key(4, true), 10)
        .unwrap();
    recorder
        .transition(&caps, &mut draft, button(true), 20)
        .unwrap();
    recorder
        .transition(&caps, &mut draft, key(5, true), 30)
        .unwrap();
    assert_eq!(
        recorder.stop(&caps, &mut draft, 45).unwrap(),
        StopOutcome::Complete
    );
    assert_eq!(
        draft.events,
        vec![
            Event {
                action: key(4, true),
                delay_ms: 10
            },
            Event {
                action: button(true),
                delay_ms: 10
            },
            Event {
                action: key(5, true),
                delay_ms: 15
            },
            Event {
                action: key(5, false),
                delay_ms: 0
            },
            Event {
                action: button(false),
                delay_ms: 0
            },
            Event {
                action: key(4, false),
                delay_ms: 1
            },
        ]
    );
}

#[test]
fn capacity_rejection_is_atomic_and_stop_can_fill_exact_limit() {
    // 2 overhead + two long-delay edges at 4 bytes each = 10.
    let caps = caps(10);
    let mut draft = empty();
    let mut recorder =
        Recorder::new(&caps, &draft, DelayPolicy::Measured { terminal_ms: 0 }).unwrap();
    recorder
        .transition(&caps, &mut draft, key(4, true), 10)
        .unwrap();
    let before = draft.clone();
    let previous = recorder.clone();
    assert!(
        recorder
            .transition(&caps, &mut draft, key(5, true), 11)
            .is_err()
    );
    assert_eq!(draft, before);
    assert_eq!(recorder, previous);
    assert_eq!(
        recorder.stop(&caps, &mut draft, 20).unwrap(),
        StopOutcome::Complete
    );
    assert_eq!(draft.events.len(), 2);
    assert_eq!(draft.events[0].delay_ms, 10);
    assert_eq!(draft.events[1].delay_ms, 0);
    assert!(validate_program(&caps, &draft).is_ok());
}

#[test]
fn too_long_stop_clamps_held_interval() {
    let caps = caps(248);
    let mut draft = empty();
    let mut recorder =
        Recorder::new(&caps, &draft, DelayPolicy::Measured { terminal_ms: 1 }).unwrap();
    recorder
        .transition(&caps, &mut draft, key(4, true), 10)
        .unwrap();
    assert_eq!(
        recorder.stop(&caps, &mut draft, 70_000).unwrap(),
        StopOutcome::TimingClamped
    );
    assert_eq!(
        draft.events,
        vec![
            Event {
                action: key(4, true),
                delay_ms: 0
            },
            Event {
                action: key(4, false),
                delay_ms: 1
            },
        ]
    );
}

#[test]
fn fixed_delay_reserves_its_actual_cost_and_fits_exactly() {
    let caps = caps(6); // Header plus two inline-delay keyboard events.
    let mut draft = empty();
    let mut recorder = Recorder::new(&caps, &draft, DelayPolicy::Fixed(1)).unwrap();
    recorder
        .transition(&caps, &mut draft, key(4, true), 10)
        .unwrap();
    recorder.stop(&caps, &mut draft, 70_000).unwrap();
    assert_eq!(draft.events.len(), 2);
    assert!(draft.events.iter().all(|event| event.delay_ms == 1));
    assert!(validate_program(&caps, &draft).is_ok());
}
