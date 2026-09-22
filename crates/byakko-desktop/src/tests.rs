use super::*;
use byakko_core::{
    Action,
    session::{ApplyFailure, Recovery},
};
use byakko_devices::KeymapDevice;

#[path = "demo.rs"]
mod demo;

fn ready() -> Desktop {
    let mut device = demo::device().unwrap();
    let mut session = KeymapSession::new(device.descriptor().clone()).unwrap();
    let generation = session.connect().unwrap();
    let Command::Read { operation, .. } = session.request_read().unwrap() else {
        unreachable!()
    };
    session.accept(Completion::Read {
        generation,
        operation,
        result: device.read(),
    });
    Desktop {
        session,
        executor: Executor::spawn(device, Default::default()).unwrap(),
        layer: "Typing".into(),
        selected: Some("Alpha".into()),
        search: String::new(),
        notice: None,
        closing: Closing::Open,
    }
}

#[test]
fn stages_only_selected_generic_layer_and_preserves_fixed_opaque_action() {
    let mut app = ready();
    let _ = app.update(Message::SelectLayer("Studio".into()));
    let _ = app.update(Message::Stage(1));
    assert_eq!(
        app.session.changes(),
        vec![Change {
            layer: "Studio".into(),
            key: "Alpha".into(),
            action: Action::Key(5)
        }]
    );
    let draft = app.session.draft().unwrap().clone();
    let _ = app.update(Message::SelectKey("Fixed".into()));
    let _ = app.update(Message::Stage(2));
    assert_eq!(app.session.draft(), Some(&draft));
    assert!(app.notice.is_some());
    assert!(matches!(draft["Studio"]["Fixed"], Action::Opaque { .. }));
}

#[test]
fn close_waits_for_apply_and_keeps_failure_and_draft_visible() {
    let mut app = ready();
    app.stage(1);
    let Command::Apply {
        generation,
        operation,
        ..
    } = app.session.request_apply().unwrap()
    else {
        unreachable!()
    };
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    let _ = app.update(Message::DiscardAndClose);
    assert!(app.busy());
    let _ = app.complete(Completion::Apply {
        generation,
        operation,
        result: Err(ApplyFailure {
            message: "readback failed".into(),
            recovery: Recovery::Unverified,
        }),
    });
    assert_eq!(app.closing, Closing::Open);
    assert_eq!(app.session.changes().len(), 1);
    assert!(matches!(app.session.status(), Status::Unverified { .. }));
    assert!(app.session.request_apply().is_err());
}

#[test]
fn dirty_close_confirmation_is_dismissed_by_new_edit_and_clean_close_does_not_mutate() {
    let mut app = ready();
    app.stage(1);
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::ConfirmDiscard);
    let _ = app.update(Message::Revert);
    assert_eq!(app.closing, Closing::Open);
    assert!(app.session.changes().is_empty());
    let baseline = app.session.baseline().cloned();
    let _ = app.update(Message::Close);
    assert_eq!(app.session.baseline(), baseline.as_ref());
}
