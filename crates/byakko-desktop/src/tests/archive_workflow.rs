use super::*;
use crate::archive::{FileState, Message as Archive};
use byakko_core::archive::ArchiveState;
use macro_workflow::settle;

fn send(app: &mut Desktop, message: Archive) {
    let _ = app.update(Message::Archive(message));
}

#[test]
fn diagnostic_capture_is_explicit_and_path_changes_preserve_its_bytes() {
    let mut app = ready();
    let _ = app.update(Message::Page(Page::Archive));
    assert_eq!(app.session.archive(), Some(&ArchiveState::Idle));
    assert!(!app.busy());
    send(&mut app, Archive::Capture);
    assert!(app.busy());
    settle(&mut app);
    let Some(ArchiveState::Captured(before)) = app.session.archive().cloned() else {
        panic!("expected captured native archive");
    };
    assert!(!before.bytes.is_empty());
    send(&mut app, Archive::Path("diagnostic-capture.json".into()));
    assert_eq!(app.session.archive(), Some(&ArchiveState::Captured(before)));
    assert!(!app.busy());
    drop(app.view());
}

#[test]
fn export_requires_a_capture_without_starting_device_work() {
    let mut app = ready();
    send(&mut app, Archive::Path("diagnostic-capture.json".into()));
    send(&mut app, Archive::Export);
    assert_eq!(app.archive_file, FileState::Idle);
    assert_eq!(app.session.archive(), Some(&ArchiveState::Idle));
    assert!(!app.busy());
    assert_eq!(
        app.notice.as_deref(),
        Some("Capture the current configuration before exporting")
    );
}

#[test]
fn failed_export_keeps_close_open_and_reports_error() {
    let mut app = ready();
    let state = FileState::Exporting {
        generation: app.session.generation(),
    };
    app.archive_file = state;
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    send(
        &mut app,
        Archive::FileComplete(state, Err("write failed".into())),
    );
    assert_eq!(app.closing, Closing::Open);
    assert_eq!(app.archive_file, FileState::Idle);
    assert_eq!(app.notice.as_deref(), Some("write failed"));
}

#[test]
fn stale_export_completion_cannot_complete_a_new_export() {
    let mut app = ready();
    let state = FileState::Exporting {
        generation: app.session.generation(),
    };
    let stale = FileState::Exporting {
        generation: app.session.generation() - 1,
    };
    app.archive_file = state;
    send(&mut app, Archive::FileComplete(stale, Ok(())));
    assert_eq!(app.archive_file, state);
    assert!(app.notice.is_none());
}

#[test]
fn device_change_during_export_keeps_close_open() {
    let mut app = ready();
    let state = FileState::Exporting {
        generation: app.session.generation(),
    };
    app.archive_file = state;
    app.closing = Closing::Waiting;
    app.session.disconnect();
    app.session.connect().unwrap();
    send(&mut app, Archive::FileComplete(state, Ok(())));
    assert_eq!(app.archive_file, FileState::Idle);
    assert_eq!(app.closing, Closing::Open);
    assert_eq!(
        app.notice.as_deref(),
        Some("Device changed while saving the captured configuration")
    );
}
