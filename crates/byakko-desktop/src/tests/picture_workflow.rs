use super::*;
use crate::picture::Message as Picture;
use byakko_core::picture::{Channel, Edit, editor::Status as PictureStatus};
use byakko_core::session::CompletionPayload;
use byakko_core::session::FeatureResult;
use byakko_core::session::{DeviceActivity, Feature};
use macro_workflow::settle;

fn send(app: &mut Desktop, message: Picture) {
    let _ = app.update(Message::Picture(message));
}

fn start_picture_read(app: &mut Desktop) {
    let request = app.session.request_picture_read();
    app.submit(request);
}

fn loaded() -> Desktop {
    let mut app = ready();
    let _ = app.update(Message::Page(Page::Picture));
    start_picture_read(&mut app);
    settle(&mut app);
    assert_eq!(
        app.session.picture().unwrap().status(),
        &PictureStatus::Ready
    );
    app
}

fn settle_live(app: &mut Desktop) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while app.busy() || app.live_picture.has_pending() {
        assert!(
            std::time::Instant::now() < deadline,
            "live picture worker timed out"
        );
        let _ = app.poll();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn brush_paints_new_keys_and_batches_until_the_configured_gap() {
    let mut app = loaded();
    app.config.auto_save_delay = Duration::from_secs(60);
    send(
        &mut app,
        Picture::Live(Edit::Color {
            key: "Alpha".into(),
            color: [9, 80, 170],
        }),
    );
    send(&mut app, Picture::Select("Fixed".into()));
    assert_eq!(app.brush_color, Some([9, 80, 170]));
    let shown = crate::picture::projected_colors(&app).unwrap();
    assert_eq!(shown["Alpha"], [9, 80, 170]);
    assert_eq!(shown["Fixed"], [9, 80, 170]);
    assert!(!app.busy());
    assert!(!app.flush_live_picture());
    app.config.auto_save_delay = Duration::ZERO;
    send(&mut app, Picture::Select("Fixed".into()));
    settle_live(&mut app);
    assert_eq!(
        app.session.picture().unwrap().draft().unwrap()["Alpha"],
        [9, 80, 170]
    );
    assert_eq!(
        app.session.picture().unwrap().draft().unwrap()["Fixed"],
        [9, 80, 170]
    );
}

fn wait_for_live_write(app: &mut Desktop) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !matches!(
        app.session.activity(),
        byakko_core::session::Activity::Device {
            request: DeviceActivity::Apply(Feature::Picture),
            ..
        }
    ) {
        assert!(
            std::time::Instant::now() < deadline,
            "live picture write did not start"
        );
        let _ = app.poll();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn live_color_edits_coalesce_while_a_write_is_in_flight() {
    let mut app = loaded();
    send(
        &mut app,
        Picture::Live(Edit::Color {
            key: "Alpha".into(),
            color: [10, 20, 30],
        }),
    );
    wait_for_live_write(&mut app);
    send(
        &mut app,
        Picture::Live(Edit::Color {
            key: "Alpha".into(),
            color: [11, 21, 31],
        }),
    );
    assert_eq!(
        crate::picture::projected_colors(&app).unwrap()["Alpha"],
        [11, 21, 31]
    );
    settle_live(&mut app);
    assert_eq!(
        app.session.picture().unwrap().draft().unwrap()["Alpha"],
        [11, 21, 31]
    );
    assert!(!app.session.picture().unwrap().dirty());
    assert!(!app.live_picture.has_pending());
    assert_eq!(app.session.status(), &Status::Ready);
}

#[test]
fn live_color_refreshes_a_stale_picture_without_a_manual_read() {
    let mut app = loaded();
    let _ = app.update(Message::Lighting(crate::lighting::Message::Live(
        byakko_core::lighting::Edit::Brightness(20),
    )));
    macro_workflow::settle(&mut app);
    send(
        &mut app,
        Picture::Live(Edit::Color {
            key: "Alpha".into(),
            color: [90, 12, 43],
        }),
    );
    settle_live(&mut app);
    assert_eq!(
        app.session.picture().unwrap().draft().unwrap()["Alpha"],
        [90, 12, 43]
    );
}

#[test]
fn live_channel_edit_submits_immediately_and_replaces_only_its_channel() {
    let mut app = loaded();
    send(
        &mut app,
        Picture::Live(Edit::Channel {
            key: "Alpha".into(),
            channel: Channel::Red,
            value: 90,
        }),
    );
    send(
        &mut app,
        Picture::Live(Edit::Channel {
            key: "Alpha".into(),
            channel: Channel::Green,
            value: 91,
        }),
    );
    assert!(app.busy());
    assert_eq!(
        crate::picture::projected_colors(&app).unwrap()["Alpha"],
        [90, 91, 56]
    );
    settle_live(&mut app);
    assert_eq!(
        app.session.picture().unwrap().draft().unwrap()["Alpha"],
        [90, 91, 56]
    );
}

#[test]
fn failed_live_write_keeps_intent_without_automatic_retry() {
    let mut app = loaded();
    send(
        &mut app,
        Picture::Live(Edit::Color {
            key: "Alpha".into(),
            color: [8, 9, 10],
        }),
    );
    wait_for_live_write(&mut app);
    let byakko_core::session::Activity::Device {
        operation,
        request: DeviceActivity::Apply(Feature::Picture),
    } = app.session.activity()
    else {
        panic!("expected picture apply")
    };
    let operation = *operation;
    let _ = app.complete(Completion {
        generation: app.session.generation(),
        operation,
        payload: CompletionPayload::Picture(FeatureResult::Apply(Err(ApplyFailure {
            message: "restore mismatch".into(),
            recovery: Recovery::Failed,
        }))),
    });
    assert!(!app.flush_live_picture());
    assert!(app.live_picture.blocked);
    assert!(!app.live_picture.has_pending());
    assert_eq!(
        crate::picture::projected_colors(&app).unwrap()["Alpha"],
        [8, 9, 10]
    );
    assert!(matches!(
        app.session.picture().unwrap().status(),
        PictureStatus::Unverified {
            problem: byakko_core::session::Problem::Apply(_)
        }
    ));
}

#[test]
fn entering_colors_does_not_issue_reads_and_connection_preloads_once() {
    let mut app = ready();
    let _ = app.update(Message::Page(Page::Picture));
    assert_eq!(
        app.session.activity(),
        &byakko_core::session::Activity::Idle
    );
    start_picture_read(&mut app);
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
        byakko_core::session::Activity::Device {
            request: DeviceActivity::Read(Feature::Keymap),
            ..
        }
    ));
    settle(&mut app);
    let _ = app.update(Message::Page(Page::Picture));
    start_picture_read(&mut app);
    settle(&mut app);
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
    start_picture_read(&mut app);
    let byakko_core::session::Activity::Device {
        operation,
        request: DeviceActivity::Read(Feature::Picture),
    } = app.session.activity()
    else {
        panic!("expected automatic color read");
    };
    let _ = app.complete(Completion {
        generation: app.session.generation(),
        operation: *operation,
        payload: CompletionPayload::Picture(FeatureResult::Read(Err("USB read failed".into()))),
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
fn picker_activity_defers_existing_paint_and_drag_blocks_sending() {
    use crate::color_picker::Interaction;
    use crate::lighting::Message as Lighting;
    let mut app = loaded();
    let activity = |app: &mut Desktop, event| {
        let _ = app.update(Message::Lighting(Lighting::PickerInteraction(event)));
    };
    activity(&mut app, Interaction::Started);
    send(
        &mut app,
        Picture::Live(Edit::Color {
            key: "Alpha".into(),
            color: [12, 90, 180],
        }),
    );
    // Even an already-due edit cannot send while a picker drag is held.
    assert!(!app.busy());
    assert!(!app.flush_live_picture());
    app.config.auto_save_delay = Duration::from_secs(2);
    activity(&mut app, Interaction::Finished);
    assert!(!app.flush_live_picture());
    // Movement alone renews the deadline without replacing the queued paint.
    app.live_picture
        .postpone(std::time::Instant::now(), Duration::ZERO);
    activity(&mut app, Interaction::Moved);
    assert!(!app.flush_live_picture());
    assert_eq!(
        crate::picture::projected_colors(&app).unwrap()["Alpha"],
        [12, 90, 180]
    );
    app.config.auto_save_delay = Duration::ZERO;
    activity(&mut app, Interaction::Finished);
    settle_live(&mut app);
    assert_eq!(
        app.session.picture().unwrap().draft().unwrap()["Alpha"],
        [12, 90, 180]
    );
}
