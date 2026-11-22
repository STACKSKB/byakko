use super::notifications::{click_text, rendered_text};
use super::*;

fn choices() -> Availability {
    Availability::Choices {
        wired: test_endpoint("wired"),
        receiver: test_endpoint("receiver"),
    }
}

#[test]
fn delayed_attachment_leaves_navigation_responsive() {
    use std::sync::mpsc;

    let (started, starts) = mpsc::channel();
    let (release, releases) = mpsc::channel();
    let releases = Mutex::new(releases);
    let mut app = App::new(
        memory::demo().unwrap().session().unwrap(),
        Box::new(move |expected| {
            assert_eq!(expected, Some("receiver"));
            started.send(()).unwrap();
            // A watchdog catches a regression that blocks the update thread.
            releases
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .expect("update thread did not release attachment");
            let worker = Executor::spawn(memory::demo()?, Default::default())
                .map_err(|error| error.to_string())?;
            Ok(("receiver".into(), worker))
        }),
    );
    app.link.monitor(Discovery::spawn(choices).unwrap());
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.link.presence() != &choices() {
        let _ = app.update(Message::Scan);
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }

    let _ = app.update(Message::ChooseTransport("receiver".into()));
    starts.recv_timeout(Duration::from_secs(2)).unwrap();
    let _ = app.update(Message::Page(Page::Settings));
    assert_eq!(app.page, Page::Settings);
    assert!(
        app.link.settling(),
        "navigation must run before attach finishes"
    );
    assert!(app.link.executor().is_none());
    assert!(click_text(&app, "Read / reconnect").is_empty());
    assert!(click_text(&app, "Wired USB").is_empty());
    let _ = app.update(Message::Read);
    let _ = app.update(Message::ChooseTransport("wired".into()));
    assert_eq!(app.link.selected(), Some("receiver"));
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);

    release.send(()).unwrap();
    drain(&mut app);
    assert!(matches!(
        app.session.connection(),
        Connection::Connected { generation: 1 }
    ));
    assert_eq!(app.session.keymap().status(), &Status::Ready);
    assert!(starts.try_recv().is_err(), "only one attachment may start");
}

#[test]
fn a_failed_reconnect_resumes_close_and_confirms_discard_only_when_dirty() {
    use iced::futures::{FutureExt, StreamExt};
    use iced_runtime::{Action, task::into_stream};

    for dirty in [false, true] {
        let mut h = DiscoveryApp::new(false);
        h.observe(ready("wired"));
        if dirty {
            edit(&mut h.app);
        }
        let draft = h.app.session.keymap().draft().cloned();
        h.reject_attach.store(true, Ordering::SeqCst);
        let _ = h.app.update(Message::Read);
        let _ = h.app.update(Message::Close);
        assert_eq!(h.app.closing, Closing::Waiting);

        let deadline = Instant::now() + Duration::from_secs(2);
        let task = loop {
            let task = h.app.update(Message::Poll(Instant::now()));
            if !h.app.link.settling() {
                break task;
            }
            assert!(
                Instant::now() < deadline,
                "failed attachment did not settle"
            );
            std::thread::yield_now();
        };
        assert_eq!(h.app.notice, "Injected attach failure");
        assert_eq!(h.app.session.connection(), &Connection::Disconnected);
        let action =
            into_stream(task).and_then(|mut actions| actions.next().now_or_never().flatten());
        if dirty {
            assert_eq!(h.app.closing, Closing::ConfirmDiscard);
            assert!(action.is_none());
            assert_eq!(h.app.session.keymap().draft(), draft.as_ref());
        } else {
            assert!(matches!(action, Some(Action::Exit)));
        }
    }
}

#[test]
fn dual_transport_startup_requires_a_choice_and_pins_either_endpoint() {
    for id in ["wired", "receiver"] {
        let mut h = DiscoveryApp::new(false);
        h.observe(choices());
        assert_eq!(h.app.link.selected(), None);
        assert!(h.attachments.lock().unwrap().is_empty());

        let _ = h.app.update(Message::ChooseTransport("other".into()));
        let _ = h.app.update(Message::Read);
        assert!(h.attachments.lock().unwrap().is_empty());

        let label = test_endpoint(id).label;
        let messages = click_text(&h.app, label);
        assert!(matches!(messages.as_slice(), [Message::ChooseTransport(chosen)] if chosen == id));
        for message in messages {
            let _ = h.app.update(message);
        }
        drain(&mut h.app);
        assert_eq!(h.app.link.selected(), Some(id));
        assert_eq!(h.app.session.keymap().status(), &Status::Ready);
        assert_eq!(*h.attachments.lock().unwrap(), [Some(id.into())]);
        assert!(h.writes.lock().unwrap().is_empty());
        assert!(rendered_text(&h.app).contains(&format!("{label} (selected)")));
    }
}

#[test]
fn manual_inventory_refresh_finds_an_alternative_without_reattaching() {
    let mut h = DiscoveryApp::new(false);
    h.observe(ready("wired"));
    let generation = h.app.session.connection().clone();

    *h.presence.lock().unwrap() = choices();
    let messages = click_text(&h.app, "Find connections");
    assert!(matches!(messages.as_slice(), [Message::RefreshTransports]));
    for message in messages {
        let _ = h.app.update(message);
    }
    h.wait_for(|app| app.link.presence() == &choices() && !app.link.scan_requested());

    assert_eq!(h.app.link.selected(), Some("wired"));
    assert_eq!(h.app.session.connection(), &generation);
    assert_eq!(*h.attachments.lock().unwrap(), [Some("wired".into())]);
    assert_eq!(h.reads.load(Ordering::SeqCst), 1);
}

#[test]
fn both_replacement_endpoints_are_offered_when_the_pinned_endpoint_is_gone() {
    let mut h = DiscoveryApp::new(false);
    h.observe(ready("old-wired"));
    h.observe(choices());

    assert_eq!(h.app.link.selected(), Some("old-wired"));
    assert_eq!(h.app.session.connection(), &Connection::Disconnected);
    let rendered = rendered_text(&h.app);
    assert!(rendered.contains(&"Wired USB".into()));
    assert!(rendered.contains(&"2.4 GHz receiver".into()));
    assert!(
        rendered
            .iter()
            .any(|text| text == "Selected connection unavailable. Choose a connection to switch.")
    );
}

#[test]
fn a_transport_choice_waits_for_a_submitted_write() {
    let mut h = DiscoveryApp::new(false);
    h.observe(choices());
    let _ = h.app.update(Message::ChooseTransport("wired".into()));
    drain(&mut h.app);
    edit(&mut h.app);
    let _ = h.app.update(Message::Save);
    assert!(h.app.session.busy());

    let _ = h.app.update(Message::ChooseTransport("receiver".into()));
    assert_eq!(h.app.link.selected(), Some("wired"));
    assert!(h.app.transport_switch.is_none());
    drain(&mut h.app);
    assert_eq!(*h.writes.lock().unwrap(), ["wired"]);

    let _ = h.app.update(Message::ChooseTransport("receiver".into()));
    drain(&mut h.app);
    assert_eq!(h.app.link.selected(), Some("receiver"));
    assert_eq!(*h.writes.lock().unwrap(), ["wired"]);
}

#[test]
fn switching_after_loss_requires_confirmation_and_disposes_of_the_old_draft() {
    let mut h = DiscoveryApp::new(false);
    h.observe(ready("wired"));
    let generation = h.app.session.connection().clone();
    edit(&mut h.app);
    h.observe(ready("receiver"));
    assert_eq!(h.app.session.connection(), &Connection::Disconnected);
    assert_eq!(h.app.link.selected(), Some("wired"));

    let _ = h.app.update(Message::ChooseTransport("receiver".into()));
    assert_eq!(h.app.transport_switch.as_deref(), Some("receiver"));
    assert!(h.app.session.keymap().dirty());
    let _ = h.app.update(Message::CancelTransportSwitch);
    assert!(h.app.transport_switch.is_none());
    assert!(h.app.session.keymap().dirty());
    assert_eq!(*h.attachments.lock().unwrap(), [Some("wired".into())]);

    let _ = h.app.update(Message::ChooseTransport("receiver".into()));
    let _ = h.app.update(Message::ConfirmTransportSwitch);
    drain(&mut h.app);
    assert_eq!(h.app.link.selected(), Some("receiver"));
    assert_ne!(h.app.session.connection(), &generation);
    assert!(!h.app.session.keymap().dirty());
    assert!(h.app.keys.selected.is_none());
    assert_eq!(h.app.session.keymap().status(), &Status::Ready);

    let _ = h.app.update(Message::Save);
    drain(&mut h.app);
    assert!(h.writes.lock().unwrap().is_empty());

    edit(&mut h.app);
    let _ = h.app.update(Message::Save);
    drain(&mut h.app);
    assert_eq!(*h.writes.lock().unwrap(), ["receiver"]);
}

#[test]
fn switch_discards_deferred_lighting_write_only_after_confirmation() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = calls.clone();
    let session = memory::demo().unwrap().session().unwrap();
    let mut app = App::new(
        session,
        Box::new(move |expected| {
            let id = expected.ok_or("Pick a connection")?.to_owned();
            let worker = Executor::spawn(
                ObservedDevice {
                    memory: memory::demo()?,
                    reads: Default::default(),
                    fail_save: false,
                    macro_reads: Default::default(),
                    feature_calls: observed.clone(),
                    fail_lighting: false,
                },
                Default::default(),
            )
            .map_err(|error| error.to_string())?;
            Ok((id, worker))
        }),
    );
    app.config.short_edit_delay = Duration::from_secs(60);
    app.link.monitor(Discovery::spawn(choices).unwrap());
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.link.presence() != &choices() {
        let _ = app.update(Message::Scan);
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let _ = app.update(Message::ChooseTransport("wired".into()));
    drain(&mut app);

    let _ = app.update(Message::Lighting(lighting::Message::Edit(
        byakko_core::model::lighting::Edit::Brightness(1),
    )));
    assert!(app.session.lighting().unwrap().dirty());
    assert!(app.autosave.pending());
    let _ = app.update(Message::ChooseTransport("receiver".into()));
    elapsed(&mut app);
    assert_eq!(app.transport_switch.as_deref(), Some("receiver"));
    assert_eq!(app.link.selected(), Some("wired"));
    assert!(app.autosave.pending());
    assert!(!calls.lock().unwrap().contains(&"apply-lighting"));

    let _ = app.update(Message::ConfirmTransportSwitch);
    drain(&mut app);
    elapsed(&mut app);
    assert_eq!(app.link.selected(), Some("receiver"));
    assert!(!app.autosave.pending());
    assert!(!app.session.lighting().unwrap().dirty());
    assert!(!calls.lock().unwrap().contains(&"apply-lighting"));
}

#[cfg(target_os = "linux")]
#[test]
fn permission_setup_retries_the_failed_selected_receiver() {
    let mut h = DiscoveryApp::new(false);
    h.observe(choices());
    h.reject_attach.store(true, Ordering::SeqCst);
    let _ = h.app.update(Message::ChooseTransport("receiver".into()));
    drain(&mut h.app);
    assert_eq!(h.app.notice, "Injected attach failure");
    h.reject_attach.store(false, Ordering::SeqCst);
    let _ = h
        .app
        .update(Message::KeyboardAccessFinished(Ok("Installed".into())));
    drain(&mut h.app);
    assert_eq!(h.app.link.selected(), Some("receiver"));
    assert_eq!(h.app.session.keymap().status(), &Status::Ready);
}
