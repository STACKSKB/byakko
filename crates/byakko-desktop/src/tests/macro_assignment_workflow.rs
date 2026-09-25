use super::*;
use byakko_core::{
    macros::{Edit, editor::Status as MacroStatus},
    session::{Activity, ApplyFailure, Recovery},
};

fn loaded_pointer() -> Desktop {
    let mut app = macro_workflow::loaded();
    let _ = app.update(Message::Macro(macro_editor::Message::Select(
        "pointer".into(),
    )));
    macro_workflow::settle(&mut app);
    let _ = app.update(Message::SelectLayer("Studio".into()));
    let _ = app.update(Message::SelectKey("Beta".into()));
    app
}

#[test]
fn dirty_macro_saves_then_assigns_the_captured_key() {
    let mut app = loaded_pointer();
    let _ = app.update(Message::Macro(macro_editor::Message::Edit(Edit::Repeat(1))));
    app.repeat_input = "1".into();
    assert!(app.session.macros().unwrap().dirty());
    assert!(app.assignment_problem("hold").is_none());
    app.save_and_assign_macro("hold".into());
    assert!(matches!(
        app.session.activity(),
        Activity::ApplyMacro { .. }
    ));
    let _ = app.update(Message::SelectKey("Alpha".into()));
    macro_workflow::settle(&mut app);
    assert!(app.macro_notice.is_none());
    assert!(!app.session.macros().unwrap().dirty());
    assert_eq!(
        app.session.macros().unwrap().draft().unwrap().repeat_count,
        1
    );
    assert_eq!(
        app.session.baseline().unwrap().bindings["Studio"]["Beta"],
        Action::Named {
            id: "sequence/pointer/hold".into()
        }
    );
    assert_ne!(
        app.session.baseline().unwrap().bindings["Studio"]["Alpha"],
        Action::Named {
            id: "sequence/pointer/hold".into()
        }
    );
}

#[test]
fn failed_macro_save_never_stages_key_assignment() {
    let mut app = loaded_pointer();
    let key_before = app.session.baseline().unwrap().bindings["Studio"]["Beta"].clone();
    let _ = app.update(Message::Macro(macro_editor::Message::Edit(Edit::Repeat(1))));
    app.repeat_input = "1".into();
    app.save_and_assign_macro("hold".into());
    let Activity::ApplyMacro { operation, slot } = app.session.activity().clone() else {
        panic!("save did not start");
    };
    let generation = app.session.generation();
    let _ = app.complete(Completion::ApplyMacro {
        generation,
        operation: operation + 1,
        slot: slot.clone(),
        result: Err(ApplyFailure {
            message: "stale".into(),
            recovery: Recovery::NotAttempted,
        }),
    });
    assert!(app.macro_assignment.is_some());
    let _ = app.complete(Completion::ApplyMacro {
        generation,
        operation,
        slot,
        result: Err(ApplyFailure {
            message: "write failed".into(),
            recovery: Recovery::Verified,
        }),
    });
    assert!(app.macro_assignment.is_none());
    assert!(matches!(
        app.session.macros().unwrap().status(),
        MacroStatus::Unverified { .. }
    ));
    assert!(app.session.changes().is_empty());
    assert_eq!(
        app.session.baseline().unwrap().bindings["Studio"]["Beta"],
        key_before
    );
    assert!(
        app.macro_notice
            .as_deref()
            .unwrap()
            .contains("Macro save failed")
    );
}

#[test]
fn key_failure_reports_macro_saved_without_claiming_assignment() {
    let mut app = loaded_pointer();
    let key_before = app.session.baseline().unwrap().bindings["Studio"]["Beta"].clone();
    app.save_and_assign_macro("play".into());
    let Activity::Apply { operation } = app.session.activity() else {
        panic!("key assignment did not start");
    };
    let operation = *operation;
    let _ = app.complete(Completion::Apply {
        generation: app.session.generation(),
        operation,
        result: Err(ApplyFailure {
            message: "readback failed".into(),
            recovery: Recovery::Failed,
        }),
    });
    assert!(app.macro_assignment.is_none());
    assert_eq!(
        app.session.baseline().unwrap().bindings["Studio"]["Beta"],
        key_before
    );
    assert!(
        app.macro_notice
            .as_deref()
            .unwrap()
            .contains("Macro saved; key assignment failed")
    );
}

#[test]
fn invalid_count_and_unrelated_key_edits_stop_before_save() {
    let mut app = loaded_pointer();
    let _ = app.update(Message::Macro(macro_editor::Message::Edit(Edit::Repeat(1))));
    app.repeat_input = "bad".into();
    assert!(app.assignment_problem("hold").is_some());
    app.save_and_assign_macro("hold".into());
    assert!(!app.busy());
    app.repeat_input = "1".into();
    app.stage(1);
    assert!(
        app.assignment_problem("hold")
            .unwrap()
            .contains("other key assignments")
    );
    app.save_and_assign_macro("hold".into());
    assert!(!app.busy());
    assert!(app.session.macros().unwrap().dirty());
}

#[test]
fn close_stays_open_when_macro_saves_but_key_assignment_cannot_start() {
    let mut app = loaded_pointer();
    let _ = app.update(Message::Macro(macro_editor::Message::Edit(Edit::Repeat(1))));
    app.repeat_input = "1".into();
    app.save_and_assign_macro("hold".into());
    let Activity::ApplyMacro { operation, slot } = app.session.activity().clone() else {
        panic!("macro save did not start");
    };
    let generation = app.session.generation();
    let mut snapshot = app.session.macros().unwrap().baseline().unwrap().clone();
    snapshot.content = byakko_core::macros::Content::Editable(
        app.session.macros().unwrap().draft().unwrap().clone(),
    );
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    app.executor = None;
    let _ = app.complete(Completion::ApplyMacro {
        generation,
        operation,
        slot,
        result: Ok(snapshot),
    });
    assert_eq!(app.closing, Closing::Open);
    assert!(app.macro_assignment.is_none());
    assert!(
        app.macro_notice
            .as_deref()
            .unwrap()
            .contains("key assignment could not start")
    );
}
