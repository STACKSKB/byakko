use super::*;
use crate::lighting::Message as Lighting;
use byakko_core::lighting::{Channel, Color, Content, Edit, editor::Status as LightingStatus};
use macro_workflow::settle;

fn send(app: &mut Desktop, message: Lighting) {
    let _ = app.update(Message::Lighting(message));
}

fn loaded() -> Desktop {
    let mut app = ready();
    let _ = app.update(Message::Page(Page::Lighting));
    send(&mut app, Lighting::Read);
    settle(&mut app);
    assert_eq!(
        app.session.lighting().unwrap().status(),
        &LightingStatus::Ready
    );
    app
}

#[test]
fn lighting_page_uses_explicit_read_and_keeps_cached_snapshot_on_revisit() {
    let mut app = ready();
    let _ = app.update(Message::Page(Page::Lighting));
    assert!(matches!(
        app.session.activity(),
        byakko_core::session::Activity::Idle
    ));
    send(&mut app, Lighting::Read);
    settle(&mut app);
    assert_eq!(
        app.session.lighting().unwrap().status(),
        &LightingStatus::Ready
    );
    let _ = app.update(Message::Page(Page::Keys));
    let _ = app.update(Message::Page(Page::Lighting));
    assert!(!app.busy());
    assert!(matches!(
        app.session.activity(),
        byakko_core::session::Activity::Idle
    ));

    let mut app = ready();
    let _ = app.update(Message::Read);
    let _ = app.update(Message::Page(Page::Lighting));
    settle(&mut app);
    send(&mut app, Lighting::Read);
    settle(&mut app);
    assert_eq!(
        app.session.lighting().unwrap().status(),
        &LightingStatus::Ready
    );
}

#[test]
fn reconnect_keeps_both_keymap_and_lighting_conflict_diagnostics() {
    let mut app = loaded();
    app.stage(1);
    app.session.edit_lighting(Edit::Brightness(63)).unwrap();

    let mut changed_keys = app.session.baseline().unwrap().clone();
    changed_keys.revision.push(2);
    let Command::Read {
        generation,
        operation,
    } = app.session.request_read().unwrap()
    else {
        unreachable!()
    };
    app.session.accept(Completion::Read {
        generation,
        operation,
        result: Ok(changed_keys),
    });
    assert!(matches!(app.session.status(), Status::Conflict { .. }));

    let mut changed_lighting = app.session.lighting().unwrap().baseline().unwrap().clone();
    changed_lighting.revision.push(2);
    let Command::ReadLighting {
        generation,
        operation,
    } = app.session.request_lighting_read().unwrap()
    else {
        unreachable!()
    };
    app.session.accept(Completion::ReadLighting {
        generation,
        operation,
        result: Ok(changed_lighting),
    });
    assert!(matches!(
        app.session.lighting().unwrap().status(),
        LightingStatus::Conflict { .. }
    ));

    app.accept_availability(Availability::Missing);
    assert_eq!(app.auto_read, AutoRead::ManualOnly);
    let notice = app.notice.as_deref().unwrap();
    assert!(notice.contains("Keys: Device values conflict"));
    assert!(notice.contains("Lighting: Device values conflict"));
    app.accept_availability(Availability::Ready {
        id: "demo-2".into(),
    });
    assert_eq!(app.session.status(), &Status::Disconnected);
}

#[test]
fn lighting_uses_capabilities_and_memory_executor_without_losing_other_drafts() {
    let mut app = loaded();
    app.stage(1);
    let keys = app.session.changes();
    send(&mut app, Lighting::Edit(Edit::Effect("sweep".into())));
    send(&mut app, Lighting::Edit(Edit::Brightness(63)));
    send(&mut app, Lighting::Edit(Edit::Speed(7)));
    send(&mut app, Lighting::Edit(Edit::Option("in".into())));
    send(&mut app, Lighting::Edit(Edit::Channel(Channel::Red, 20)));
    send(&mut app, Lighting::Edit(Edit::Channel(Channel::Blue, 40)));
    let draft = app.session.lighting().unwrap().draft().unwrap().clone();
    assert_eq!(draft.color, Some(Color::Rgb([20, 255, 40])));
    assert_eq!(draft.brightness, Some(63));
    assert_eq!(draft.speed, Some(7));
    assert_eq!(draft.option.as_deref(), Some("in"));
    send(&mut app, Lighting::Edit(Edit::Effect("sweep".into())));
    assert_eq!(app.session.lighting().unwrap().draft(), Some(&draft));
    send(&mut app, Lighting::Edit(Edit::Brightness(81)));
    assert!(app.notice.is_some());
    assert_eq!(app.session.lighting().unwrap().draft(), Some(&draft));
    drop(app.view());
    send(&mut app, Lighting::Apply);
    assert!(app.busy());
    send(&mut app, Lighting::Edit(Edit::Color(Color::Rainbow)));
    assert_eq!(app.session.lighting().unwrap().draft(), Some(&draft));
    settle(&mut app);
    assert!(app.notice.is_none());
    assert_eq!(
        app.session.lighting().unwrap().status(),
        &LightingStatus::Ready
    );
    assert!(!app.session.lighting().unwrap().dirty());
    assert_eq!(app.session.changes(), keys);
    assert_eq!(app.session.status(), &Status::Ready);
    send(&mut app, Lighting::Read);
    settle(&mut app);
    assert_eq!(
        app.session.lighting().unwrap().baseline().unwrap().content,
        Content::Editable(draft)
    );
    send(&mut app, Lighting::Edit(Edit::Color(Color::Rainbow)));
    send(&mut app, Lighting::Edit(Edit::Channel(Channel::Green, 2)));
    assert!(app.notice.is_some());
    assert_eq!(
        app.session.lighting().unwrap().draft().unwrap().color,
        Some(Color::Rainbow)
    );
    send(&mut app, Lighting::Edit(Edit::Effect("off".into())));
    let off = app.session.lighting().unwrap().draft().unwrap();
    assert_eq!(
        (off.brightness, off.speed, &off.option, &off.color),
        (None, None, &None, &None)
    );
    send(&mut app, Lighting::Revert);
    assert!(!app.session.lighting().unwrap().dirty());
}

#[test]
fn live_onboard_choices_apply_and_queue_during_the_first_write() {
    let mut app = loaded();
    send(&mut app, Lighting::Live(Edit::Effect("sweep".into())));
    assert!(app.busy());
    send(&mut app, Lighting::Live(Edit::Option("in".into())));
    assert!(app.live_lighting.has_pending());
    settle(&mut app);
    let editor = app.session.lighting().unwrap();
    let Some(Content::Editable(saved)) = editor.baseline().map(|snapshot| &snapshot.content) else {
        panic!("expected editable lighting readback");
    };
    assert_eq!(saved.effect, "sweep");
    assert_eq!(saved.option.as_deref(), Some("in"));
    assert!(!app.live_lighting.has_queued());
}

#[test]
fn live_effect_click_survives_an_unrelated_read() {
    let mut app = loaded();
    let request = app.session.request_settings_read();
    app.submit(request);
    assert!(app.busy());
    send(&mut app, Lighting::Live(Edit::Effect("sweep".into())));
    assert!(app.live_lighting.has_pending());
    settle(&mut app);
    let editor = app.session.lighting().unwrap();
    let Some(Content::Editable(saved)) = editor.baseline().map(|snapshot| &snapshot.content) else {
        panic!("expected editable lighting readback");
    };
    assert_eq!(saved.effect, "sweep");
}

#[test]
fn close_waits_for_lighting_and_failed_write_retains_draft_and_diagnostic() {
    let mut app = loaded();
    send(&mut app, Lighting::Edit(Edit::Brightness(42)));
    let baseline = app.session.lighting().unwrap().baseline().cloned();
    let draft = app.session.lighting().unwrap().draft().cloned();
    let Command::ApplyLighting {
        generation,
        operation,
        ..
    } = app.session.request_lighting_apply().unwrap()
    else {
        unreachable!()
    };
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    let _ = app.complete(Completion::ApplyLighting {
        generation,
        operation: operation + 1,
        result: Ok(baseline.clone().unwrap()),
    });
    assert_eq!(app.closing, Closing::Waiting);
    assert!(app.busy());
    let failure = ApplyFailure {
        message: "readback mismatch".into(),
        recovery: Recovery::Failed,
    };
    let _ = app.complete(Completion::ApplyLighting {
        generation,
        operation,
        result: Err(failure.clone()),
    });
    assert_eq!(app.closing, Closing::Open);
    let editor = app.session.lighting().unwrap();
    assert_eq!(editor.baseline(), baseline.as_ref());
    assert_eq!(editor.draft(), draft.as_ref());
    assert_eq!(
        editor.status(),
        &LightingStatus::Unverified {
            problem: byakko_core::session::Problem::Apply(failure)
        }
    );
    assert!(app.session.request_lighting_apply().is_err());
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::ConfirmDiscard);
}

#[test]
fn verified_lighting_close_checks_its_result_despite_keymap_invalidation() {
    let mut app = loaded();
    send(&mut app, Lighting::Edit(Edit::Brightness(42)));
    let Command::ApplyLighting {
        generation,
        operation,
        mut expected,
        desired,
    } = app.session.request_lighting_apply().unwrap()
    else {
        unreachable!()
    };
    let _ = app.update(Message::Close);
    expected.content = Content::Editable(desired);
    let _ = app.complete(Completion::ApplyLighting {
        generation,
        operation,
        result: Ok(expected),
    });
    // complete() returned the exit task; it must not classify keymap ReadRequired as lighting failure.
    assert_eq!(app.closing, Closing::Waiting);
    assert!(!app.session.dirty());
    assert_eq!(
        app.session.lighting().unwrap().status(),
        &LightingStatus::Ready
    );
}

#[test]
fn failed_lighting_write_requires_manual_reconnect_and_keeps_diagnostic() {
    let mut app = loaded();
    send(&mut app, Lighting::Edit(Edit::Brightness(42)));
    let Command::ApplyLighting {
        generation,
        operation,
        ..
    } = app.session.request_lighting_apply().unwrap()
    else {
        unreachable!()
    };
    let _ = app.complete(Completion::ApplyLighting {
        generation,
        operation,
        result: Err(ApplyFailure {
            message: "lighting readback uncertain".into(),
            recovery: Recovery::Unverified,
        }),
    });
    app.accept_availability(Availability::Missing);
    assert_eq!(app.auto_read, AutoRead::ManualOnly);
    app.accept_availability(Availability::Ready {
        id: "demo-2".into(),
    });
    assert_eq!(app.session.status(), &Status::Disconnected);
    assert!(!app.session.busy());
    assert!(
        app.notice
            .as_deref()
            .is_some_and(|notice| notice.contains("lighting readback uncertain"))
    );
}

#[test]
fn per_key_mode_activates_in_lighting_and_effect_choice_returns_in_place() {
    let mut app = loaded();
    send(&mut app, Lighting::Panel(crate::lighting::Panel::PerKey));
    settle(&mut app);
    assert_eq!(app.page, Page::Lighting);
    assert_eq!(app.lighting_panel, crate::lighting::Panel::PerKey);
    assert_eq!(
        app.session.lighting().unwrap().draft().unwrap().effect,
        "per-key"
    );
    send(&mut app, Lighting::Live(Edit::Effect("sweep".into())));
    settle(&mut app);
    assert_eq!(app.page, Page::Lighting);
    assert_eq!(app.lighting_panel, crate::lighting::Panel::Onboard);
    assert_eq!(
        app.session.lighting().unwrap().draft().unwrap().effect,
        "sweep"
    );
}
