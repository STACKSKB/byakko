use super::*;
use byakko_core::macros::{Action, Event, Program};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_LABEL_TEST: AtomicU64 = AtomicU64::new(0);

fn label_directory() -> PathBuf {
    std::env::temp_dir().join(format!(
        "byakko-desktop-label-test-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT_LABEL_TEST.fetch_add(1, Ordering::Relaxed),
    ))
}

fn document() -> Document {
    Document {
        format_version: 2,
        backend_id: "nia87".into(),
        source_slot: "slot-49".into(),
        name: "Imported name".into(),
        binding: Some("hold".into()),
        program: Program {
            repeat_count: 0,
            events: vec![Event {
                action: Action::Key {
                    usage: 5,
                    pressed: false,
                },
                delay_ms: 0,
            }],
        },
    }
}

#[test]
fn import_stages_only_selected_slot_and_roundtrips_metadata_without_binding() {
    let mut app = crate::tests::macro_workflow::loaded();
    let baseline = app.session.macros().unwrap().baseline().cloned();
    let ticket = app.session.begin_macro_file(FileOperation::Import).unwrap();
    let imported = document();
    let _ = app.update_macro_files(Message::Complete(
        ticket,
        Ok(Some(Box::new(imported.clone()))),
    ));
    let editor = app.session.macros().unwrap();
    assert_eq!(editor.slot(), "intro");
    assert_eq!(editor.draft(), Some(&imported.program));
    assert_eq!(editor.baseline(), baseline.as_ref());
    assert!(editor.dirty());
    assert!(app.session.changes().is_empty());
    let output = app.macro_files.document(editor).unwrap();
    assert_eq!(output.backend_id, "memory");
    assert_eq!(output.source_slot, "intro");
    assert_eq!(output.name, imported.name);
    assert_eq!(
        output.binding, None,
        "backend-local binding IDs cannot migrate implicitly"
    );
    assert!(
        app.macro_notice
            .as_ref()
            .unwrap()
            .contains("source binding")
    );
    assert_eq!(
        macro_files::decode(&macro_files::encode(&output).unwrap()).unwrap(),
        output
    );
    let mut compatible = imported;
    compatible.backend_id = "memory".into();
    let ticket = app.session.begin_macro_file(FileOperation::Import).unwrap();
    let _ = app.update_macro_files(Message::Complete(ticket, Ok(Some(Box::new(compatible)))));
    let output = app
        .macro_files
        .document(app.session.macros().unwrap())
        .unwrap();
    assert_eq!(output.binding.as_deref(), Some("hold"));
    assert!(
        app.session.changes().is_empty(),
        "metadata never stages a binding"
    );
}

#[test]
fn failed_and_stale_imports_cannot_replace_program_or_metadata_or_close_the_window() {
    let mut app = crate::tests::macro_workflow::loaded();
    let before = app.session.macros().unwrap().draft().cloned();
    let ticket = app.session.begin_macro_file(FileOperation::Import).unwrap();
    let _ = app.update(AppMessage::Close);
    assert_eq!(app.closing, Closing::Waiting);
    let mut wrong = ticket.clone();
    wrong.operation += 1;
    let _ = app.update_macro_files(Message::Complete(wrong, Ok(Some(Box::new(document())))));
    assert!(app.busy());
    assert!(app.macro_files.metadata.is_empty());
    let mut invalid = document();
    invalid.program.events[0].delay_ms = 2001;
    let _ = app.update_macro_files(Message::Complete(ticket, Ok(Some(Box::new(invalid)))));
    assert_eq!(app.closing, Closing::Open);
    assert_eq!(app.session.macros().unwrap().draft(), before.as_ref());
    assert_eq!(app.session.macros().unwrap().status(), &Status::Ready);
    assert!(app.macro_files.metadata.is_empty());
    assert!(app.macro_notice.is_some());
    let ticket = app.session.begin_macro_file(FileOperation::Import).unwrap();
    let _ = app.update_macro_files(Message::Complete(ticket, Err("Invalid JSON".into())));
    assert!(!app.busy());
    assert_eq!(app.session.macros().unwrap().draft(), before.as_ref());
}

#[test]
fn export_keeps_unverified_state_and_close_waits_for_its_result() {
    let mut app = crate::tests::macro_workflow::loaded();
    app.session.disconnect();
    let trust = app.session.macros().unwrap().status().clone();
    let ticket = app.session.begin_macro_file(FileOperation::Export).unwrap();
    let _ = app.update(AppMessage::Close);
    assert_eq!(app.closing, Closing::Waiting);
    let close = app.update_macro_files(Message::Complete(ticket, Ok(None)));
    assert_eq!(close.units(), 1);
    assert_eq!(app.session.macros().unwrap().status(), &trust);
}

#[test]
fn labels_save_locally_and_load_on_next_attach_without_device_write() {
    let directory = label_directory();
    let mut app = crate::tests::macro_workflow::loaded();
    app.macro_files = Fields::with_labels_directory(Some(directory.clone()));
    app.macro_files
        .load_labels(app.session.macros().unwrap())
        .unwrap();
    let before_generation = app.session.generation();
    let before_baseline = app.session.baseline().cloned();
    let before_activity = app.session.activity().clone();
    let _ = app.update_macro_files(Message::Name("Desk macro".into()));
    assert!(app.macro_files.labels_dirty(app.session.macros().unwrap()));
    let _ = app.update_macro_files(Message::SaveLabels);
    assert_eq!(app.session.generation(), before_generation);
    assert_eq!(app.session.activity(), &before_activity);
    assert_eq!(app.session.baseline(), before_baseline.as_ref());
    assert!(!app.macro_files.labels_dirty(app.session.macros().unwrap()));
    assert!(
        app.macro_notice
            .as_deref()
            .unwrap()
            .contains("Local labels saved")
    );

    let mut next = crate::tests::macro_workflow::loaded();
    next.macro_files = Fields::with_labels_directory(Some(directory));
    next.accept_availability(crate::discovery::Availability::Missing);
    next.accept_availability(crate::discovery::Availability::Ready { id: "demo".into() });
    assert_eq!(
        next.macro_files.name(next.session.macros().unwrap()),
        "Desk macro"
    );
}

#[test]
fn corrupt_newest_label_snapshot_is_visible_on_attach() {
    let directory = label_directory();
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("label-00000000000000000001.json"), b"{").unwrap();
    let mut app = crate::tests::macro_workflow::loaded();
    app.macro_files = Fields::with_labels_directory(Some(directory));
    app.accept_availability(crate::discovery::Availability::Missing);
    app.accept_availability(crate::discovery::Availability::Ready { id: "demo".into() });
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("Local labels could not be loaded")
    );
    assert_eq!(
        app.macro_files.name(app.session.macros().unwrap()),
        "Intro sequence"
    );
}
