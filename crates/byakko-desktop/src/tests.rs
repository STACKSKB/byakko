use super::*;
use byakko_core::{
    Action,
    session::{ApplyFailure, Recovery},
};
use byakko_devices::KeymapDevice;

mod archive_workflow;
#[path = "demo.rs"]
mod demo;
mod lighting_workflow;
pub(crate) mod macro_workflow;
mod picture_workflow;
mod recording_workflow;
mod settings_workflow;

fn ready() -> Desktop {
    let mut device = demo::device().unwrap();
    let mut session = Session::new(device.descriptor().clone())
        .unwrap()
        .with_macros(device.macro_capabilities().unwrap().clone())
        .unwrap()
        .with_lighting(device.lighting_capabilities().unwrap().clone())
        .unwrap()
        .with_picture(device.picture_capabilities().unwrap().clone())
        .unwrap()
        .with_settings(device.settings_capabilities().unwrap().clone())
        .unwrap()
        .with_archive(device.archive_capabilities().unwrap())
        .unwrap();
    let generation = session.connect().unwrap();
    let Command::Read { operation, .. } = session.request_read().unwrap() else {
        unreachable!()
    };
    session.accept(Completion::Read {
        generation,
        operation,
        result: device.read(),
    });
    let executor = Executor::spawn(device, Default::default()).unwrap();
    executor.set_generation(generation);
    Desktop {
        ui: panels::UiStyle::DEFAULT,
        archive_file: archive::FileState::Idle,
        archive_path: String::new(),
        picture_selected: None,
        settings_selected: None,
        macro_files: Default::default(),
        clock: std::time::Instant::now(),
        recording_options: Default::default(),
        host: None,
        page: Page::Keys,
        macro_form: Default::default(),
        repeat_input: String::new(),
        session,
        executor: Some(executor),
        attach: Box::new(|_| {
            Executor::spawn(demo::device()?, Default::default()).map_err(|e| e.to_string())
        }),
        discovery: Discovery::spawn(|| Availability::Ready { id: "demo".into() }).unwrap(),
        presence: Some(Availability::Ready { id: "demo".into() }),
        selected_device: Some("demo".into()),
        auto_read: AutoRead::Enabled,
        layer: "Typing".into(),
        selected: Some("Alpha".into()),
        search: String::new(),
        notice: None,
        closing: Closing::Open,
    }
}

#[test]
fn automatic_reconnect_preserves_staged_draft_and_ignores_late_completion() {
    let mut app = ready();
    app.stage(1);
    let draft = app.session.draft().cloned();
    let baseline = app.session.baseline().cloned().unwrap();
    let old_generation = app.session.generation();
    app.accept_availability(Availability::Missing);
    assert_eq!(app.session.status(), &Status::Disconnected);
    assert_eq!(app.session.draft(), draft.as_ref());
    app.accept_availability(Availability::Ready {
        id: "demo-2".into(),
    });
    let byakko_core::session::Activity::Read { operation } = app.session.activity() else {
        panic!("automatic reconnect did not read");
    };
    let operation = *operation;
    assert_ne!(old_generation, app.session.generation());
    let _ = app.complete(Completion::Read {
        generation: old_generation,
        operation,
        result: Ok(baseline.clone()),
    });
    assert!(app.session.busy());
    let _ = app.complete(Completion::Read {
        generation: app.session.generation(),
        operation,
        result: Ok(baseline),
    });
    assert_eq!(app.session.status(), &Status::Ready);
    assert_eq!(app.session.draft(), draft.as_ref());
    assert_eq!(app.session.changes().len(), 1);
}

#[test]
fn changed_configuration_path_forces_new_read() {
    let mut app = ready();
    let attached = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = attached.clone();
    app.attach = Box::new(move |id| {
        observed.lock().unwrap().push(id.to_owned());
        Executor::spawn(demo::device()?, Default::default()).map_err(|e| e.to_string())
    });
    let generation = app.session.generation();
    app.accept_availability(Availability::Ready {
        id: "another-path".into(),
    });
    assert!(matches!(
        app.session.activity(),
        byakko_core::session::Activity::Read { .. }
    ));
    assert!(app.session.generation() > generation);
    assert_eq!(app.selected_device.as_deref(), Some("another-path"));
    assert_eq!(attached.lock().unwrap().as_slice(), ["another-path"]);
}

#[test]
fn ambiguous_and_enumeration_failure_block_automatic_reads() {
    let mut app = ready();
    app.accept_availability(Availability::Ambiguous { count: 2 });
    assert_eq!(app.session.status(), &Status::Disconnected);
    assert!(!app.session.busy());
    assert!(view::status(&app).contains("2 matching"));
    app.accept_availability(Availability::Error("permission denied".into()));
    assert!(!app.session.busy());
    assert!(view::status(&app).contains("permission denied"));
}

#[test]
fn failed_write_does_not_restart_automatically_after_reappearance() {
    let mut app = ready();
    app.stage(1);
    let Command::Apply {
        generation,
        operation,
        ..
    } = app.session.request_apply().unwrap()
    else {
        unreachable!()
    };
    let _ = app.complete(Completion::Apply {
        generation,
        operation,
        result: Err(ApplyFailure {
            message: "uncertain write".into(),
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
            .is_some_and(|message| message.contains("uncertain write"))
    );
}

#[test]
fn passive_discovery_tick_keeps_dirty_close_confirmation_open() {
    let mut app = ready();
    app.stage(1);
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::ConfirmDiscard);
    let _ = app.update(Message::Scan);
    assert_eq!(app.closing, Closing::ConfirmDiscard);
}

#[test]
fn stages_only_selected_generic_layer_and_preserves_fixed_opaque_action() {
    let mut app = ready();
    let _ = app.update(Message::SelectLayer("Studio".into()));
    let _ = app.update(Message::Stage(1));
    assert_eq!(
        app.session.changes(),
        vec![Change {
            layer: "Studio".into(),
            key: "Alpha".into(),
            action: Action::Key(5)
        }]
    );
    let draft = app.session.draft().unwrap().clone();
    let _ = app.update(Message::SelectKey("Fixed".into()));
    let _ = app.update(Message::Stage(2));
    assert_eq!(app.session.draft(), Some(&draft));
    assert!(app.notice.is_some());
    assert!(matches!(draft["Studio"]["Fixed"], Action::Opaque { .. }));
}

#[test]
fn close_waits_for_apply_and_keeps_failure_and_draft_visible() {
    let mut app = ready();
    app.stage(1);
    let Command::Apply {
        generation,
        operation,
        ..
    } = app.session.request_apply().unwrap()
    else {
        unreachable!()
    };
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    let _ = app.update(Message::DiscardAndClose);
    assert!(app.busy());
    let _ = app.complete(Completion::Apply {
        generation,
        operation,
        result: Err(ApplyFailure {
            message: "readback failed".into(),
            recovery: Recovery::Unverified,
        }),
    });
    assert_eq!(app.closing, Closing::Open);
    assert_eq!(app.session.changes().len(), 1);
    assert!(matches!(app.session.status(), Status::Unverified { .. }));
    assert!(app.session.request_apply().is_err());
}

#[test]
fn dirty_close_confirmation_is_dismissed_by_new_edit_and_clean_close_does_not_mutate() {
    let mut app = ready();
    app.stage(1);
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::ConfirmDiscard);
    let _ = app.update(Message::Revert);
    assert_eq!(app.closing, Closing::Open);
    assert!(app.session.changes().is_empty());
    let baseline = app.session.baseline().cloned();
    let _ = app.update(Message::Close);
    assert_eq!(app.session.baseline(), baseline.as_ref());
}
