use super::*;
use crate::{
    contract::{ApplyFailure, Recovery},
    model::keymap::{Action, ActionCategory, ActionChoice, Layer, PhysicalKey, State},
};
fn session() -> Session {
    Session::new(Descriptor {
        backend_id: "test".into(),
        device_name: "Test".into(),
        layers: vec![Layer {
            id: "base".into(),
            label: "Base".into(),
        }],
        keys: vec![PhysicalKey {
            id: "a".into(),
            label: "A".into(),
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            visible: true,
            writable: true,
        }],
        actions: vec![
            ActionChoice {
                label: "A".into(),
                action: Action::Key(4),
                category: ActionCategory::Alphanumeric,
            },
            ActionChoice {
                label: "B".into(),
                action: Action::Key(5),
                category: ActionCategory::Alphanumeric,
            },
        ],
        shortcuts: None,
    })
    .unwrap()
}
fn state(action: Action) -> State {
    State {
        revision: vec![1],
        bindings: [("base".into(), [("a".into(), action)].into())].into(),
    }
}
fn result(command: &Command, payload: FeatureResult<State>) -> Completion {
    Completion {
        generation: command.generation,
        operation: command.operation,
        payload: CompletionPayload::Keymap(payload),
    }
}
fn loaded() -> Session {
    let mut s = session();
    s.connect().unwrap();
    let c = s.read().unwrap();
    assert_eq!(
        s.accept(result(&c, FeatureResult::Read(Ok(state(Action::Key(4)))))),
        Outcome::Loaded
    );
    s
}
fn edit(s: &mut Session) {
    s.edit(Change {
        layer: "base".into(),
        key: "a".into(),
        action: Action::Key(5),
    })
    .unwrap();
}
#[test]
fn stale_and_wrong_direction_do_not_consume_ticket() {
    let mut s = session();
    s.connect().unwrap();
    let c = s.read().unwrap();
    let mut stale = result(&c, FeatureResult::Read(Ok(state(Action::Key(4)))));
    stale.generation += 1;
    assert_eq!(s.accept(stale), Outcome::Ignored);
    assert_eq!(
        s.accept(result(&c, FeatureResult::Apply(Ok(state(Action::Key(4)))))),
        Outcome::Ignored
    );
    assert!(s.busy());
    assert_eq!(
        s.accept(result(&c, FeatureResult::Read(Ok(state(Action::Key(4)))))),
        Outcome::Loaded
    );
}
#[test]
fn reconnect_keeps_edits_until_one_read_reestablishes_baseline() {
    let mut s = loaded();
    edit(&mut s);
    s.disconnect();
    assert!(s.keymap().dirty());
    s.connect().unwrap();
    assert!(s.save().is_err());
    let c = s.read().unwrap();
    assert_eq!(
        s.accept(result(&c, FeatureResult::Read(Ok(state(Action::Key(4)))))),
        Outcome::Loaded
    );
    assert!(s.keymap().dirty());
    assert!(s.save().is_ok());
}
#[test]
fn failed_save_and_disconnect_retain_edits() {
    let mut s = loaded();
    edit(&mut s);
    let c = s.save().unwrap();
    let failure = ApplyFailure {
        message: "write failed".into(),
        recovery: Recovery::Failed,
    };
    assert_eq!(
        s.accept(result(&c, FeatureResult::Apply(Err(failure.clone())))),
        Outcome::Failed(Problem::Apply(failure))
    );
    s.disconnect();
    assert!(s.keymap().dirty());
    assert_eq!(s.keymap().changes().len(), 1);
}
#[test]
fn disconnect_during_save_records_uncertainty() {
    let mut s = loaded();
    edit(&mut s);
    let c = s.save().unwrap();
    s.disconnect();
    assert!(matches!(
        s.keymap().status(),
        crate::editor::Status::Unverified {
            problem: Problem::Apply(ApplyFailure {
                recovery: Recovery::Unverified,
                ..
            })
        }
    ));
    assert!(s.keymap().dirty());
    assert_eq!(
        s.accept(result(&c, FeatureResult::Apply(Ok(state(Action::Key(5)))))),
        Outcome::Ignored
    );
}
#[test]
fn invalid_edits_do_not_change_draft() {
    let mut s = loaded();
    let before = s.keymap().draft().cloned();
    for action in [
        Action::Key(99),
        Action::Opaque {
            backend_id: "test".into(),
            data: vec![1],
            label: "raw".into(),
        },
    ] {
        assert!(
            s.edit(Change {
                layer: "base".into(),
                key: "a".into(),
                action
            })
            .is_err()
        );
        assert_eq!(s.keymap().draft(), before.as_ref());
    }
    let mut descriptor = s.descriptor().clone();
    descriptor.keys[0].writable = false;
    let mut s = Session::new(descriptor).unwrap();
    s.connect().unwrap();
    let command = s.read().unwrap();
    s.accept(result(
        &command,
        FeatureResult::Read(Ok(state(Action::Key(4)))),
    ));
    assert!(
        s.edit(Change {
            layer: "base".into(),
            key: "a".into(),
            action: Action::Key(5)
        })
        .is_err()
    );
}
#[test]
fn readback_mismatch_and_read_failure_preserve_draft() {
    let mut s = loaded();
    edit(&mut s);
    let c = s.save().unwrap();
    assert_eq!(
        s.accept(result(&c, FeatureResult::Apply(Ok(state(Action::Key(4)))))),
        Outcome::Failed(Problem::ApplyReadbackMismatch)
    );
    assert!(s.keymap().dirty());
    let c = s.read().unwrap();
    assert_eq!(
        s.accept(result(&c, FeatureResult::Read(Err("read failed".into())))),
        Outcome::Failed(Problem::Read("read failed".into()))
    );
    assert!(s.keymap().dirty());
}
#[test]
fn conflict_retains_edits_and_save_accepts_full_readback() {
    let mut s = loaded();
    edit(&mut s);
    let c = s.read().unwrap();
    let mut changed = state(Action::Key(4));
    changed.revision = vec![2];
    assert_eq!(
        s.accept(result(&c, FeatureResult::Read(Ok(changed)))),
        Outcome::Conflict
    );
    assert!(s.keymap().dirty());
    let mut s = loaded();
    edit(&mut s);
    let c = s.save().unwrap();
    assert_eq!(
        s.accept(result(&c, FeatureResult::Apply(Ok(state(Action::Key(5)))))),
        Outcome::Saved
    );
    assert!(!s.keymap().dirty());
}
#[path = "session_macros.rs"]
mod macros;
