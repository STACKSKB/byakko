use super::*;
use crate::{
    macro_editor::Message as Macro,
    macro_form::{Input, Kind},
};
use byakko_core::macros::{Content, Edit, editor::Status as MacroStatus};
use byakko_devices::Device;

#[test]
fn explicit_new_empty_slot_stages_editable_count_without_changing_stored_zero() {
    let mut app = ready();
    let mut device = demo::device().unwrap();
    let mut caps = device.macro_capabilities().unwrap().clone();
    caps.editable_repeat_counts = 1..=12;
    let mut session = Session::new(device.descriptor().clone())
        .unwrap()
        .with_macros(caps)
        .unwrap();
    let generation = session.connect().unwrap();
    session.select_macro("spare").unwrap();
    let Command::ReadMacro {
        operation, slot, ..
    } = session.request_macro_read().unwrap()
    else {
        unreachable!()
    };
    session.accept(Completion::ReadMacro {
        generation,
        operation,
        slot,
        result: device.read_macro("spare"),
    });
    app.session = session;
    app.initialize_new_macro();
    assert_eq!(
        app.session.macros().unwrap().draft().unwrap().repeat_count,
        0
    );

    app.macro_new_slot = Some("spare".into());
    app.initialize_new_macro();
    let editor = app.session.macros().unwrap();
    assert_eq!(
        editor.baseline().unwrap().content,
        Content::Editable(byakko_core::macros::Program {
            repeat_count: 0,
            events: vec![]
        })
    );
    assert_eq!(editor.draft().unwrap().repeat_count, 1);
    assert!(editor.dirty());
    assert_eq!(app.repeat_input, "1");
}

#[test]
fn add_uses_first_free_slot_and_capacity_disables_further_adds() {
    let mut app = ready();
    let _ = app.update(Message::Page(Page::Macros));
    app.read_macro_catalog_in_background();
    settle(&mut app);
    let editor = app.session.macros().unwrap();
    assert_eq!(editor.configured_slots().unwrap().len(), 3);
    assert_eq!(app.session.next_free_macro_slot(), Some("spare"));

    send(&mut app, Macro::Add);
    settle(&mut app);
    assert_eq!(app.macro_new_slot.as_deref(), Some("spare"));
    assert_eq!(app.session.macros().unwrap().slot(), "spare");
    send(&mut app, Macro::Edit(Edit::Repeat(1)));
    send(
        &mut app,
        Macro::Edit(Edit::Insert {
            at: 0,
            event: byakko_core::macros::Event {
                action: byakko_core::macros::Action::Key {
                    usage: 4,
                    pressed: true,
                },
                delay_ms: 10,
            },
        }),
    );
    send(&mut app, Macro::Apply);
    settle(&mut app);
    assert_eq!(app.macro_new_slot, None);
    assert_eq!(
        app.session
            .macros()
            .unwrap()
            .configured_slots()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(app.session.next_free_macro_slot(), None);
    send(&mut app, Macro::Add);
    assert!(!app.busy());
    assert_eq!(app.session.macros().unwrap().slot(), "spare");
}

#[test]
fn reselecting_ready_macro_keeps_candidate_and_does_not_read_again() {
    let mut app = ready();
    app.read_macro_catalog_in_background();
    settle(&mut app);
    send(&mut app, Macro::Add);
    settle(&mut app);
    assert_eq!(app.macro_new_slot.as_deref(), Some("spare"));
    assert_eq!(app.session.macros().unwrap().status(), &MacroStatus::Ready);
    send(&mut app, Macro::Select("spare".into()));
    assert_eq!(app.macro_new_slot.as_deref(), Some("spare"));
    assert!(!app.busy());
    assert_eq!(app.session.macros().unwrap().status(), &MacroStatus::Ready);
}

#[test]
fn macro_scan_is_passive_and_navigation_keeps_the_same_scan() {
    let mut app = ready();
    app.read_macro_catalog_in_background();
    assert!(app.session.macro_catalog_scanning());
    assert!(!app.busy());
    let _ = app.update(Message::Page(Page::Macros));
    assert_eq!(
        app.session.activity(),
        &byakko_core::session::Activity::Idle
    );
    let _ = app.update(Message::Page(Page::Keys));
    let _ = app.update(Message::SelectKey("Alpha".into()));
    app.stage(1);
    assert!(!app.session.changes().is_empty());
    let draft = app.session.draft().cloned();
    settle(&mut app);
    assert_eq!(app.session.draft(), draft.as_ref());
    assert_eq!(
        app.session
            .macros()
            .unwrap()
            .configured_slots()
            .unwrap()
            .len(),
        3
    );
    let _ = app.update(Message::Page(Page::Macros));
    assert!(!app.session.macro_catalog_scanning());
    assert!(!app.busy());
}

#[test]
fn slot_editor_opens_during_background_catalog_scan() {
    let mut app = ready();
    app.read_macro_catalog_in_background();
    assert!(app.session.macro_catalog_scanning());
    assert!(app.session.macros().unwrap().catalog().is_none());
    let _ = app.update(Message::Page(Page::Macros));
    drop(app.view());
    send(&mut app, Macro::Select("intro".into()));
    assert!(matches!(
        app.session.activity(),
        byakko_core::session::Activity::ReadMacro { .. }
    ));
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while app.busy() {
        assert!(std::time::Instant::now() < deadline);
        let _ = app.poll();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(app.session.macros().unwrap().status(), &MacroStatus::Ready);
    assert!(app.session.macros().unwrap().draft().is_some());
    drop(app.view());
}

#[test]
fn add_checks_candidates_before_catalog_completes() {
    let mut app = ready();
    let _ = app.update(Message::Page(Page::Macros));
    assert!(app.session.macros().unwrap().catalog().is_none());
    let first = app
        .session
        .next_macro_candidate_after(None)
        .unwrap()
        .to_owned();
    send(&mut app, Macro::Add);
    assert_eq!(app.macro_new_slot.as_deref(), Some(first.as_str()));
    assert!(matches!(
        app.session.activity(),
        byakko_core::session::Activity::ReadMacro { .. }
    ));
    settle(&mut app);
    let editor = app.session.macros().unwrap();
    assert_eq!(editor.status(), &MacroStatus::Ready);
    assert_eq!(editor.slot(), first);
    let still_scanning = editor.catalog().is_none();
    if still_scanning
        && matches!(editor.baseline().map(|snapshot| &snapshot.content), Some(Content::Editable(program)) if !program.events.is_empty())
    {
        let next = app
            .session
            .next_macro_candidate_after(Some(&first))
            .unwrap()
            .to_owned();
        send(&mut app, Macro::Add);
        assert_eq!(app.macro_new_slot.as_deref(), Some(next.as_str()));
        settle(&mut app);
        assert_eq!(app.session.macros().unwrap().slot(), next);
    }
}
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

#[test]
fn unchanged_slot_read_preserves_unsubmitted_event_fields() {
    let mut app = loaded();
    send(&mut app, Macro::Inspect(0));
    send(&mut app, Macro::Form(Input::Wait("123".into())));
    let draft = app.session.macros().unwrap().draft().cloned();

    send(&mut app, Macro::Read);
    settle(&mut app);
    assert_eq!(app.session.macros().unwrap().draft(), draft.as_ref());
    assert_eq!(app.macro_form.target, Some(0));
    assert_eq!(app.macro_form.wait, "123");

    send(&mut app, Macro::Select("intro".into()));
    settle(&mut app);
    assert_eq!(app.macro_form.target, Some(0));
    assert_eq!(app.macro_form.wait, "123");
}

fn send(app: &mut Desktop, message: Macro) {
    let _ = app.update(Message::Macro(message));
}

pub(super) fn settle(app: &mut Desktop) {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while app.busy()
        || app.session.macro_catalog_scanning()
        || app.live_lighting.has_pending()
        || app.live_picture.has_pending()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "memory worker timed out"
        );
        let _ = app.poll();
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub(crate) fn loaded() -> Desktop {
    let mut app = ready();
    send(&mut app, Macro::Select("intro".into()));
    settle(&mut app);
    assert_eq!(app.session.macros().unwrap().status(), &MacroStatus::Ready);
    app
}

#[test]
fn binding_requires_saved_compatible_macro_and_uses_backend_action() {
    let mut app = loaded();
    let _ = app.update(Message::Page(Page::Macros));
    send(&mut app, Macro::Select("pointer".into()));
    settle(&mut app);
    send(&mut app, Macro::ChooseBinding("hold".into()));
    send(&mut app, Macro::Assign("hold".into()));
    assert!(
        app.macro_notice
            .as_deref()
            .unwrap()
            .contains("repeat count")
    );
    assert!(app.session.changes().is_empty());
    assert_eq!(
        app.session.macros().unwrap().draft().unwrap().repeat_count,
        2
    );
    let _ = app.update(Message::SelectLayer("Studio".into()));
    let _ = app.update(Message::SelectKey("Fixed".into()));
    send(&mut app, Macro::ChooseBinding("play".into()));
    send(&mut app, Macro::Assign("play".into()));
    assert!(app.macro_notice.is_some());
    assert!(app.session.changes().is_empty());
    let _ = app.update(Message::SelectKey("Beta".into()));
    send(&mut app, Macro::Assign("play".into()));
    settle(&mut app);
    assert_eq!(
        app.session.baseline().unwrap().bindings["Studio"]["Beta"],
        Action::Named {
            id: "sequence/pointer/play".into()
        }
    );
    assert_eq!(app.page, Page::Macros);
    send(&mut app, Macro::Read);
    settle(&mut app);
    send(&mut app, Macro::Edit(Edit::Repeat(1)));
    send(&mut app, Macro::ChooseBinding("hold".into()));
    send(&mut app, Macro::Assign("hold".into()));
    assert!(
        app.macro_notice.is_some(),
        "an unsaved count must not authorize binding"
    );
    assert!(app.session.changes().is_empty());
    send(&mut app, Macro::Apply);
    settle(&mut app);
    send(&mut app, Macro::Assign("hold".into()));
    settle(&mut app);
    assert!(app.macro_notice.is_none());
    assert_eq!(app.page, Page::Macros);
    let action = Action::Named {
        id: "sequence/pointer/hold".into(),
    };
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
fn chosen_playback_assigns_and_verifies_without_leaving_macros() {
    let mut app = loaded();
    let _ = app.update(Message::Page(Page::Macros));
    send(&mut app, Macro::Select("pointer".into()));
    settle(&mut app);
    let _ = app.update(Message::SelectLayer("Studio".into()));
    let _ = app.update(Message::SelectKey("Beta".into()));

    send(&mut app, Macro::ChooseBinding("play".into()));
    assert!(
        app.session.changes().is_empty(),
        "choosing only previews playback"
    );
    assert_eq!(app.page, Page::Macros);
    send(&mut app, Macro::Assign("play".into()));
    settle(&mut app);
    assert!(app.macro_notice.is_none());
    assert_eq!(app.page, Page::Macros);
    assert!(app.session.changes().is_empty());
    assert_eq!(
        app.session.baseline().unwrap().bindings["Studio"]["Beta"],
        Action::Named {
            id: "sequence/pointer/play".into()
        }
    );
}

#[test]
fn assigning_macro_does_not_save_unrelated_key_draft() {
    let mut app = loaded();
    let _ = app.update(Message::SelectKey("Beta".into()));
    app.stage(1);
    let before = app.session.changes();
    send(&mut app, Macro::ChooseBinding("play".into()));
    send(&mut app, Macro::Assign("play".into()));
    assert_eq!(app.session.changes(), before);
    assert!(!app.busy());
    assert!(
        app.macro_notice
            .as_deref()
            .unwrap()
            .contains("other key assignments")
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
    assert!(app.macro_notice.is_some());
    assert_eq!(app.session.macros().unwrap().slot(), "intro");
    send(&mut app, Macro::Apply);
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    settle(&mut app);
    assert_eq!(
        app.closing,
        Closing::ConfirmDiscard,
        "successful macro save preserves the unsaved keymap draft"
    );
    assert_eq!(app.session.changes().len(), 1);
    assert_eq!(app.session.status(), &Status::Ready);
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
    assert!(app.macro_notice.is_some());
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
    assert!(app.macro_notice.is_some());
    assert_eq!(app.session.macros().unwrap().baseline(), opaque.as_ref());
}

#[test]
fn staging_repeat_count_preserves_an_unsubmitted_event_edit() {
    let mut app = loaded();
    send(&mut app, Macro::Inspect(0));
    send(&mut app, Macro::Form(Input::Wait("37".into())));
    send(&mut app, Macro::RepeatInput("03".into()));
    send(&mut app, Macro::StageRepeat);

    assert!(app.macro_notice.is_none());
    assert_eq!(app.repeat_input, "3");
    assert_eq!(app.macro_form.target, Some(0));
    assert_eq!(app.macro_form.wait, "37");
    assert_eq!(
        app.session.macros().unwrap().draft().unwrap().repeat_count,
        3
    );

    send(&mut app, Macro::StageEvent);
    assert!(app.macro_notice.is_none());
    assert_eq!(
        app.session.macros().unwrap().draft().unwrap().events[0].delay_ms,
        37
    );
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
