use super::*;
use crate::controller::discovery::Availability;
use crate::view::application::picture_is_displayed;
use byakko_core::{
    contract::{ApplyFailure, Recovery},
    model::keymap::{Action, Change, State},
};
use byakko_devices::{
    Device, Executor,
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
    fn start_host_lighting(
        &mut self,
        mode: byakko_core::model::lighting::HostMode,
        setting: Option<byakko_core::model::lighting::Setting>,
        expected: &byakko_core::model::lighting::Snapshot,
        backup: &Path,
    ) -> Result<Box<dyn byakko_devices::HostActivity>, ApplyFailure> {
        self.feature_calls.lock().unwrap().push("host-start");
        Ok(Box::new(ObservedHost {
            activity: self
                .memory
                .start_host_lighting(mode, setting, expected, backup)?,
            calls: self.feature_calls.clone(),
            fail_restore: self.fail_lighting,
        }))
    }
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

struct ObservedHost {
    activity: Box<dyn byakko_devices::HostActivity>,
    calls: Arc<Mutex<Vec<&'static str>>>,
    fail_restore: bool,
}
impl byakko_devices::HostActivity for ObservedHost {
    fn send_frame(&mut self, frame: byakko_devices::HostFrame) -> Result<(), String> {
        self.calls.lock().unwrap().push("host-frame");
        self.activity.send_frame(frame)
    }
    fn finish(self: Box<Self>) -> Result<byakko_core::model::lighting::Snapshot, ApplyFailure> {
        self.calls.lock().unwrap().push("host-finish");
        let restored = self.activity.finish()?;
        if self.fail_restore {
            Err(ApplyFailure {
                message: "Injected restoration read failure".into(),
                recovery: Recovery::Unverified,
            })
        } else {
            Ok(restored)
        }
    }
}

fn synthetic_sampler(
    source: byakko_core::model::lighting::HostSource,
    _: byakko_devices::screen_sample::ScreenCapture,
) -> Result<crate::controller::sampler::Sampler, String> {
    crate::controller::sampler::Sampler::spawn(Duration::from_millis(10), move || {
        Ok(move || {
            Ok(match source {
                byakko_core::model::lighting::HostSource::ScreenAverage => {
                    byakko_devices::HostFrame::Rgb([12, 34, 56])
                }
                byakko_core::model::lighting::HostSource::PlaybackAudio { bands } => {
                    byakko_devices::HostFrame::Bands(vec![0; usize::from(bands)])
                }
            })
        })
    })
}

fn poll_host_until(app: &mut App, predicate: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !predicate(app) {
        assert!(Instant::now() < deadline, "host timeout: {}", app.notice);
        let _ = app.update(Message::Poll(Instant::now()));
        std::thread::yield_now();
    }
}

#[test]
fn host_messages_prepare_stream_restore_on_focus_loss_and_preserve_other_drafts() {
    let (mut app, calls) = feature_app(false);
    app.host = Host::new(synthetic_sampler);
    edit(&mut app);
    let keymap = app.session.keymap().draft().cloned();
    let lighting = app.session.lighting().unwrap().baseline().cloned();
    let picture = app.session.picture().unwrap().baseline().cloned();
    let _ = app.update(Message::Host(host::Message::Mode("screen-average".into())));
    let _ = app.update(Message::Host(host::Message::Start));
    assert!(app.host.preparing());
    poll_host_until(&mut app, |app| app.session.host().active());
    let deadline = Instant::now() + Duration::from_secs(2);
    while !calls.lock().unwrap().contains(&"host-frame") {
        assert!(Instant::now() < deadline);
        let _ = app.update(Message::Poll(Instant::now()));
        std::thread::yield_now();
    }
    let _ = app.update(Message::Read);
    let _ = app.update(Message::Macros(macros::Message::Clear));
    assert!(app.session.host().active());
    let _ = app.update(Message::HostFocusLost);
    poll_host_until(&mut app, |app| !app.host.busy());
    assert!(app.session.host().is_idle());
    assert_eq!(app.session.keymap().draft(), keymap.as_ref());
    assert_eq!(
        app.session.lighting().unwrap().baseline(),
        lighting.as_ref()
    );
    assert_eq!(app.session.picture().unwrap().baseline(), picture.as_ref());
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls.iter().filter(|call| **call == "host-start").count(),
        1
    );
    assert_eq!(
        calls.iter().filter(|call| **call == "host-finish").count(),
        1
    );
    assert!(
        calls
            .iter()
            .all(|call| matches!(*call, "host-start" | "host-frame" | "host-finish"))
    );
}

#[test]
fn host_preparation_failure_and_cancellation_never_start_the_device() {
    for fail in [false, true] {
        let (mut app, calls) = feature_app(false);
        app.host = if fail {
            Host::new(|_, _| {
                crate::controller::sampler::Sampler::spawn(Duration::from_secs(1), || {
                    Err::<fn() -> Result<byakko_devices::HostFrame, String>, _>(
                        "Injected sampler preparation failure".into(),
                    )
                })
            })
        } else {
            Host::new(synthetic_sampler)
        };
        let _ = app.update(Message::Host(host::Message::Mode("screen-average".into())));
        let _ = app.update(Message::Host(host::Message::Start));
        if fail {
            poll_host_until(&mut app, |app| !app.host.busy());
            assert!(app.notice.contains("preparation failure"));
        } else {
            let _ = app.update(Message::Host(host::Message::Stop));
            for _ in 0..5 {
                let _ = app.update(Message::Poll(Instant::now()));
            }
        }
        assert!(!app.host.busy());
        assert!(app.session.host().is_idle());
        assert!(calls.lock().unwrap().is_empty());
    }
}

#[test]
fn host_sampler_failure_restores_lighting_and_retains_the_error() {
    let (mut app, calls) = feature_app(false);
    app.host = Host::new(|_, _| {
        crate::controller::sampler::Sampler::spawn(Duration::from_millis(1), || {
            Ok(|| Err("Injected capture failure".into()))
        })
    });
    let lighting = app.session.lighting().unwrap().baseline().cloned();
    let _ = app.update(Message::Host(host::Message::Mode("screen-average".into())));
    let _ = app.update(Message::Host(host::Message::Start));
    poll_host_until(&mut app, |app| !app.host.busy());
    assert!(app.notice.contains("Injected capture failure"));
    assert!(app.notice.contains("The previous settings were restored."));
    assert_eq!(app.closing, Closing::Open);
    assert_eq!(app.session.lighting().unwrap().status(), &Status::Ready);
    assert_eq!(
        app.session.lighting().unwrap().baseline(),
        lighting.as_ref()
    );
    assert_eq!(*calls.lock().unwrap(), ["host-start", "host-finish"]);
}

#[test]
fn host_close_waits_for_restoration_and_reopens_on_unknown_restore() {
    for fail_restore in [false, true] {
        let (mut app, calls) = feature_app(fail_restore);
        app.host = Host::new(synthetic_sampler);
        edit(&mut app);
        let _ = app.update(Message::Host(host::Message::Mode("screen-average".into())));
        let _ = app.update(Message::Host(host::Message::Start));
        poll_host_until(&mut app, |app| !app.session.host().is_idle());
        let _ = app.update(Message::Close);
        assert_eq!(app.closing, Closing::Waiting);
        poll_host_until(&mut app, |app| !app.host.busy());
        assert_eq!(
            calls
                .lock()
                .unwrap()
                .iter()
                .filter(|call| **call == "host-finish")
                .count(),
            1
        );
        assert!(app.session.keymap().dirty());
        if fail_restore {
            assert_eq!(app.closing, Closing::Open);
            assert!(app.session.requires_manual_read());
            assert!(app.notice.contains("restoration read failure"));
        } else {
            assert_eq!(app.closing, Closing::ConfirmDiscard);
        }
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
    while app.session.busy() || app.session.catalog_scanning() || app.link.settling() {
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
    let _ = app.update(Message::Macros(macros::Message::NewEvent));
    let _ = app.update(Message::Macros(macros::Message::Kind(macros::Kind::Key)));
    let _ = app.update(Message::Macros(macros::Message::Value("4".into())));
    let _ = app.update(Message::Macros(macros::Message::Delay("0".into())));
    let _ = app.update(Message::Macros(macros::Message::StageEvent));
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
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.link.settling() {
        assert!(Instant::now() < deadline, "worker did not retire");
        let _ = app.update(Message::Poll(Instant::now()));
        std::thread::yield_now();
    }
    if !app.session.busy() && !app.session.catalog_scanning() {
        return;
    }
    let completion = app
        .link
        .executor()
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
    app.link.retire(&mut app.session).unwrap();
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

#[test]
fn repeat_text_stages_immediately_and_invalid_text_blocks_save_and_clean_assignment() {
    let (mut app, _) = ready_to_record();
    let _ = app.update(Message::Macros(macros::Message::Event(0)));
    let _ = app.update(Message::Macros(macros::Message::Delay("unfinished".into())));
    let _ = app.update(Message::Macros(macros::Message::Repeat(" 03 ".into())));
    assert_eq!(
        app.session.macros().unwrap().draft().unwrap().repeat_count,
        3
    );
    assert_eq!(app.macros.repeat, " 03 ");
    assert_eq!(app.macros.delay, "unfinished");
    assert_eq!(app.macros.composer, macros::Composer::Replace(0));
    let _ = app.update(Message::Macros(macros::Message::Save));
    drain(&mut app);
    assert!(!app.session.macros().unwrap().dirty());
    let _ = app.update(Message::Keys(keymap::Message::Key("Alpha".into())));
    for invalid in ["", "oops", "0", "65536"] {
        let _ = app.update(Message::Macros(macros::Message::Repeat(invalid.into())));
        for message in [
            macros::Message::Save,
            macros::Message::Assign("play-Greeting".into()),
        ] {
            let _ = app.update(Message::Macros(message));
            assert!(!app.session.busy());
            assert!(!app.notice.is_empty());
            assert!(app.assignment_binding.is_none());
            assert_eq!(app.macros.repeat, invalid);
            assert_eq!(
                app.session.macros().unwrap().draft().unwrap().repeat_count,
                3
            );
        }
    }
    assert!(!app.session.keymap().dirty());
}

#[test]
fn unchanged_macro_content_preserves_composer_across_new_revision_and_failed_reads() {
    use byakko_core::contract::FeatureResult;
    let (mut app, _) = ready_to_record();
    let _ = app.update(Message::Macros(macros::Message::Event(1)));
    let _ = app.update(Message::Macros(macros::Message::Delay("unfinished".into())));
    let mut snapshot = app.session.macros().unwrap().baseline().unwrap().clone();
    snapshot.revision.push(42);
    let command = app.session.read_macro().unwrap();
    let mut stale = command.clone().map(|_| CompletionPayload::Macro {
        slot: "Greeting".into(),
        result: FeatureResult::Read(Ok(snapshot.clone())),
    });
    stale.generation = stale.generation.wrapping_add(1);
    let _ = app.complete(stale);
    assert!(app.session.busy());
    assert_eq!(app.macros.delay, "unfinished");
    assert_eq!(app.macros.composer, macros::Composer::Replace(1));
    let _ = app.complete(command.map(|_| CompletionPayload::Macro {
        slot: "Greeting".into(),
        result: FeatureResult::Read(Ok(snapshot.clone())),
    }));
    for result in [Ok(snapshot), Err("Injected read failure".into())] {
        let command = app.session.read_macro().unwrap();
        let _ = app.complete(command.map(|_| CompletionPayload::Macro {
            slot: "Greeting".into(),
            result: FeatureResult::Read(result),
        }));
        assert_eq!(app.macros.delay, "unfinished");
        assert_eq!(app.macros.composer, macros::Composer::Replace(1));
    }
}

#[test]
fn changed_slot_resets_composer_even_with_identical_program() {
    use byakko_core::contract::FeatureResult;
    let (mut app, _) = ready_to_record();
    let _ = app.update(Message::Macros(macros::Message::Event(0)));
    let _ = app.update(Message::Macros(macros::Message::Delay("unfinished".into())));
    let mut snapshot = app.session.macros().unwrap().baseline().unwrap().clone();
    app.session.select_macro("Spare").unwrap();
    snapshot.slot = "Spare".into();
    let command = app.session.read_macro().unwrap();
    let _ = app.complete(command.map(|_| CompletionPayload::Macro {
        slot: "Spare".into(),
        result: FeatureResult::Read(Ok(snapshot)),
    }));
    assert_eq!(app.macros.composer, macros::Composer::Closed);
    assert_ne!(app.macros.delay, "unfinished");
}

#[test]
fn macro_composer_resets_only_after_accepted_sequence_edit() {
    use byakko_core::contract::FeatureResult;
    let (mut app, _) = ready_to_record();
    let original = app.session.macros().unwrap().draft().unwrap().clone();
    let _ = app.update(Message::Macros(macros::Message::Event(0)));
    for invalid in ["unfinished", "4294967295"] {
        let _ = app.update(Message::Macros(macros::Message::Delay(invalid.into())));
        let _ = app.update(Message::Macros(macros::Message::StageEvent));
        assert_eq!(app.macros.composer, macros::Composer::Replace(0));
        assert_eq!(app.macros.delay, invalid);
        assert_eq!(app.session.macros().unwrap().draft(), Some(&original));
    }
    let command = app.session.read_macro().unwrap();
    let _ = app.update(Message::Macros(macros::Message::Revert));
    assert_eq!(app.macros.composer, macros::Composer::Replace(0));
    assert_eq!(app.macros.delay, "4294967295");
    let _ = app.update(Message::Macros(macros::Message::Delay("99".into())));
    assert_eq!(app.macros.delay, "4294967295", "busy inputs are disabled");
    let snapshot = app.session.macros().unwrap().baseline().unwrap().clone();
    let _ = app.complete(command.map(|_| CompletionPayload::Macro {
        slot: "Greeting".into(),
        result: FeatureResult::Read(Ok(snapshot)),
    }));
    let _ = app.update(Message::Macros(macros::Message::Delay("19".into())));
    let _ = app.update(Message::Macros(macros::Message::StageEvent));
    assert_eq!(
        app.session.macros().unwrap().draft().unwrap().events[0].delay_ms,
        19
    );
    assert_eq!(app.macros.composer, macros::Composer::Closed);
    let _ = app.update(Message::Macros(macros::Message::Event(0)));
    let _ = app.update(Message::Macros(macros::Message::NewEvent));
    assert_eq!(app.macros.composer, macros::Composer::New);
    let _ = app.update(Message::Macros(macros::Message::Kind(macros::Kind::Key)));
    let _ = app.update(Message::Macros(macros::Message::Value("5".into())));
    let _ = app.update(Message::Macros(macros::Message::Delay("0".into())));
    let _ = app.update(Message::Macros(macros::Message::StageEvent));
    assert_eq!(
        app.session.macros().unwrap().draft().unwrap().events.len(),
        original.events.len() + 1
    );
    assert_eq!(app.macros.composer, macros::Composer::Closed);
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
    assert_eq!(
        *calls.lock().unwrap(),
        ["read-lighting", "read-settings", "read-picture"]
    );
    calls.lock().unwrap().clear();
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
    assert!(calls.lock().unwrap().is_empty());
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
    assert_eq!(*calls.lock().unwrap(), ["apply-lighting", "apply-lighting"]);
}

#[test]
fn picture_colors_batch_and_only_real_selector_changes_require_a_new_read() {
    use byakko_core::model::lighting::Edit;
    let (mut app, calls) = feature_app(false);
    let _ = app.update(Message::Page(Page::Picture));
    drain(&mut app);
    assert_eq!(*calls.lock().unwrap(), ["apply-lighting", "read-picture"]);
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
    assert_eq!(*calls.lock().unwrap(), ["apply-settings"]);

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
    assert_eq!(*calls.lock().unwrap(), ["apply-lighting"]);
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
    assert_eq!(*calls.lock().unwrap(), ["apply-lighting"]);
    assert!(!app.session.busy());
    assert_eq!(app.closing, Closing::Open);
}

struct DiscoveryApp {
    app: App,
    presence: Arc<Mutex<Availability>>,
    attachments: Arc<Mutex<Vec<Option<String>>>>,
    reads: Arc<AtomicUsize>,
    reject_attach: Arc<std::sync::atomic::AtomicBool>,
}
impl DiscoveryApp {
    fn new(fail_save: bool) -> Self {
        let presence = Arc::new(Mutex::new(Availability::Missing));
        let probe = presence.clone();
        let attachments = Arc::new(Mutex::new(Vec::new()));
        let calls = attachments.clone();
        let reads = Arc::new(AtomicUsize::new(0));
        let count = reads.clone();
        let reject_attach = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let rejected = reject_attach.clone();
        let available = presence.clone();
        let session = Session::new(memory::demo().unwrap().descriptor().clone()).unwrap();
        let mut app = App::new(
            session,
            Box::new(move |expected| {
                calls.lock().unwrap().push(expected.map(str::to_owned));
                if rejected.load(Ordering::SeqCst) {
                    return Err("Injected attach failure".into());
                }
                let Availability::Ready { id } = available.lock().unwrap().clone() else {
                    return Err("No keyboard".into());
                };
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
                .map_err(|error| error.to_string())?;
                Ok((id, worker))
            }),
        );
        app.link
            .monitor(Discovery::spawn(move || probe.lock().unwrap().clone()).unwrap());
        Self {
            app,
            presence,
            attachments,
            reads,
            reject_attach,
        }
    }
    fn observe(&mut self, presence: Availability) {
        *self.presence.lock().unwrap() = presence.clone();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let _ = self.app.update(Message::Scan);
            drain(&mut self.app);
            if self.app.link.presence() == &presence {
                break;
            }
            assert!(Instant::now() < deadline, "discovery did not settle");
            std::thread::yield_now();
        }
    }
    fn wait_for(&mut self, ready: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !ready(&self.app) {
            let _ = self.app.update(Message::Scan);
            drain(&mut self.app);
            assert!(Instant::now() < deadline, "connection did not settle");
            std::thread::yield_now();
        }
    }
}

#[test]
fn discovery_replaces_connections_but_never_reads_on_an_unchanged_scan() {
    let mut h = DiscoveryApp::new(false);
    h.observe(Availability::Missing);
    assert!(h.attachments.lock().unwrap().is_empty());
    h.observe(Availability::Ready { id: "first".into() });
    assert_eq!(h.reads.load(Ordering::SeqCst), 1);
    let first_generation = h.app.session.connection().clone();
    edit(&mut h.app);
    h.observe(Availability::Ready { id: "first".into() });
    assert_eq!(h.reads.load(Ordering::SeqCst), 1);
    h.observe(Availability::Missing);
    assert!(h.app.link.executor().is_none());
    assert_eq!(h.app.session.connection(), &Connection::Disconnected);
    assert!(h.app.session.keymap().dirty());
    h.observe(Availability::Ready {
        id: "second".into(),
    });
    assert_ne!(h.app.session.connection(), &first_generation);
    assert_eq!(h.reads.load(Ordering::SeqCst), 2);
    assert!(h.app.session.keymap().dirty());
    assert_eq!(
        *h.attachments.lock().unwrap(),
        [Some("first".into()), Some("second".into())]
    );
    assert_eq!(h.app.session.keymap().status(), &Status::Ready);
}

#[test]
fn manual_refresh_replaces_worker_even_without_a_discovery_change_and_failure_retires_it() {
    let mut h = DiscoveryApp::new(false);
    h.observe(Availability::Ready { id: "first".into() });
    let generation = h.app.session.connection().clone();
    edit(&mut h.app);
    // Unplug/replug occurred between scans; deliberate Read chooses the current collection.
    *h.presence.lock().unwrap() = Availability::Ready {
        id: "second".into(),
    };
    let _ = h.app.update(Message::Read);
    drain(&mut h.app);
    assert_ne!(h.app.session.connection(), &generation);
    assert_eq!(*h.attachments.lock().unwrap(), [Some("first".into()), None]);
    assert!(h.app.session.keymap().dirty());
    h.reject_attach.store(true, Ordering::SeqCst);
    let _ = h.app.update(Message::Read);
    drain(&mut h.app);
    assert!(h.app.link.executor().is_none());
    assert_eq!(h.app.session.connection(), &Connection::Disconnected);
    assert!(h.app.session.keymap().dirty());
    assert_eq!(h.app.notice, "Injected attach failure");
}

#[test]
fn failed_write_holds_automatic_refresh_until_explicit_read() {
    let mut h = DiscoveryApp::new(true);
    h.observe(Availability::Ready { id: "first".into() });
    edit(&mut h.app);
    let _ = h.app.update(Message::Save);
    drain(&mut h.app);
    let diagnostic = h.app.session.keymap().status().clone();
    assert!(h.app.session.requires_manual_read());
    h.observe(Availability::Missing);
    h.observe(Availability::Ready {
        id: "second".into(),
    });
    assert_eq!(h.attachments.lock().unwrap().len(), 1);
    assert!(h.app.link.executor().is_none());
    assert_eq!(h.app.session.keymap().status(), &diagnostic);
    assert!(h.app.session.keymap().dirty());
    let _ = h.app.update(Message::Read);
    drain(&mut h.app);
    assert_eq!(h.app.session.keymap().status(), &Status::Ready);
    assert!(!h.app.session.requires_manual_read());
    assert!(h.app.session.keymap().dirty());
    assert_eq!(h.attachments.lock().unwrap().last(), Some(&None));
}

#[test]
fn reconnect_after_read_failure_waits_for_cleanup_and_keeps_edits() {
    struct FailingDevice {
        memory: MemoryDevice,
        reads: usize,
        release: std::sync::mpsc::Receiver<()>,
        retiring: std::sync::mpsc::Sender<()>,
    }
    impl Device for FailingDevice {
        fn read(&mut self) -> Result<State, String> {
            self.reads += 1;
            if self.reads == 1 {
                self.memory.read()
            } else {
                Err("The keyboard was disconnected.".into())
            }
        }
        fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
            unreachable!("this test never writes")
        }
    }
    impl Drop for FailingDevice {
        fn drop(&mut self) {
            let _ = self.retiring.send(());
            // Model a worker still releasing its native resources after a failed read.
            let _ = self.release.recv_timeout(Duration::from_secs(3));
        }
    }
    let (release, released) = std::sync::mpsc::channel();
    let (retiring, retired) = std::sync::mpsc::channel();
    let first = Mutex::new(Some(FailingDevice {
        memory: memory::demo().unwrap(),
        reads: 0,
        release: released,
        retiring,
    }));
    let attachments = Arc::new(AtomicUsize::new(0));
    let count = attachments.clone();
    let mut app = App::new(
        Session::new(memory::demo().unwrap().descriptor().clone()).unwrap(),
        Box::new(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            let worker = if let Some(first) = first.lock().unwrap().take() {
                Executor::spawn(first, Default::default())
            } else {
                Executor::spawn(memory::demo()?, Default::default())
            }
            .map_err(|error| error.to_string())?;
            Ok(("demo".into(), worker))
        }),
    );
    let _ = app.update(Message::Read);
    drain(&mut app);
    edit(&mut app);
    let draft = app.session.keymap().draft().cloned();
    let request = app.session.read();
    let _ = app.submit(request);
    drain(&mut app);
    assert!(
        !app.session.busy(),
        "failure must clear the pending operation"
    );
    assert!(app.notice.contains("keyboard was disconnected"));
    let _ = app.update(Message::Read);
    retired.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(app.link.settling());
    assert!(app.link.executor().is_none());
    for _ in 0..3 {
        let _ = app.update(Message::Poll(Instant::now()));
        let _ = app.update(Message::Read);
    }
    assert_eq!(attachments.load(Ordering::SeqCst), 1);
    assert_eq!(app.session.keymap().draft(), draft.as_ref());
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    release.send(()).unwrap();
    drain(&mut app);
    assert_eq!(attachments.load(Ordering::SeqCst), 2);
    assert_eq!(app.session.keymap().status(), &Status::Ready);
    assert_eq!(app.session.keymap().draft(), draft.as_ref());
    assert!(app.session.keymap().dirty());
    assert_eq!(app.closing, Closing::ConfirmDiscard);
}

#[test]
fn ambiguous_and_failed_enumeration_never_attach_or_clear_drafts() {
    let mut h = DiscoveryApp::new(false);
    h.observe(Availability::Ready { id: "first".into() });
    edit(&mut h.app);
    h.observe(Availability::Ambiguous { count: 2 });
    assert!(h.app.link.executor().is_none());
    h.observe(Availability::Error("permission".into()));
    assert!(h.app.session.keymap().dirty());
    assert_eq!(h.attachments.lock().unwrap().len(), 1);
    assert_eq!(h.reads.load(Ordering::SeqCst), 1);
}

#[test]
fn unchanged_inventory_retries_failed_attachment_without_fast_idle_scans() {
    let mut h = DiscoveryApp::new(false);
    h.reject_attach.store(true, Ordering::SeqCst);
    h.observe(Availability::Ready { id: "first".into() });
    assert!(h.app.link.executor().is_none());
    assert!(!h.app.link.awaiting_discovery());
    h.reject_attach.store(false, Ordering::SeqCst);
    h.wait_for(|app| app.link.executor().is_some());
    assert_eq!(h.reads.load(Ordering::SeqCst), 1);
    let _ = h.app.update(Message::Scan);
    assert!(
        !h.app.link.awaiting_discovery(),
        "a pending idle scan must not enable the startup cadence"
    );
}

#[test]
fn refreshing_a_picture_page_without_macros_never_activates_lighting() {
    let device = memory::demo().unwrap();
    let session = Session::new(device.descriptor().clone())
        .unwrap()
        .with_lighting(device.lighting_capabilities().unwrap().clone())
        .unwrap()
        .with_picture(device.picture_capabilities().unwrap().clone())
        .unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = calls.clone();
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
                        fail_lighting: false,
                        feature_calls: observed.clone(),
                    },
                    Default::default(),
                )
                .map_err(|error| error.to_string())?,
            ))
        }),
    );
    let _ = app.update(Message::Page(Page::Picture));
    let _ = app.update(Message::Read);
    drain(&mut app);
    assert_eq!(*calls.lock().unwrap(), ["read-lighting", "read-picture"]);
    assert!(!picture_is_displayed(&app.session));
    assert!(!app.session.lighting().unwrap().dirty());
    let _ = app.update(Message::Read);
    drain(&mut app);
    assert_eq!(
        *calls.lock().unwrap(),
        [
            "read-lighting",
            "read-picture",
            "read-lighting",
            "read-picture"
        ]
    );
}

#[test]
fn a_failed_connection_read_can_retry_on_the_same_inventory() {
    struct FirstReadFails {
        memory: MemoryDevice,
        reads: Arc<AtomicUsize>,
    }
    impl Device for FirstReadFails {
        fn read(&mut self) -> Result<State, String> {
            if self.reads.fetch_add(1, Ordering::SeqCst) == 0 {
                Err("Temporary read failure".into())
            } else {
                self.memory.read()
            }
        }
        fn apply(
            &mut self,
            expected: &State,
            changes: &[Change],
            backup: &Path,
        ) -> Result<State, ApplyFailure> {
            self.memory.apply(expected, changes, backup)
        }
    }
    let reads = Arc::new(AtomicUsize::new(0));
    let observed = reads.clone();
    let mut app = App::new(
        Session::new(memory::demo().unwrap().descriptor().clone()).unwrap(),
        Box::new(move |_| {
            Ok((
                "demo".into(),
                Executor::spawn(
                    FirstReadFails {
                        memory: memory::demo()?,
                        reads: observed.clone(),
                    },
                    Default::default(),
                )
                .map_err(|error| error.to_string())?,
            ))
        }),
    );
    app.link
        .monitor(Discovery::spawn(|| Availability::Ready { id: "demo".into() }).unwrap());
    let _ = app.update(Message::Read);
    drain(&mut app);
    assert!(matches!(
        app.session.keymap().status(),
        Status::Unverified {
            problem: Problem::Read(_)
        }
    ));
    assert!(!app.session.requires_manual_read());
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.session.keymap().status() != &Status::Ready {
        let _ = app.update(Message::Scan);
        drain(&mut app);
        assert!(Instant::now() < deadline, "read did not retry");
        std::thread::yield_now();
    }
    assert_eq!(reads.load(Ordering::SeqCst), 2);
}

#[test]
fn assignment_search_and_physical_capture_stage_only_advertised_actions() {
    use crate::form::catalog::{InputMode, Message as Catalog};
    use iced::keyboard::key::Code;
    let (mut app, reads) = app(false);
    let _ = app.update(Message::Read);
    drain(&mut app);
    let _ = app.update(Message::Keys(keymap::Message::Key("Alpha".into())));
    let _ = app.update(Message::Keys(keymap::Message::Catalog(Catalog::Search(
        "b".into(),
    ))));
    let _ = app.update(Message::Keys(keymap::Message::Catalog(
        Catalog::SubmitSearch,
    )));
    assert_eq!(
        app.session.keymap().draft().unwrap()["Typing"]["Alpha"],
        Action::Key(5)
    );
    let _ = app.update(Message::Keys(keymap::Message::Catalog(Catalog::Capture)));
    let captured = input::catalog::capture(key_event(Code::KeyA, true)).unwrap();
    let _ = app.update(Message::Keys(keymap::Message::Catalog(captured)));
    assert_eq!(app.keys.catalog.input, InputMode::Browse);
    assert_eq!(
        app.session.keymap().draft().unwrap()["Typing"]["Alpha"],
        Action::Key(4)
    );
    assert!(input::catalog::capture(key_event(Code::KeyA, false)).is_none());
    for event in [
        key_event(Code::Escape, true),
        Event::Window(window::Event::Unfocused),
    ] {
        let _ = app.update(Message::Keys(keymap::Message::Catalog(Catalog::Capture)));
        let _ = app.update(Message::Keys(keymap::Message::Catalog(
            input::catalog::capture(event).unwrap(),
        )));
        assert_eq!(app.keys.catalog.input, InputMode::Browse);
        assert!(!app.session.keymap().dirty());
    }
    let _ = app.update(Message::Keys(keymap::Message::Catalog(Catalog::Capture)));
    let _ = app.update(Message::Keys(keymap::Message::Catalog(Catalog::Captured(
        0xff,
    ))));
    assert!(app.notice.contains("not supported"));
    assert!(!app.session.keymap().dirty());
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn shortcut_messages_stage_save_reload_and_reset_using_advertised_choices() {
    use crate::form::shortcut::Message as Shortcut;
    let (mut app, reads) = app(false);
    let _ = app.update(Message::Read);
    drain(&mut app);
    let caps = app.session.descriptor().shortcuts.as_ref().unwrap().clone();
    let modifier = caps.modifiers[0].usage;
    let second = caps.modifiers[1].usage;
    let key = caps.keys[1].usage;
    let _ = app.update(Message::Keys(keymap::Message::Key("Alpha".into())));
    for message in [
        Shortcut::ToggleModifier(modifier),
        Shortcut::SelectKey(key),
        Shortcut::Stage,
    ] {
        let _ = app.update(Message::Keys(keymap::Message::Shortcut(message)));
    }
    assert_eq!(
        app.session.keymap().draft().unwrap()["Typing"]["Alpha"],
        Action::Shortcut {
            modifiers: vec![modifier],
            key
        }
    );
    let _ = app.update(Message::Keys(keymap::Message::Shortcut(
        Shortcut::ToggleModifier(second),
    )));
    let _ = app.update(Message::Keys(keymap::Message::Shortcut(
        Shortcut::ToggleModifier(0xffff),
    )));
    assert_eq!(app.keys.shortcut.modifiers, [modifier, second]);
    assert!(app.keys.shortcut.error.is_some());
    assert!(
        app.notice.is_empty(),
        "shortcut validation stays in its form"
    );
    let _ = app.update(Message::Keys(keymap::Message::Shortcut(Shortcut::Stage)));
    assert!(app.keys.shortcut.error.is_none());
    let expected = Action::Shortcut {
        modifiers: vec![modifier, second],
        key,
    };
    assert_eq!(
        app.session.keymap().draft().unwrap()["Typing"]["Alpha"],
        expected
    );
    let _ = app.update(Message::Save);
    drain(&mut app);
    assert!(!app.session.keymap().dirty());
    assert_eq!(
        app.session.keymap().baseline().unwrap().bindings["Typing"]["Alpha"],
        expected
    );
    let _ = app.update(Message::Keys(keymap::Message::Key("Beta".into())));
    assert!(app.keys.shortcut.modifiers.is_empty());
    assert!(app.keys.shortcut.key.is_none());
    let _ = app.update(Message::Keys(keymap::Message::Key("Alpha".into())));
    assert_eq!(app.keys.shortcut.modifiers, [modifier, second]);
    assert_eq!(app.keys.shortcut.key, Some(key));
    let _ = app.update(Message::Keys(keymap::Message::Layer("Navigation".into())));
    assert!(app.keys.shortcut.modifiers.is_empty());
    assert_eq!(
        reads.load(Ordering::SeqCst),
        1,
        "staging and navigation never reread the device"
    );
}

#[test]
fn device_work_cancels_key_capture_instead_of_rearming_it_after_completion() {
    use crate::form::catalog::{InputMode, Message as Catalog};
    let (mut app, _) = app(false);
    let _ = app.update(Message::Read);
    drain(&mut app);
    edit(&mut app);
    let _ = app.update(Message::Keys(keymap::Message::Catalog(Catalog::Capture)));
    assert_eq!(app.keys.catalog.input, InputMode::Capture);
    let _ = app.update(Message::Save);
    assert_eq!(app.keys.catalog.input, InputMode::Browse);
    drain(&mut app);
    let saved = app.session.keymap().draft().unwrap().clone();
    let _ = app.update(Message::Keys(keymap::Message::Catalog(Catalog::Captured(
        4,
    ))));
    assert_eq!(app.session.keymap().draft(), Some(&saved));
    assert!(!app.session.keymap().dirty());
}
