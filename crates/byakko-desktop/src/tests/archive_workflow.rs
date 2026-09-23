use super::*;
use crate::archive::{FileState, Message as Archive};
use byakko_core::archive::{ArchiveState, NativeArchive};
use macro_workflow::settle;

fn send(app: &mut Desktop, message: Archive) {
    let _ = app.update(Message::Archive(message));
}

#[test]
fn native_archive_capture_and_review_use_owned_memory_backend_values() {
    let mut app = ready();
    let _ = app.update(Message::Page(Page::Archive));
    send(&mut app, Archive::Capture);
    settle(&mut app);
    let ArchiveState::Captured(before) = app.session.archive().unwrap() else {
        panic!("expected captured native archive")
    };
    let before = before.clone();
    let mut target = before.clone();
    target.bytes.push(b' ');
    let request = app.session.request_archive_review(target.clone());
    app.submit(request);
    assert!(app.busy());
    settle(&mut app);
    let ArchiveState::Ready(review) = app.session.archive().unwrap() else {
        panic!("expected archive review")
    };
    assert_eq!(review.before, before);
    assert_eq!(review.target, target);
    assert!(!review.changes.is_empty());
    drop(app.view());
    send(&mut app, Archive::Path("another-file.json".into()));
    assert!(matches!(
        app.session.archive(),
        Some(ArchiveState::Captured(_))
    ));
    assert!(
        app.session
            .request_archive_review(NativeArchive {
                bytes: vec![0; 5000],
                ..target
            })
            .is_err()
    );
}

#[test]
fn failed_local_file_task_keeps_close_open_and_reports_error() {
    let mut app = ready();
    let state = FileState::Importing {
        generation: app.session.generation(),
    };
    app.archive_file = state;
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    send(
        &mut app,
        Archive::FileComplete(state, Err("read failed".into())),
    );
    assert_eq!(app.closing, Closing::Open);
    assert_eq!(app.archive_file, FileState::Idle);
    assert_eq!(app.notice.as_deref(), Some("read failed"));
}
