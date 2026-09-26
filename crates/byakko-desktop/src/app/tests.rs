use super::*;
use byakko_core::{
    Action, Change, State,
    contract::{ApplyFailure, Recovery},
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
    fn read_macro(&mut self, slot: &str) -> Result<byakko_core::macros::Snapshot, String> {
        self.macro_reads.fetch_add(1, Ordering::SeqCst);
        self.memory.read_macro(slot)
    }
    fn apply_macro(
        &mut self,
        expected: &byakko_core::macros::Snapshot,
        desired: &byakko_core::macros::Program,
        backup: &Path,
    ) -> Result<byakko_core::macros::Snapshot, ApplyFailure> {
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
    app.session.disconnect();
    let _ = app.update(Message::Read);
    settle(&mut app);
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    assert!(app.session.keymap().dirty());
    assert_eq!(
        app.session.keymap().draft().unwrap()["Typing"]["Alpha"],
        Action::Key(5)
    );
}
