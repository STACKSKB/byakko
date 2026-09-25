use super::*;
use crate::session::{CommandPayload, CompletionPayload};
use crate::session::{FeatureCommand, FeatureResult};
use crate::{
    Action, Change, Descriptor, Layer, PhysicalKey, State,
    macros::{self, Content, Edit, Snapshot, recorder::DelayPolicy},
    session::{Command, Completion, Problem, Status as SessionStatus},
};
use std::collections::BTreeMap;

fn session() -> Session {
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
        shortcuts: None,
    };
    let capabilities = macros::Capabilities {
        backend_id: "memory".into(),
        slots: ["one", "two"]
            .map(|id| macros::Choice {
                id: id.into(),
                label: id.into(),
            })
            .into(),
        repeat_counts: 1..=4,
        editable_repeat_counts: 1..=4,
        delays_ms: 0..=100,
        keys: Some(1..=10),
        buttons: vec![],
        movement: None,
        backend_actions: vec![],
        bindings: vec![],
        byte_budget: None,
    };
    Session::new(descriptor)
        .unwrap()
        .with_macros(capabilities)
        .unwrap()
}

fn program(repeat_count: u32) -> Program {
    Program {
        repeat_count,
        events: vec![macros::Event {
            action: macros::Action::Key {
                usage: 4,
                pressed: true,
            },
            delay_ms: 0,
        }],
    }
}

fn snapshot(revision: u8, program: Program) -> Snapshot {
    Snapshot {
        backend_id: "memory".into(),
        slot: "one".into(),
        revision: vec![revision],
        content: Content::Editable(program),
    }
}

fn ready() -> Session {
    let mut session = session();
    session.connect().unwrap();
    let Command {
        generation,
        operation,
        payload: CommandPayload::Keymap(FeatureCommand::Read(())),
    } = session.request_read().unwrap()
    else {
        unreachable!()
    };
    session.accept(Completion {
        generation,
        operation,
        payload: CompletionPayload::Keymap(FeatureResult::Read(Ok(State {
            revision: vec![1],
            bindings: BTreeMap::from([(
                "layer".into(),
                BTreeMap::from([("key".into(), Action::Key(4))]),
            )]),
        }))),
    });
    read_macro(&mut session, snapshot(1, program(1)));
    session
}

fn read_macro(session: &mut Session, value: Snapshot) {
    let Command {
        generation,
        operation,
        payload: CommandPayload::Macro(FeatureCommand::Read(slot)),
    } = session.request_macro_read().unwrap()
    else {
        unreachable!()
    };
    session.accept(Completion {
        generation,
        operation,
        payload: CompletionPayload::Macro {
            slot,
            result: FeatureResult::Read(Ok(value)),
        },
    });
}

#[test]
fn begin_requires_eligible_target_and_import_validates_atomically() {
    let mut session = session();
    assert!(session.begin_macro_file(FileOperation::Import).is_err());
    assert!(session.begin_macro_file(FileOperation::Export).is_err());
    session.connect().unwrap();
    assert!(session.begin_macro_file(FileOperation::Import).is_err());
    let mut session = ready();
    session.edit_macro(Edit::Repeat(2)).unwrap();
    let old = session.macros().unwrap().draft().cloned();
    let baseline = session.macros().unwrap().baseline().cloned();
    let ticket = session.begin_macro_file(FileOperation::Import).unwrap();
    assert!(
        session
            .finish_macro_file(&ticket, Some(program(99)))
            .is_err()
    );
    assert_eq!(session.activity(), &Activity::Idle);
    assert_eq!(session.macros().unwrap().draft(), old.as_ref());
    assert_eq!(session.macros().unwrap().baseline(), baseline.as_ref());
    assert_eq!(session.macros().unwrap().status(), &Status::Ready);
    let ticket = session.begin_macro_file(FileOperation::Import).unwrap();
    assert_eq!(
        session.finish_macro_file(&ticket, Some(program(3))),
        Ok(Acceptance::Accepted)
    );
    assert_eq!(session.macros().unwrap().draft(), Some(&program(3)));
    assert_eq!(session.macros().unwrap().baseline(), baseline.as_ref());
}

#[test]
fn file_activity_excludes_edits_device_io_recording_and_slot_change() {
    let mut session = ready();
    let ticket = session.begin_macro_file(FileOperation::Import).unwrap();
    assert!(
        session
            .stage(Change {
                layer: "layer".into(),
                key: "key".into(),
                action: Action::Key(5)
            })
            .is_err()
    );
    assert!(session.edit_macro(Edit::Repeat(2)).is_err());
    assert!(session.revert().is_err());
    assert!(session.revert_macro().is_err());
    assert!(session.request_read().is_err());
    assert!(session.request_apply().is_err());
    assert!(session.request_macro_read().is_err());
    assert!(session.request_macro_apply().is_err());
    assert!(
        session
            .start_macro_recording(DelayPolicy::Fixed(1))
            .is_err()
    );
    assert!(session.select_macro("two").is_err());
    assert!(session.begin_macro_file(FileOperation::Export).is_err());
    assert_eq!(session.macros().unwrap().draft(), Some(&program(1)));
    assert_eq!(
        session.finish_macro_file(&ticket, None),
        Ok(Acceptance::Accepted)
    );
    assert_eq!(session.activity(), &Activity::Idle);
}

#[test]
fn stale_kind_token_and_connection_completions_are_ignored() {
    let mut session = ready();
    let ticket = session.begin_macro_file(FileOperation::Import).unwrap();
    let mut wrong_kind = ticket.clone();
    wrong_kind.kind = FileOperation::Export;
    let mut wrong_token = ticket.clone();
    wrong_token.operation += 1;
    for wrong in [&wrong_kind, &wrong_token] {
        assert_eq!(
            session.finish_macro_file(wrong, Some(program(2))),
            Ok(Acceptance::IgnoredStale)
        );
    }
    assert_eq!(
        session.activity(),
        &Activity::MacroFile {
            ticket: ticket.clone()
        }
    );
    session.disconnect();
    session.connect().unwrap();
    assert_eq!(
        session.finish_macro_file(&ticket, Some(program(2))),
        Ok(Acceptance::IgnoredStale)
    );
    assert_eq!(session.macros().unwrap().draft(), Some(&program(1)));
}

#[test]
fn export_keeps_retained_unverified_draft_and_failures_preserve_trust() {
    let mut session = ready();
    session.edit_macro(Edit::Repeat(2)).unwrap();
    let old = session.macros().unwrap().draft().cloned();
    session.disconnect();
    assert!(session.begin_macro_file(FileOperation::Import).is_err());
    let status = session.macros().unwrap().status().clone();
    assert!(matches!(
        status,
        Status::Unverified {
            problem: Problem::ReadRequired
        }
    ));
    let export = session.begin_macro_file(FileOperation::Export).unwrap();
    assert_eq!(
        session.finish_macro_file(&export, None),
        Ok(Acceptance::Accepted)
    );
    assert_eq!(session.macros().unwrap().draft(), old.as_ref());
    assert_eq!(session.macros().unwrap().status(), &status);
    session.connect().unwrap();
    read_macro(&mut session, snapshot(1, program(1)));
    let trust = session.macros().unwrap().status().clone();
    let import = session.begin_macro_file(FileOperation::Import).unwrap();
    assert_eq!(
        session.finish_macro_file(&import, None),
        Ok(Acceptance::Accepted)
    );
    assert_eq!(session.macros().unwrap().draft(), old.as_ref());
    assert_eq!(session.macros().unwrap().status(), &trust);
    assert_eq!(session.activity(), &Activity::Idle);
    assert_eq!(
        session.status(),
        &SessionStatus::Unverified {
            problem: Problem::ReadRequired
        }
    );
}
