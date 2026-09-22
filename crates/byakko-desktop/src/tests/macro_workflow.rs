use super::*;
use crate::{
    macro_editor::Message as Macro,
    macro_form::{Input, Kind},
};
use byakko_core::macros::{Content, Edit, editor::Status as MacroStatus};

#[test]
fn failed_read_preserves_unsubmitted_form_input() {
    let mut app = loaded();
    send(&mut app, Macro::Inspect(0));
    send(&mut app, Macro::Form(Input::Wait("123".into())));
    let Command::ReadMacro {
        generation,
        operation,
        slot,
    } = app.session.request_macro_read().unwrap()
    else {
        unreachable!()
    };
    let _ = app.complete(Completion::ReadMacro {
        generation,
        operation,
        slot,
        result: Err("device gone".into()),
    });
    assert_eq!(app.macro_form.wait, "123");
    assert_eq!(app.macro_form.target, Some(0));
    assert!(matches!(
        app.session.macros().unwrap().status(),
        MacroStatus::Unverified { .. }
    ));
}

fn send(app: &mut Desktop, message: Macro) {
    let _ = app.update(Message::Macro(message));
}

fn settle(app: &mut Desktop) {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while app.busy() {
        assert!(
            std::time::Instant::now() < deadline,
            "memory worker timed out"
        );
        let _ = app.poll();
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn loaded() -> Desktop {
    let mut app = ready();
    send(&mut app, Macro::Select("intro".into()));
    settle(&mut app);
    assert_eq!(app.session.macros().unwrap().status(), &MacroStatus::Ready);
    app
}

#[test]
fn binding_policy_is_explicit_and_saved_binding_uses_backend_action() {
    let mut app = loaded();
    send(&mut app, Macro::Select("pointer".into()));
    settle(&mut app);
    send(&mut app, Macro::Bind("hold".into()));
    assert!(app.notice.is_some());
    assert!(app.session.changes().is_empty());
    assert_eq!(
        app.session.macros().unwrap().draft().unwrap().repeat_count,
        2
    );
    let _ = app.update(Message::SelectLayer("Studio".into()));
    let _ = app.update(Message::SelectKey("Fixed".into()));
    send(&mut app, Macro::Bind("play".into()));
    assert!(app.notice.is_some());
    assert!(app.session.changes().is_empty());
    let _ = app.update(Message::SelectKey("Beta".into()));
    send(&mut app, Macro::Bind("play".into()));
    let staged = app.session.changes();
    assert_eq!(
        staged,
        vec![Change {
            layer: "Studio".into(),
            key: "Beta".into(),
            action: Action::Named {
                id: "sequence/pointer/play".into()
            },
        }]
    );
    send(&mut app, Macro::Edit(Edit::Repeat(1)));
    send(&mut app, Macro::Bind("hold".into()));
    assert!(
        app.notice.is_some(),
        "an unsaved count must not authorize binding"
    );
    assert_eq!(app.session.changes(), staged);
    send(&mut app, Macro::Apply);
    settle(&mut app);
    send(&mut app, Macro::Bind("hold".into()));
    assert!(
        app.notice.is_some(),
        "keymap must be reread after a macro write"
    );
    let _ = app.update(Message::Read);
    settle(&mut app);
    send(&mut app, Macro::Read);
    settle(&mut app);
    send(&mut app, Macro::Bind("hold".into()));
    assert!(app.notice.is_none());
    assert_eq!(app.page, Page::Keys);
    let action = Action::Named {
        id: "sequence/pointer/hold".into(),
    };
    assert_eq!(app.session.changes()[0].action, action);
    let _ = app.update(Message::Apply);
    settle(&mut app);
    assert_eq!(
        app.session.baseline().unwrap().bindings["Studio"]["Beta"],
        action
    );
    assert!(app.session.changes().is_empty());
    send(&mut app, Macro::Read);
    settle(&mut app);
    assert_eq!(
        app.session.macros().unwrap().draft().unwrap().repeat_count,
        1
    );
}

#[test]
fn form_projection_preserves_every_demo_action_and_wait() {
    let mut device = demo::device().unwrap();
    let caps = device.macro_capabilities().unwrap().clone();
    for slot in ["intro", "pointer"] {
        let Content::Editable(program) = device.read_macro(slot).unwrap().content else {
            unreachable!()
        };
        for (index, event) in program.events.iter().enumerate() {
            let form = crate::macro_form::Form::from_event(index, event, &caps);
            assert_eq!(form.event(&caps).unwrap(), *event);
            assert_eq!(form.target, Some(index));
        }
    }
}

#[test]
fn messages_edit_and_verify_a_memory_slot_without_losing_keymap_draft() {
    let mut app = loaded();
    app.stage(1);
    send(&mut app, Macro::Inspect(0));
    send(&mut app, Macro::Form(Input::Wait("0".into())));
    send(&mut app, Macro::StageEvent);
    send(&mut app, Macro::Form(Input::Kind(Kind::Move)));
    send(&mut app, Macro::Form(Input::First("-8".into())));
    send(&mut app, Macro::Form(Input::Second("3".into())));
    send(&mut app, Macro::Form(Input::Wait("20".into())));
    send(&mut app, Macro::StageEvent);
    send(&mut app, Macro::RepeatInput("0".into()));
    send(&mut app, Macro::StageRepeat);
    let desired = app.session.macros().unwrap().draft().unwrap().clone();
    assert_eq!(desired.events.len(), 3);
    assert_eq!(desired.events[0].delay_ms, 0);
    assert_eq!(desired.repeat_count, 0);
    send(&mut app, Macro::Select("pointer".into()));
    assert!(app.notice.is_some());
    assert_eq!(app.session.macros().unwrap().slot(), "intro");
    send(&mut app, Macro::Apply);
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    settle(&mut app);
    assert_eq!(
        app.closing,
        Closing::ConfirmDiscard,
        "successful macro save can finish close even when keymap trust was invalidated"
    );
    assert_eq!(app.session.changes().len(), 1);
    assert!(matches!(app.session.status(), Status::Unverified { .. }));
    assert!(!app.session.macros().unwrap().dirty());
    send(&mut app, Macro::Read);
    settle(&mut app);
    assert_eq!(
        app.session.macros().unwrap().baseline().unwrap().content,
        Content::Editable(desired)
    );
}

#[test]
fn rejected_input_is_atomic_and_sequence_edits_clear_replacement_target() {
    let mut app = loaded();
    let original = app.session.macros().unwrap().draft().cloned();
    send(&mut app, Macro::Inspect(0));
    send(&mut app, Macro::Form(Input::Wait("2001".into())));
    send(&mut app, Macro::StageEvent);
    assert!(app.notice.is_some());
    assert_eq!(app.macro_form.wait, "2001");
    assert_eq!(app.session.macros().unwrap().draft(), original.as_ref());
    send(&mut app, Macro::Edit(Edit::Move { from: 0, to: 1 }));
    assert_eq!(app.macro_form.target, None);
    send(&mut app, Macro::Inspect(1));
    send(&mut app, Macro::Edit(Edit::Remove { at: 1 }));
    assert_eq!(app.macro_form.target, None);
    send(&mut app, Macro::Revert);
    assert_eq!(app.session.macros().unwrap().draft(), original.as_ref());
    send(&mut app, Macro::Select("archive".into()));
    settle(&mut app);
    let opaque = app.session.macros().unwrap().baseline().cloned();
    send(&mut app, Macro::Edit(Edit::Clear));
    send(&mut app, Macro::Apply);
    assert!(!app.busy());
    assert!(app.notice.is_some());
    assert_eq!(app.session.macros().unwrap().baseline(), opaque.as_ref());
}

#[test]
fn macro_failure_prevents_close_and_stale_success_cannot_hide_it() {
    let mut app = loaded();
    send(&mut app, Macro::Edit(Edit::Clear));
    let Command::ApplyMacro {
        generation,
        operation,
        expected,
        ..
    } = app.session.request_macro_apply().unwrap()
    else {
        unreachable!()
    };
    let _ = app.update(Message::Close);
    let _ = app.complete(Completion::ApplyMacro {
        generation,
        operation,
        slot: expected.slot.clone(),
        result: Err(ApplyFailure {
            message: "readback failed".into(),
            recovery: Recovery::Failed,
        }),
    });
    assert_eq!(app.closing, Closing::Open);
    let editor = app.session.macros().unwrap();
    assert!(editor.dirty());
    assert!(matches!(editor.status(), MacroStatus::Unverified { .. }));
    let _ = app.complete(Completion::ApplyMacro {
        generation,
        operation,
        slot: expected.slot.clone(),
        result: Ok(expected),
    });
    assert_eq!(app.closing, Closing::Open);
    assert!(matches!(
        app.session.macros().unwrap().status(),
        MacroStatus::Unverified { .. }
    ));
}
