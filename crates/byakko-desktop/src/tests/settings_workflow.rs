use super::*;
use crate::settings::Message as Settings;
use byakko_core::settings::{Content, Edit, Value, editor::Status as SettingsStatus};
use macro_workflow::settle;
use std::time::{Duration, Instant};

#[test]
fn queued_settings_coalesce_latest_value_per_field_and_preserve_failure_intent() {
    let now = Instant::now();
    let mut pending = crate::settings::Pending::default();
    pending.queue(
        Edit {
            id: "repeat_delay".into(),
            value: Value::Number(10),
        },
        now,
        Duration::from_millis(50),
    );
    pending.queue(
        Edit {
            id: "repeat_delay".into(),
            value: Value::Number(12),
        },
        now,
        Duration::from_millis(50),
    );
    pending.queue(
        Edit {
            id: "studio_mode".into(),
            value: Value::Toggle(true),
        },
        now,
        Duration::from_millis(50),
    );
    assert!(!pending.ready(now));
    assert!(pending.ready(now + Duration::from_millis(50)));
    assert_eq!(
        pending.queued_value("repeat_delay"),
        Some(&Value::Number(12))
    );
    pending.begin(Edit {
        id: "repeat_delay".into(),
        value: Value::Number(12),
    });
    pending.reconcile(&SettingsStatus::Unverified {
        problem: byakko_core::session::Problem::ReadRequired,
    });
    assert!(pending.blocked());
    assert_eq!(
        pending.queued_value("repeat_delay"),
        Some(&Value::Number(12))
    );
    assert_eq!(
        pending.queued_value("studio_mode"),
        Some(&Value::Toggle(true))
    );
    pending.retry();
    assert!(pending.ready(now));
}

#[test]
fn live_settings_accept_multiple_fields_while_first_write_is_in_flight() {
    let mut app = loaded();
    send(
        &mut app,
        Settings::Live(Edit {
            id: "repeat_delay".into(),
            value: Value::Number(12),
        }),
    );
    assert!(app.busy());
    send(
        &mut app,
        Settings::Live(Edit {
            id: "studio_mode".into(),
            value: Value::Toggle(true),
        }),
    );
    assert_eq!(
        app.live_settings.queued_value("studio_mode"),
        Some(&Value::Toggle(true))
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.busy() || app.live_settings.has_pending() {
        assert!(Instant::now() < deadline, "settings write timed out");
        let _ = app.poll();
        std::thread::sleep(Duration::from_millis(2));
    }
    let values = app.session.settings().unwrap().draft().unwrap();
    assert_eq!(values["repeat_delay"], Value::Number(12));
    assert_eq!(values["studio_mode"], Value::Toggle(true));
    assert!(!app.live_settings.has_queued());
}

fn send(app: &mut Desktop, message: Settings) {
    let _ = app.update(Message::Settings(message));
}

fn loaded() -> Desktop {
    let mut app = ready();
    let _ = app.update(Message::Page(Page::Settings));
    send(&mut app, Settings::Read);
    settle(&mut app);
    assert_eq!(
        app.session.settings().unwrap().status(),
        &SettingsStatus::Ready
    );
    app
}

#[test]
fn settings_page_uses_explicit_read_and_keeps_verified_snapshot_across_navigation() {
    let mut app = ready();
    let _ = app.update(Message::Page(Page::Settings));
    assert!(!app.busy());
    send(&mut app, Settings::Read);
    assert!(matches!(
        app.session.activity(),
        byakko_core::session::Activity::ReadSettings { .. }
    ));
    settle(&mut app);
    assert_eq!(
        app.session.settings().unwrap().status(),
        &SettingsStatus::Ready
    );
    let _ = app.update(Message::Page(Page::Keys));
    let _ = app.update(Message::Page(Page::Settings));
    assert!(!app.busy());
    assert_eq!(
        app.session.settings().unwrap().status(),
        &SettingsStatus::Ready
    );
}

#[test]
fn generic_settings_stage_one_field_and_save_through_executor() {
    let mut app = loaded();
    let original = app.session.settings().unwrap().draft().unwrap().clone();
    send(
        &mut app,
        Settings::Edit(Edit {
            id: "repeat_delay".into(),
            value: Value::Number(12),
        }),
    );
    let draft = app.session.settings().unwrap().draft().unwrap().clone();
    assert_eq!(draft["repeat_delay"], Value::Number(12));
    send(
        &mut app,
        Settings::Edit(Edit {
            id: "studio_mode".into(),
            value: Value::Toggle(true),
        }),
    );
    assert!(app.notice.is_some());
    assert_eq!(app.session.settings().unwrap().draft(), Some(&draft));
    drop(app.view());
    send(&mut app, Settings::Apply);
    assert!(app.busy());
    settle(&mut app);
    assert_eq!(
        app.session.settings().unwrap().status(),
        &SettingsStatus::Ready
    );
    assert!(!app.session.settings().unwrap().dirty());
    send(&mut app, Settings::Read);
    settle(&mut app);
    assert_eq!(
        app.session.settings().unwrap().baseline().unwrap().content,
        Content::Editable(draft)
    );
    assert_ne!(app.session.settings().unwrap().draft(), Some(&original));
}

#[test]
fn failed_setting_apply_retains_draft_and_blocks_close() {
    let mut app = loaded();
    send(
        &mut app,
        Settings::Edit(Edit {
            id: "studio_mode".into(),
            value: Value::Toggle(true),
        }),
    );
    let baseline = app.session.settings().unwrap().baseline().cloned();
    let draft = app.session.settings().unwrap().draft().cloned();
    let Command::ApplySetting {
        generation,
        operation,
        ..
    } = app.session.request_setting_apply().unwrap()
    else {
        unreachable!()
    };
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    let _ = app.complete(Completion::ReadSettings {
        generation,
        operation,
        result: Ok(baseline.clone().unwrap()),
    });
    assert!(app.busy());
    let failure = ApplyFailure {
        message: "restore mismatch".into(),
        recovery: Recovery::Failed,
    };
    let _ = app.complete(Completion::ApplySetting {
        generation,
        operation,
        result: Err(failure.clone()),
    });
    assert_eq!(app.closing, Closing::Open);
    let editor = app.session.settings().unwrap();
    assert_eq!(editor.baseline(), baseline.as_ref());
    assert_eq!(editor.draft(), draft.as_ref());
    assert_eq!(
        editor.status(),
        &SettingsStatus::Unverified {
            problem: byakko_core::session::Problem::Apply(failure),
        }
    );
}
