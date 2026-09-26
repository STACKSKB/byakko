use super::*;
use byakko_core::{
    contract::{ApplyFailure, Recovery},
    model::keymap::{Action, Change, State},
};
use byakko_devices::{
    Device,
    memory::{self, MemoryDevice},
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct ObservedDevice {
    memory: MemoryDevice,
    reads: Arc<AtomicUsize>,
    fail_save: bool,
    macro_reads: Arc<AtomicUsize>,
}
impl Device for ObservedDevice {
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
        Message::Poll,
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
