use super::*;
use crate::macros::{Action, Choice, Event};

fn caps() -> Capabilities {
    Capabilities {
        backend_id: "memory".into(),
        slots: ["one", "two"]
            .map(|id| Choice {
                id: id.into(),
                label: id.into(),
            })
            .into(),
        repeat_counts: 1..=4,
        delays_ms: 0..=100,
        keys: Some(1..=10),
        buttons: vec![],
        movement: None,
        backend_actions: vec![],
    }
}
fn program(usage: u16) -> Program {
    Program {
        repeat_count: 1,
        events: vec![Event {
            action: Action::Key {
                usage,
                pressed: true,
            },
            delay_ms: 0,
        }],
    }
}
fn snapshot(revision: u8, content: Content) -> Snapshot {
    Snapshot {
        backend_id: "memory".into(),
        slot: "one".into(),
        revision: vec![revision],
        content,
    }
}

#[test]
fn dirty_read_conflicts_but_matching_read_retains_draft() {
    let mut editor = Editor::new(caps()).unwrap();
    let original = snapshot(1, Content::Editable(program(1)));
    editor.accept_read(Ok(original.clone()));
    editor
        .edit(Edit::Replace {
            at: 0,
            event: program(2).events[0].clone(),
        })
        .unwrap();
    editor.accept_read(Ok(original.clone()));
    assert_eq!(editor.status(), &Status::Ready);
    assert_eq!(
        editor.request_apply().unwrap(),
        (original.clone(), program(2))
    );
    let changed = snapshot(2, Content::Editable(program(3)));
    editor.accept_read(Ok(changed.clone()));
    assert_eq!(editor.status(), &Status::Conflict { device: changed });
    assert_eq!(editor.baseline(), Some(&original));
    assert_eq!(editor.draft(), Some(&program(2)));
    assert!(editor.request_apply().is_err());
    editor.revert().unwrap();
    assert!(!editor.dirty());
    assert!(matches!(editor.status(), Status::Conflict { .. }));
}

#[test]
fn malformed_outcomes_and_failed_apply_preserve_draft() {
    let mut editor = Editor::new(caps()).unwrap();
    let original = snapshot(1, Content::Editable(program(1)));
    editor.accept_read(Ok(original.clone()));
    editor.edit(Edit::Repeat(2)).unwrap();
    let draft = editor.draft().cloned();
    let mut other_slot = original.clone();
    other_slot.slot = "two".into();
    editor.accept_read(Ok(other_slot.clone()));
    assert!(matches!(
        editor.status(),
        Status::Unverified {
            problem: Problem::Read(_)
        }
    ));
    assert_eq!(editor.baseline(), Some(&original));
    editor.accept_apply(Ok(other_slot));
    assert!(matches!(
        editor.status(),
        Status::Unverified {
            problem: Problem::InvalidApplyResult(_)
        }
    ));
    editor.accept_apply(Ok(snapshot(2, Content::Editable(program(1)))));
    assert_eq!(
        editor.status(),
        &Status::Unverified {
            problem: Problem::ApplyReadbackMismatch
        }
    );
    let failure = ApplyFailure {
        message: "write failed".into(),
        recovery: crate::session::Recovery::Failed,
    };
    editor.accept_apply(Err(failure.clone()));
    assert_eq!(
        editor.status(),
        &Status::Unverified {
            problem: Problem::Apply(failure)
        }
    );
    assert_eq!(editor.draft(), draft.as_ref());
    assert_eq!(editor.baseline(), Some(&original));
}

#[test]
fn opaque_is_readable_but_not_editable_and_slot_change_clears_clean_state() {
    let mut editor = Editor::new(caps()).unwrap();
    let opaque = snapshot(
        1,
        Content::Opaque {
            reason: "unknown encoding".into(),
        },
    );
    editor.accept_read(Ok(opaque.clone()));
    assert_eq!(editor.baseline(), Some(&opaque));
    assert!(editor.draft().is_none());
    assert!(editor.edit(Edit::Clear).is_err());
    assert!(editor.request_apply().is_err());
    editor.select("one").unwrap();
    assert_eq!(editor.baseline(), Some(&opaque));
    editor.select("two").unwrap();
    assert!(editor.baseline().is_none());
    assert_eq!(editor.status(), &Status::Unloaded);
}

#[test]
fn rejected_edits_are_atomic_and_dirty_slot_change_is_rejected() {
    let mut editor = Editor::new(caps()).unwrap();
    editor.accept_read(Ok(snapshot(1, Content::Editable(program(1)))));
    assert!(editor.edit(Edit::Repeat(99)).is_err());
    assert_eq!(editor.draft(), Some(&program(1)));
    editor.edit(Edit::Repeat(2)).unwrap();
    assert!(editor.select("two").is_err());
    assert_eq!(editor.slot(), "one");
    editor.invalidate();
    assert!(editor.edit(Edit::Clear).is_err());
    editor.revert().unwrap();
    assert_eq!(
        editor.status(),
        &Status::Unverified {
            problem: Problem::ReadRequired
        }
    );
}
