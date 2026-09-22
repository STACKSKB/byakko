use super::*;
use crate::settings::Message as Settings;
use byakko_core::settings::{Content, Edit, Value, editor::Status as SettingsStatus};
use macro_workflow::settle;

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
fn generic_settings_stage_one_field_and_save_through_executor() {
    let mut app = loaded();
    let original = app.session.settings().unwrap().draft().unwrap().clone();
    send(&mut app, Settings::Select("repeat_delay".into()));
    assert_eq!(app.settings_selected.as_deref(), Some("repeat_delay"));
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
