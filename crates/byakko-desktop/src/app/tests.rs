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
}
impl Device for ObservedDevice {
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
                },
                Default::default(),
            )
            .map_err(|e| e.to_string())?;
            Ok(("demo".into(), worker))
        }),
    );
    (app, reads)
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
