use super::*;
use crate::view::application::picture_is_displayed;
use byakko_core::{
    contract::{ApplyFailure, Recovery},
    model::keymap::{Action, Change, State},
};
use byakko_devices::{
    Device,
    memory::{self, MemoryDevice},
};
use iced::Event;
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

struct ObservedDevice {
    memory: MemoryDevice,
    reads: Arc<AtomicUsize>,
    fail_save: bool,
    macro_reads: Arc<AtomicUsize>,
    feature_calls: Arc<Mutex<Vec<&'static str>>>,
    fail_lighting: bool,
}
impl Device for ObservedDevice {
    fn capture_archive(&mut self) -> Result<byakko_core::model::archive::NativeArchive, String> {
        self.feature_calls.lock().unwrap().push("capture-archive");
        self.memory.capture_archive()
    }
    fn read_lighting(&mut self) -> Result<byakko_core::model::lighting::Snapshot, String> {
        self.feature_calls.lock().unwrap().push("read-lighting");
        self.memory.read_lighting()
    }
    fn apply_lighting(
        &mut self,
        expected: &byakko_core::model::lighting::Snapshot,
        desired: &byakko_core::model::lighting::Setting,
        backup: &Path,
    ) -> Result<byakko_core::model::lighting::Snapshot, ApplyFailure> {
        self.feature_calls.lock().unwrap().push("apply-lighting");
        if self.fail_lighting {
            return Err(ApplyFailure {
                message: "Injected lighting failure".into(),
                recovery: Recovery::Verified,
            });
        }
        self.memory.apply_lighting(expected, desired, backup)
    }
    fn read_picture(&mut self) -> Result<byakko_core::model::picture::Snapshot, String> {
        self.feature_calls.lock().unwrap().push("read-picture");
        self.memory.read_picture()
    }
    fn apply_picture(
        &mut self,
        expected: &byakko_core::model::picture::Snapshot,
        desired: &std::collections::BTreeMap<String, [u8; 3]>,
        backup: &Path,
    ) -> Result<byakko_core::model::picture::Snapshot, ApplyFailure> {
        self.feature_calls.lock().unwrap().push("apply-picture");
        self.memory.apply_picture(expected, desired, backup)
    }
    fn read_settings(&mut self) -> Result<byakko_core::model::settings::Snapshot, String> {
        self.feature_calls.lock().unwrap().push("read-settings");
        self.memory.read_settings()
    }
    fn apply_setting(
        &mut self,
        expected: &byakko_core::model::settings::Snapshot,
        edit: &byakko_core::model::settings::Edit,
        backup: &Path,
    ) -> Result<byakko_core::model::settings::Snapshot, ApplyFailure> {
        self.feature_calls.lock().unwrap().push("apply-settings");
        self.memory.apply_setting(expected, edit, backup)
    }
    fn read_macro(&mut self, slot: &str) -> Result<byakko_core::model::macros::Snapshot, String> {
        self.macro_reads.fetch_add(1, Ordering::SeqCst);
        self.memory.read_macro(slot)
    }
    fn apply_macro(
        &mut self,
        expected: &byakko_core::model::macros::Snapshot,
        desired: &byakko_core::model::macros::Program,
        backup: &Path,
    ) -> Result<byakko_core::model::macros::Snapshot, ApplyFailure> {
        self.memory.apply_macro(expected, desired, backup)
    }
    fn read(&mut self) -> Result<State, String> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.memory.read()
    }
    fn apply(
        &mut self,
        expected: &State,
        changes: &[Change],
        backup: &Path,
    ) -> Result<State, ApplyFailure> {
        if self.fail_save {
            return Err(ApplyFailure {
                message: "Injected save failure".into(),
                recovery: Recovery::Verified,
            });
        }
        self.memory.apply(expected, changes, backup)
    }
}

fn app(fail_save: bool) -> (App, Arc<AtomicUsize>) {
    let reads = Arc::new(AtomicUsize::new(0));
    let count = reads.clone();
    let session = Session::new(memory::demo().unwrap().descriptor().clone()).unwrap();
    let app = App::new(
        session,
        Box::new(move |selected| {
            assert!(selected.is_none_or(|id| id == "demo"));
            let worker = Executor::spawn(
                ObservedDevice {
                    memory: memory::demo()?,
                    reads: count.clone(),
                    fail_save,
                    macro_reads: Default::default(),
                    feature_calls: Default::default(),
                    fail_lighting: false,
                },
                Default::default(),
            )
            .map_err(|e| e.to_string())?;
            Ok(("demo".into(), worker))
        }),
    );
    (app, reads)
}

fn macro_app(fail_assignment: bool) -> (App, Arc<AtomicUsize>) {
    let macro_reads = Arc::new(AtomicUsize::new(0));
    let count = macro_reads.clone();
    let device = memory::demo().unwrap();
    let session = Session::new(device.descriptor().clone())
        .unwrap()
        .with_macros(device.macro_capabilities().unwrap().clone())
        .unwrap();
    (
        App::new(
            session,
            Box::new(move |_| {
                let device = ObservedDevice {
                    memory: memory::demo()?,
                    reads: Default::default(),
                    macro_reads: count.clone(),
                    fail_save: fail_assignment,
                    feature_calls: Default::default(),
                    fail_lighting: false,
                };
                Ok((
                    "demo".into(),
                    Executor::spawn(device, Default::default())
                        .map_err(|error| error.to_string())?,
                ))
            }),
        ),
        macro_reads,
    )
}

fn drain(app: &mut App) {
    while app.session.busy() || app.session.catalog_scanning() {
        settle(app);
    }
}

#[test]
fn macro_messages_discover_read_candidate_edit_and_assign_once() {
    let (mut app, reads) = macro_app(false);
    let _ = app.update(Message::Read);
    drain(&mut app);
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    let _ = app.update(Message::Page(Page::Macros));
    let _ = app.update(Message::Page(Page::Keys));
    assert_eq!(
        reads.load(Ordering::SeqCst),
        3,
        "navigation does not read again"
    );
    let _ = app.update(Message::Macros(macros::Message::Add));
    drain(&mut app);
    assert_eq!(app.session.macros().unwrap().slot(), "Spare");
    assert_eq!(
        reads.load(Ordering::SeqCst),
        4,
        "Add reads its candidate once"
    );
    let _ = app.update(Message::Macros(macros::Message::Repeat("1".into())));
    let _ = app.update(Message::Macros(macros::Message::ApplyRepeat));
    let _ = app.update(Message::Macros(macros::Message::Insert));
    let _ = app.update(Message::Keys(keymap::Message::Key("Alpha".into())));
    let _ = app.update(Message::Macros(macros::Message::Assign(
        "play-Spare".into(),
    )));
    drain(&mut app);
    assert_eq!(app.notice, "Macro saved and assigned.");
    assert_eq!(
        app.files.form.bindings.get("Spare").map(String::as_str),
        Some("play-Spare")
    );
    assert!(!app.session.macros().unwrap().dirty());
    assert_eq!(
        app.session.keymap().baseline().unwrap().bindings["Typing"]["Alpha"],
        Action::Named {
            id: "play-Spare".into()
        }
    );
    assert_eq!(
        reads.load(Ordering::SeqCst),
        4,
        "save uses the transaction result without a frontend getter or scan restart"
    );
}

#[test]
fn partial_assignment_failure_keeps_saved_macro_and_discard_prompt() {
    let (mut app, _) = macro_app(true);
    let _ = app.update(Message::Read);
    drain(&mut app);
    let _ = app.update(Message::Macros(macros::Message::Select("Greeting".into())));
    drain(&mut app);
    let _ = app.update(Message::Macros(macros::Message::Repeat("2".into())));
    let _ = app.update(Message::Macros(macros::Message::ApplyRepeat));
    let _ = app.update(Message::Keys(keymap::Message::Key("Alpha".into())));
    let _ = app.update(Message::Macros(macros::Message::Assign(
        "play-Greeting".into(),
    )));
    let _ = app.update(Message::Close);
    drain(&mut app);
    assert_eq!(app.closing, Closing::Open);
    assert!(app.notice.starts_with("Macro saved; assignment failed."));
    assert!(!app.files.form.bindings.contains_key("Greeting"));
    assert!(app.assignment_binding.is_none());
    assert_eq!(
        app.session.macros().unwrap().draft().unwrap().repeat_count,
        2
    );
    assert!(!app.session.macros().unwrap().dirty());
    assert!(app.session.keymap().dirty());
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::ConfirmDiscard);
}

fn settle(app: &mut App) {
    let completion = app
        .worker
        .as_ref()
        .unwrap()
        .receive(Some(Duration::from_secs(2)))
        .unwrap();
    let _ = app.complete(completion);
}

fn edit(app: &mut App) {
    let _ = app.update(Message::Keys(keymap::Message::Key("Alpha".into())));
    let _ = app.update(Message::Keys(keymap::Message::Assign(1)));
}

#[test]
fn real_messages_read_edit_save_without_navigation_reads() {
    let (mut app, reads) = app(false);
    let _ = app.update(Message::Read);
    settle(&mut app);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    edit(&mut app);
    assert!(app.session.keymap().dirty());
    let _ = app.update(Message::Save);
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    // A close waiting on a write excludes subsequent edits.
    let _ = app.update(Message::Keys(keymap::Message::Assign(2)));
    settle(&mut app);
    assert!(!app.session.keymap().dirty());
    assert_eq!(
        app.session.keymap().baseline().unwrap().bindings["Typing"]["Alpha"],
        Action::Key(5)
    );
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_save_retains_edits_and_cancels_pending_close() {
    let (mut app, _) = app(true);
    let _ = app.update(Message::Read);
    settle(&mut app);
    edit(&mut app);
    let _ = app.update(Message::Save);
    let _ = app.update(Message::Close);
    settle(&mut app);
    assert_eq!(app.closing, Closing::Open);
    assert!(app.session.keymap().dirty());
    assert!(matches!(
        app.session.keymap().status(),
        Status::Unverified {
            problem: Problem::Apply(_)
        }
    ));
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::ConfirmDiscard);
    let _ = app.update(Message::Keys(keymap::Message::Assign(2)));
    assert_eq!(
        app.session.keymap().draft().unwrap()["Typing"]["Alpha"],
        Action::Key(5)
    );
    let _ = app.update(Message::KeepEditing);
    assert_eq!(app.closing, Closing::Open);
}

#[test]
fn manual_reconnection_retains_unsaved_assignments() {
    let (mut app, reads) = app(false);
    let _ = app.update(Message::Read);
    settle(&mut app);
    edit(&mut app);
    app.worker = None;
    app.session.disconnect().unwrap();
    let _ = app.update(Message::Read);
    settle(&mut app);
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    assert!(app.session.keymap().dirty());
    assert_eq!(
        app.session.keymap().draft().unwrap()["Typing"]["Alpha"],
        Action::Key(5)
    );
}

fn ready_to_record() -> (App, Arc<AtomicUsize>) {
    let (mut app, reads) = macro_app(false);
    let _ = app.update(Message::Read);
    drain(&mut app);
    let _ = app.update(Message::Macros(macros::Message::Select("Greeting".into())));
    drain(&mut app);
    (app, reads)
}

fn key_event(code: iced::keyboard::key::Code, pressed: bool) -> Event {
    use iced::keyboard::{Event as KeyEvent, Key, Location, Modifiers, key::Physical};
    Event::Keyboard(if pressed {
        KeyEvent::KeyPressed {
            key: Key::Unidentified,
            modified_key: Key::Unidentified,
            physical_key: Physical::Code(code),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
            text: None,
            repeat: false,
        }
    } else {
        KeyEvent::KeyReleased {
            key: Key::Unidentified,
            modified_key: Key::Unidentified,
            physical_key: Physical::Code(code),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
        }
    })
}

#[test]
fn recording_is_exclusive_and_focus_loss_stages_held_releases() {
    use byakko_core::model::macros::{Action as MacroAction, Event as MacroEvent};
    use iced::keyboard::key::Code;
    let (mut app, reads) = ready_to_record();
    let before = app.session.macros().unwrap().draft().unwrap().clone();
    let read_count = reads.load(Ordering::SeqCst);
    let _ = app.update(Message::Record(recording::Message::Start));
    assert!(app.session.recording());
    let at = Instant::now();
    for (ms, code, pressed) in [
        (0, Code::KeyA, true),
        (30, Code::KeyB, true),
        (90, Code::KeyB, false),
    ] {
        let _ = app.update(Message::RecordingInput(
            key_event(code, pressed),
            at + Duration::from_millis(ms),
        ));
    }
    for message in [
        Message::Page(Page::Keys),
        Message::Read,
        Message::Save,
        Message::Macros(macros::Message::Clear),
        Message::Poll(Instant::now()),
    ] {
        let _ = app.update(message);
    }
    assert_eq!(app.page, Page::Macros);
    assert!(!app.session.busy());
    let _ = app.update(Message::RecordingInput(
        Event::Window(window::Event::Unfocused),
        at + Duration::from_millis(150),
    ));
    assert!(!app.session.recording());
    let editor = app.session.macros().unwrap();
    assert!(editor.dirty());
    let events = &editor.draft().unwrap().events;
    assert_eq!(&events[..before.events.len()], &before.events);
    let expected: Vec<_> = [(4, true, 30), (5, true, 60), (5, false, 60), (4, false, 50)]
        .into_iter()
        .map(|(usage, pressed, delay_ms)| MacroEvent {
            action: MacroAction::Key { usage, pressed },
            delay_ms,
        })
        .collect();
    assert_eq!(&events[before.events.len()..], expected);
    assert_eq!(reads.load(Ordering::SeqCst), read_count);
}

#[test]
fn closing_recording_releases_keys_before_discard_confirmation() {
    use byakko_core::model::macros::Action as MacroAction;
    use iced::keyboard::key::Code;
    let (mut app, _) = ready_to_record();
    let _ = app.update(Message::Record(recording::Message::Fixed(true)));
    let _ = app.update(Message::Record(recording::Message::Delay("17".into())));
    let _ = app.update(Message::Record(recording::Message::Start));
    let _ = app.update(Message::RecordingInput(
        key_event(Code::ShiftRight, true),
        Instant::now(),
    ));
    let _ = app.update(Message::Close);
    assert!(!app.session.recording());
    assert_eq!(app.closing, Closing::ConfirmDiscard);
    let draft = app.session.macros().unwrap().draft().unwrap().clone();
    let last = draft.events.last().unwrap();
    assert_eq!(
        last.action,
        MacroAction::Key {
            usage: 0xe5,
            pressed: false
        }
    );
    assert_eq!(last.delay_ms, 17);
    let _ = app.update(Message::RecordingInput(
        key_event(Code::KeyB, true),
        Instant::now(),
    ));
    assert_eq!(app.session.macros().unwrap().draft(), Some(&draft));
}

#[test]
fn unsupported_recorded_input_stops_and_preserves_accepted_events() {
    use byakko_core::model::macros::Action as MacroAction;
    let (mut app, _) = ready_to_record();
    let _ = app.update(Message::Record(recording::Message::Start));
    let _ = app.update(Message::RecordingInput(
        key_event(iced::keyboard::key::Code::KeyA, true),
        Instant::now(),
    ));
    let _ = app.update(Message::RecordingInput(
        Event::Mouse(iced::mouse::Event::ButtonPressed(
            iced::mouse::Button::Right,
        )),
        Instant::now(),
    ));
    assert!(!app.session.recording());
    assert!(app.notice.starts_with("Recording stopped:"));
    assert_eq!(
        app.session
            .macros()
            .unwrap()
            .draft()
            .unwrap()
            .events
            .last()
            .unwrap()
            .action,
        MacroAction::Key {
            usage: 4,
            pressed: false
        }
    );
}

#[test]
fn recording_waits_for_catalog_completion_and_focus_can_cancel_the_request() {
    use byakko_core::contract::CompletionPayload;
    for lose_focus in [false, true] {
        let (mut app, _) = ready_to_record();
        // Hold a correlated catalog request so its completion timing is deterministic.
        let catalog = app.session.request_macro_catalog().unwrap();
        let _ = app.update(Message::Record(recording::Message::Start));
        assert!(app.recording.pending());
        assert!(!app.session.recording());
        if lose_focus {
            let _ = app.update(Message::RecordingInput(
                Event::Window(window::Event::Unfocused),
                Instant::now(),
            ));
            assert!(!app.recording.pending());
        }
        let _ = app.complete(catalog.map(|_| CompletionPayload::ReadMacroCatalog {
            result: Err("Cancelled".into()),
        }));
        assert!(!app.recording.pending());
        assert!(!app.session.catalog_scanning());
        assert_eq!(app.session.recording(), !lose_focus);
    }
}

#[test]
fn recording_preferences_preserve_unsubmitted_macro_fields() {
    let (mut app, _) = ready_to_record();
    let _ = app.update(Message::Macros(macros::Message::Repeat("23".into())));
    let _ = app.update(Message::Record(recording::Message::Fixed(true)));
    let _ = app.update(Message::Record(recording::Message::Delay("17".into())));
    assert_eq!(app.macros.repeat, "23");
    let catalog = app.session.request_macro_catalog().unwrap();
    let _ =
        app.complete(catalog.map(
            |_| byakko_core::contract::CompletionPayload::ReadMacroCatalog {
                result: Err("Cancelled".into()),
            },
        ));
    assert_eq!(app.macros.repeat, "23");
}

fn feature_app(fail_lighting: bool) -> (App, Arc<Mutex<Vec<&'static str>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let count = calls.clone();
    let session = memory::demo().unwrap().session().unwrap();
    let mut app = App::new(
        session,
        Box::new(move |_| {
            Ok((
                "demo".into(),
                Executor::spawn(
                    ObservedDevice {
                        memory: memory::demo()?,
                        reads: Default::default(),
                        macro_reads: Default::default(),
                        fail_save: false,
                        feature_calls: count.clone(),
                        fail_lighting,
                    },
                    Default::default(),
                )
                .map_err(|error| error.to_string())?,
            ))
        }),
    );
    // Drive elapsed time explicitly through Poll, avoiding wall-clock-dependent tests.
    app.config.short_edit_delay = Duration::from_secs(60);
    app.config.auto_save_delay = Duration::from_secs(60);
    let _ = app.update(Message::Read);
    drain(&mut app);
    (app, calls)
}

fn elapsed(app: &mut App) {
    let _ = app.update(Message::Poll(Instant::now() + Duration::from_secs(61)));
}

#[test]
fn file_completion_survives_close_and_import_stages_only_the_selected_draft() {
    let (mut app, reads) = ready_to_record();
    let directory = file_test_directory("import-close");
    let path = directory.join("macro.json");
    let mut document = app
        .session
        .export_macro_document("Imported name".into(), None)
        .unwrap();
    document.source_slot = "Preserved".into();
    document.program.repeat_count = 7;
    byakko_devices::storage::macros::save_new(&path, &document).unwrap();
    let before = reads.load(Ordering::SeqCst);
    let _ = app.update(Message::Files(files::Message::MacroPath(
        path.to_string_lossy().into(),
    )));
    let job = app
        .files
        .begin(files::Operation::ImportMacro, &app.session)
        .unwrap();
    let _ = app.update(Message::Page(Page::Settings));
    let _ = app.update(Message::Macros(macros::Message::Repeat("22".into())));
    assert_ne!(app.page, Page::Settings);
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    let _ = app.update(Message::FileComplete(job.run()));
    assert_eq!(app.closing, Closing::ConfirmDiscard);
    assert_eq!(app.session.macros().unwrap().slot(), "Greeting");
    assert_eq!(
        app.session.macros().unwrap().draft().unwrap().repeat_count,
        7
    );
    assert_eq!(app.macros.repeat, "7");
    assert_eq!(app.files.form.name("Greeting"), "Imported name");
    assert_eq!(reads.load(Ordering::SeqCst), before);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn failed_file_operation_reopens_a_waiting_window_and_retains_the_draft() {
    let (mut app, _) = ready_to_record();
    let directory = file_test_directory("export-close");
    let path = directory.join("existing.json");
    std::fs::write(&path, b"keep me").unwrap();
    let before = app.session.macros().unwrap().draft().unwrap().clone();
    app.files.form.macro_path = path.to_string_lossy().into();
    let job = app
        .files
        .begin(files::Operation::ExportMacro, &app.session)
        .unwrap();
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    let _ = app.update(Message::FileComplete(job.run()));
    assert_eq!(app.closing, Closing::Open);
    assert!(!app.notice.is_empty());
    assert_eq!(app.session.macros().unwrap().draft(), Some(&before));
    assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn archive_navigation_is_passive_and_explicit_capture_exports_raw_bytes() {
    let (mut app, calls) = feature_app(false);
    let baseline = app.session.keymap().baseline().unwrap().clone();
    let _ = app.update(Message::Page(Page::Archive));
    assert!(calls.lock().unwrap().is_empty());
    let _ = app.update(Message::Files(files::Message::Capture));
    drain(&mut app);
    assert_eq!(*calls.lock().unwrap(), ["capture-archive"]);
    assert_eq!(app.session.keymap().baseline(), Some(&baseline));
    let directory = file_test_directory("archive");
    let path = directory.join("capture.json");
    app.files.form.archive_path = path.to_string_lossy().into();
    let job = app
        .files
        .begin(files::Operation::ExportArchive, &app.session)
        .unwrap();
    let _ = app.update(Message::FileComplete(job.run()));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        app.session.archive().unwrap().captured().unwrap().bytes
    );
    assert_eq!(*calls.lock().unwrap(), ["capture-archive"]);
    std::fs::remove_dir_all(directory).unwrap();
}

fn file_test_directory(name: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "byakko-desktop-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir(&path).unwrap();
    path
}

#[test]
fn lighting_coalesces_and_keeps_newer_intent_while_a_write_is_submitted() {
    use byakko_core::model::lighting::{Edit, Evidence};
    let (mut app, calls) = feature_app(false);
    let _ = app.update(Message::Page(Page::Lighting));
    drain(&mut app);
    for brightness in [2, 4] {
        let _ = app.update(Message::Lighting(lighting::Message::Edit(
            Edit::Brightness(brightness),
        )));
    }
    assert!(!app.session.busy());
    assert_eq!(*calls.lock().unwrap(), ["read-lighting"]);
    elapsed(&mut app);
    assert_eq!(
        app.session
            .lighting()
            .unwrap()
            .submitted()
            .unwrap()
            .brightness,
        Some(4)
    );
    let _ = app.update(Message::Lighting(lighting::Message::Edit(
        Edit::Brightness(5),
    )));
    drain(&mut app);
    assert!(app.session.lighting().unwrap().dirty());
    elapsed(&mut app);
    drain(&mut app);
    let editor = app.session.lighting().unwrap();
    assert!(!editor.dirty());
    assert_eq!(editor.draft().unwrap().brightness, Some(5));
    assert_eq!(
        editor.baseline().unwrap().evidence,
        Evidence::TransportAccepted
    );
    let _ = app.update(Message::Page(Page::Keys));
    let _ = app.update(Message::Page(Page::Lighting));
    assert_eq!(
        *calls.lock().unwrap(),
        ["read-lighting", "apply-lighting", "apply-lighting"]
    );
}

#[test]
fn picture_colors_batch_and_only_real_selector_changes_require_a_new_read() {
    use byakko_core::model::lighting::Edit;
    let (mut app, calls) = feature_app(false);
    let _ = app.update(Message::Page(Page::Picture));
    drain(&mut app);
    assert_eq!(
        *calls.lock().unwrap(),
        ["read-lighting", "apply-lighting", "read-picture"]
    );
    let _ = app.update(Message::Picture(picture::Message::Select("Alpha".into())));
    let _ = app.update(Message::Picture(picture::Message::Color([4, 5, 6])));
    let _ = app.update(Message::Picture(picture::Message::Select("Beta".into())));
    let _ = app.update(Message::Picture(picture::Message::Color([7, 8, 9])));
    elapsed(&mut app);
    drain(&mut app);
    assert!(!app.session.picture().unwrap().dirty());
    let _ = app.update(Message::Page(Page::Lighting));
    let _ = app.update(Message::Lighting(lighting::Message::Edit(
        Edit::Brightness(2),
    )));
    elapsed(&mut app);
    drain(&mut app);
    assert_eq!(app.session.picture().unwrap().status(), &Status::Ready);
    let _ = app.update(Message::Lighting(lighting::Message::Edit(Edit::Effect(
        "steady".into(),
    ))));
    elapsed(&mut app);
    drain(&mut app);
    assert!(matches!(
        app.session.picture().unwrap().status(),
        Status::Unverified {
            problem: Problem::ReadRequired
        }
    ));
    let _ = app.update(Message::Page(Page::Picture));
    drain(&mut app);
    assert_eq!(
        *calls.lock().unwrap(),
        [
            "read-lighting",
            "apply-lighting",
            "read-picture",
            "apply-picture",
            "apply-lighting",
            "apply-lighting",
            "apply-lighting",
            "read-picture"
        ]
    );
}

#[test]
fn close_flushes_queued_scalar_edits_and_failed_save_retains_draft_without_retry() {
    use byakko_core::model::{lighting::Edit, settings::Value};
    let (mut app, calls) = feature_app(false);
    let _ = app.update(Message::Page(Page::Settings));
    drain(&mut app);
    let _ = app.update(Message::Settings(settings::Message::Number(
        "sleep".into(),
        "7".into(),
    )));
    let _ = app.update(Message::Settings(settings::Message::ApplyNumber(
        "sleep".into(),
    )));
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    drain(&mut app);
    assert!(!app.session.settings().unwrap().dirty());
    assert_eq!(
        app.session.settings().unwrap().draft().unwrap()["sleep"],
        Value::Number(7)
    );
    assert_eq!(*calls.lock().unwrap(), ["read-settings", "apply-settings"]);

    let (mut app, calls) = feature_app(true);
    let _ = app.update(Message::Page(Page::Lighting));
    drain(&mut app);
    let _ = app.update(Message::Lighting(lighting::Message::Edit(
        Edit::Brightness(2),
    )));
    let _ = app.update(Message::Close);
    drain(&mut app);
    assert_eq!(app.closing, Closing::Open);
    assert!(app.session.lighting().unwrap().dirty());
    elapsed(&mut app);
    assert!(!app.session.busy());
    assert_eq!(*calls.lock().unwrap(), ["read-lighting", "apply-lighting"]);
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::ConfirmDiscard);
}

#[test]
fn picture_navigation_finishes_queued_lighting_and_reuses_the_brush() {
    use byakko_core::model::lighting::Edit;
    let (mut app, calls) = feature_app(false);
    let _ = app.update(Message::Page(Page::Lighting));
    drain(&mut app);
    let _ = app.update(Message::Lighting(lighting::Message::Edit(
        Edit::Brightness(2),
    )));
    let _ = app.update(Message::Page(Page::Picture));
    assert!(!picture_is_displayed(&app.session));
    drain(&mut app);
    assert!(picture_is_displayed(&app.session));
    let _ = app.update(Message::Picture(picture::Message::Select("Alpha".into())));
    let _ = app.update(Message::Picture(picture::Message::Color([12, 34, 56])));
    let _ = app.update(Message::Picture(picture::Message::Select("Beta".into())));
    assert_eq!(
        app.session.picture().unwrap().draft().unwrap()["Beta"],
        [12, 34, 56]
    );
    elapsed(&mut app);
    drain(&mut app);
    let _ = app.update(Message::Page(Page::Keys));
    let _ = app.update(Message::Page(Page::Picture));
    assert!(!app.session.busy());
    assert_eq!(
        *calls.lock().unwrap(),
        [
            "read-lighting",
            "apply-lighting",
            "apply-lighting",
            "read-picture",
            "apply-picture"
        ]
    );
}

#[test]
fn failed_picture_activation_does_not_read_or_retry_and_keeps_diagnostic() {
    let (mut app, calls) = feature_app(true);
    let _ = app.update(Message::Page(Page::Picture));
    drain(&mut app);
    assert!(
        app.notice
            .starts_with("Could not prepare per-key lighting.")
    );
    assert!(!picture_is_displayed(&app.session));
    assert!(app.session.lighting().unwrap().dirty());
    elapsed(&mut app);
    assert_eq!(*calls.lock().unwrap(), ["read-lighting", "apply-lighting"]);
    assert!(!app.session.busy());
    assert_eq!(app.closing, Closing::Open);
}
