//! Synthetic Nia87 picture transaction, with no hardware access.
use super::*;
use byakko_core::model::{keymap::Descriptor, picture as rgb};
use byakko_devices::nia87::looks::Look;
use std::collections::BTreeMap;

struct LookStorage {
    picture: rgb::Snapshot,
    calls: Vec<&'static str>,
    fail_apply: bool,
}

struct LookDevice {
    descriptor: Descriptor,
    storage: Arc<Mutex<LookStorage>>,
}

impl Device for LookDevice {
    fn read(&mut self) -> Result<State, String> {
        let keys: BTreeMap<String, Action> = self
            .descriptor
            .keys
            .iter()
            .map(|key| (key.id.clone(), Action::Disabled))
            .collect();
        let bindings = self
            .descriptor
            .layers
            .iter()
            .map(|layer| (layer.id.clone(), keys.clone()))
            .collect();
        Ok(State {
            revision: vec![1],
            bindings,
        })
    }

    fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
        Err(ApplyFailure {
            message: "No keymap write in this fixture".into(),
            recovery: Recovery::NotAttempted,
        })
    }

    fn read_picture(&mut self) -> Result<rgb::Snapshot, String> {
        let mut state = self.storage.lock().unwrap();
        state.calls.push("read-picture");
        let mut snapshot = state.picture.clone();
        snapshot.evidence = rgb::Evidence::Readback;
        Ok(snapshot)
    }

    fn apply_picture(
        &mut self,
        before: &rgb::Snapshot,
        desired: &BTreeMap<String, [u8; 3]>,
        _: &Path,
    ) -> Result<rgb::Snapshot, ApplyFailure> {
        let mut state = self.storage.lock().unwrap();
        assert_eq!(before, &state.picture);
        state.calls.push("apply-picture");
        if state.fail_apply {
            return Err(ApplyFailure {
                message: "Synthetic picture rejection".into(),
                recovery: Recovery::NotAttempted,
            });
        }
        state.picture.content = rgb::Content::Editable(desired.clone());
        state.picture.revision.push(1);
        state.picture.evidence = rgb::Evidence::TransportAccepted;
        Ok(state.picture.clone())
    }
}

fn looks_app() -> (App, Arc<Mutex<LookStorage>>) {
    let source = byakko_devices::nia87::application::session().unwrap();
    let descriptor = source.descriptor().clone();
    let mut caps = source.picture().unwrap().capabilities().clone();
    caps.lighting_effect = None;
    let session = Session::new(descriptor.clone())
        .unwrap()
        .with_picture(caps.clone())
        .unwrap();
    let storage = Arc::new(Mutex::new(LookStorage {
        picture: rgb::Snapshot {
            backend_id: "nia87".into(),
            revision: vec![1],
            context_revision: vec![],
            evidence: rgb::Evidence::Readback,
            content: rgb::Content::Editable(
                caps.keys
                    .iter()
                    .map(|key| (key.clone(), [1, 2, 3]))
                    .collect(),
            ),
        },
        calls: vec![],
        fail_apply: false,
    }));
    let worker_storage = storage.clone();
    let mut app = App::new(
        session,
        Box::new(move |_| {
            Ok((
                "synthetic Nia87".into(),
                Executor::spawn(
                    LookDevice {
                        descriptor: descriptor.clone(),
                        storage: worker_storage.clone(),
                    },
                    Default::default(),
                )
                .map_err(|error| error.to_string())?,
            ))
        }),
    );
    let _ = app.update(Message::Read);
    drain(&mut app);
    (app, storage)
}

#[test]
fn looks_preview_is_local_and_apply_uses_one_picture_write() {
    let (mut app, storage) = looks_app();
    let before = app.session.picture().unwrap().draft().unwrap().clone();
    assert_eq!(storage.lock().unwrap().calls, ["read-picture"]);
    let _ = app.update(Message::Picture(picture::Message::Preview(Look::Rainbow)));
    assert!(app.picture.preview_look().is_some());
    assert_eq!(app.session.picture().unwrap().draft(), Some(&before));
    assert_eq!(storage.lock().unwrap().calls, ["read-picture"]);
    let _ = app.update(Message::Picture(picture::Message::RevertPreview));
    assert!(app.picture.preview_look().is_none());
    assert_eq!(app.session.picture().unwrap().draft(), Some(&before));
    let _ = app.update(Message::Picture(picture::Message::Preview(Look::Rainbow)));
    let _ = app.update(Message::Picture(picture::Message::ApplyPreview));
    drain(&mut app);
    assert_eq!(
        storage.lock().unwrap().calls,
        ["read-picture", "apply-picture"]
    );
    assert_eq!(app.notice, "Look applied.");
    assert!(!app.session.picture().unwrap().dirty());
    assert_eq!(
        app.session.picture().unwrap().draft(),
        Some(&Look::Rainbow.colors([255, 32, 96]))
    );
}

#[test]
fn pending_manual_paint_is_kept_and_does_not_trigger_a_second_look_write() {
    let (mut app, storage) = looks_app();
    app.config.auto_save_delay = Duration::from_secs(60);
    let key = app.session.picture().unwrap().capabilities().keys[0].clone();
    let _ = app.update(Message::Picture(picture::Message::Mode(
        picture::Mode::Paint,
    )));
    let _ = app.update(Message::Picture(picture::Message::Brush([17, 18, 19])));
    let _ = app.update(Message::Picture(picture::Message::Paint(key.clone())));
    assert_eq!(
        app.session.picture().unwrap().draft().unwrap()[&key],
        [17, 18, 19]
    );
    let _ = app.update(Message::Picture(picture::Message::Preview(Look::Rainbow)));
    assert_eq!(storage.lock().unwrap().calls, ["read-picture"]);
    let _ = app.update(Message::Picture(picture::Message::ApplyPreview));
    drain(&mut app);
    assert_eq!(
        storage.lock().unwrap().calls,
        ["read-picture", "apply-picture"]
    );
    assert_eq!(
        app.session.picture().unwrap().draft().unwrap()[&key],
        [17, 18, 19]
    );
    elapsed(&mut app);
    drain(&mut app);
    assert_eq!(
        storage.lock().unwrap().calls,
        ["read-picture", "apply-picture"]
    );
}

#[test]
fn failed_look_write_keeps_preview_and_does_not_auto_retry() {
    let (mut app, storage) = looks_app();
    storage.lock().unwrap().fail_apply = true;
    let before = app.session.picture().unwrap().draft().unwrap().clone();
    let _ = app.update(Message::Picture(picture::Message::Preview(Look::NightDesk)));
    let _ = app.update(Message::Picture(picture::Message::ApplyPreview));
    drain(&mut app);
    assert_eq!(
        storage.lock().unwrap().calls,
        ["read-picture", "apply-picture"]
    );
    assert_eq!(app.picture.preview_look(), Some(Look::NightDesk));
    assert!(!app.picture.applying_preview());
    assert!(matches!(
        app.session.picture().unwrap().status(),
        Status::Unverified { .. }
    ));
    assert_eq!(
        storage.lock().unwrap().picture.content,
        rgb::Content::Editable(before)
    );
    elapsed(&mut app);
    drain(&mut app);
    assert_eq!(
        storage.lock().unwrap().calls,
        ["read-picture", "apply-picture"]
    );
}
