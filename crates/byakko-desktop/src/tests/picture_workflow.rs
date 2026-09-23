use super::*;
use crate::picture::Message as Picture;
use byakko_core::picture::{Channel, Content, Edit, editor::Status as PictureStatus};
use macro_workflow::settle;

fn send(app: &mut Desktop, message: Picture) {
    let _ = app.update(Message::Picture(message));
}

fn loaded() -> Desktop {
    let mut app = ready();
    let _ = app.update(Message::Page(Page::Picture));
    send(&mut app, Picture::Read);
    settle(&mut app);
    assert_eq!(
        app.session.picture().unwrap().status(),
        &PictureStatus::Ready
    );
    app
}

#[test]
fn entering_colors_does_not_issue_reads_and_connection_preloads_once() {
    let mut app = ready();
    let _ = app.update(Message::Page(Page::Picture));
    assert_eq!(
        app.session.activity(),
        &byakko_core::session::Activity::Idle
    );
    send(&mut app, Picture::Read);
    settle(&mut app);
    assert_eq!(
        app.session.picture().unwrap().status(),
        &PictureStatus::Ready
    );
    let _ = app.update(Message::Page(Page::Keys));
    let _ = app.update(Message::Page(Page::Picture));
    assert!(!app.busy());

    let mut app = ready();
    let _ = app.update(Message::Read);
    assert!(matches!(
        app.session.activity(),
        byakko_core::session::Activity::Read { .. }
    ));
    let _ = app.update(Message::Page(Page::Picture));
    send(&mut app, Picture::Read);
    settle(&mut app);
    assert_eq!(
        app.session.picture().unwrap().status(),
        &PictureStatus::Ready
    );
}

#[test]
fn failed_color_read_waits_for_an_explicit_retry() {
    let mut app = ready();
    let _ = app.update(Message::Page(Page::Picture));
    send(&mut app, Picture::Read);
    let byakko_core::session::Activity::ReadPicture { operation } = app.session.activity() else {
        panic!("expected automatic color read");
    };
    let _ = app.complete(Completion::ReadPicture {
        generation: app.session.generation(),
        operation: *operation,
        result: Err("USB read failed".into()),
    });
    assert!(!app.busy());
    let _ = app.update(Message::Page(Page::Keys));
    let _ = app.update(Message::Page(Page::Picture));
    assert!(!app.busy());
    assert!(matches!(
        app.session.picture().unwrap().status(),
        PictureStatus::Unverified {
            problem: byakko_core::session::Problem::Read(_)
        }
    ));
}

#[test]
fn color_page_reconnect_refreshes_baseline_without_losing_draft() {
    let mut app = loaded();
    send(
        &mut app,
        Picture::Edit(Edit::Channel {
            key: "Alpha".into(),
            channel: Channel::Red,
            value: 99,
        }),
    );
    let draft = app.session.picture().unwrap().draft().cloned();
    app.accept_availability(Availability::Missing);
    assert_eq!(app.session.status(), &Status::Disconnected);
    assert_eq!(app.session.picture().unwrap().draft(), draft.as_ref());

    app.accept_availability(Availability::Ready {
        id: "demo-2".into(),
    });
    settle(&mut app);
    assert_eq!(app.session.status(), &Status::Ready);
    assert_eq!(
        app.session.picture().unwrap().status(),
        &PictureStatus::Ready
    );
    assert_eq!(app.session.picture().unwrap().draft(), draft.as_ref());
    assert!(app.session.picture().unwrap().dirty());
}

#[test]
fn per_key_draft_uses_advertised_keys_and_saves_through_memory_executor() {
    let mut app = loaded();
    let original = app.session.picture().unwrap().draft().unwrap().clone();
    send(&mut app, Picture::Select("Fixed".into()));
    assert_eq!(app.picture_selected.as_deref(), Some("Fixed"));
    send(
        &mut app,
        Picture::Edit(Edit::Channel {
            key: "Fixed".into(),
            channel: Channel::Blue,
            value: 77,
        }),
    );
    send(
        &mut app,
        Picture::Edit(Edit::Channel {
            key: "Alpha".into(),
            channel: Channel::Red,
            value: 99,
        }),
    );
    let draft = app.session.picture().unwrap().draft().unwrap().clone();
    assert_eq!(draft["Fixed"], [200, 10, 77]);
    assert_eq!(draft["Alpha"], [99, 34, 56]);
    assert!(app.session.changes().is_empty());
    drop(app.view());
    send(
        &mut app,
        Picture::Edit(Edit::Color {
            key: "unknown".into(),
            color: [1, 2, 3],
        }),
    );
    assert!(app.notice.is_some());
    assert_eq!(app.session.picture().unwrap().draft(), Some(&draft));
    send(&mut app, Picture::Apply);
    assert!(app.busy());
    send(&mut app, Picture::Revert);
    assert_eq!(app.session.picture().unwrap().draft(), Some(&draft));
    settle(&mut app);
    assert_eq!(
        app.session.picture().unwrap().status(),
        &PictureStatus::Ready
    );
    assert!(!app.session.picture().unwrap().dirty());
    send(&mut app, Picture::Read);
    settle(&mut app);
    assert_eq!(
        app.session.picture().unwrap().baseline().unwrap().content,
        Content::Editable(draft)
    );
    send(&mut app, Picture::Revert);
    assert_ne!(app.session.picture().unwrap().draft(), Some(&original));
}

#[test]
fn failed_picture_apply_keeps_draft_visible_and_close_waits() {
    let mut app = loaded();
    send(
        &mut app,
        Picture::Edit(Edit::Channel {
            key: "Alpha".into(),
            channel: Channel::Green,
            value: 90,
        }),
    );
    let baseline = app.session.picture().unwrap().baseline().cloned();
    let draft = app.session.picture().unwrap().draft().cloned();
    let Command::ApplyPicture {
        generation,
        operation,
        ..
    } = app.session.request_picture_apply().unwrap()
    else {
        unreachable!()
    };
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    let _ = app.complete(Completion::ReadPicture {
        generation,
        operation,
        result: Ok(baseline.clone().unwrap()),
    });
    assert!(app.busy());
    let failure = ApplyFailure {
        message: "restore mismatch".into(),
        recovery: Recovery::Failed,
    };
    let _ = app.complete(Completion::ApplyPicture {
        generation,
        operation,
        result: Err(failure.clone()),
    });
    assert_eq!(app.closing, Closing::Open);
    let editor = app.session.picture().unwrap();
    assert_eq!(editor.baseline(), baseline.as_ref());
    assert_eq!(editor.draft(), draft.as_ref());
    assert_eq!(
        editor.status(),
        &PictureStatus::Unverified {
            problem: byakko_core::session::Problem::Apply(failure),
        }
    );
    assert!(app.session.request_picture_apply().is_err());
}
