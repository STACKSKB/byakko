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

#[test]
fn looks_gallery_reflows_without_splitting_names_or_losing_tiles() {
    use super::notifications::RenderedText;
    use iced::advanced::{Layout, layout::Limits, widget::Tree};
    let (app, _) = looks_app();
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(16.0));
    for width in [280.0, 354.0, 540.0, 880.0] {
        let mut gallery = crate::view::picture::view(
            &app.picture,
            app.session.descriptor(),
            app.session.picture().unwrap(),
            true,
            true,
            &app.style,
        );
        let mut tree = Tree::new(&gallery);
        let node = gallery.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &Limits::new(iced::Size::ZERO, iced::Size::new(width, 2000.0)),
        );
        let mut texts = RenderedText::default();
        gallery
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut texts);
        let tiles: Vec<_> = byakko_devices::nia87::looks::ALL
            .iter()
            .map(|look| {
                let (_, bounds) = texts
                    .0
                    .iter()
                    .find(|(label, _)| label == look.name())
                    .expect("advertised Look present in layout");
                assert!(
                    bounds.height < 24.0 && bounds.x + bounds.width <= width,
                    "{width}: {look:?}: {bounds:?}"
                );
                bounds
            })
            .collect();
        if width == 280.0 {
            assert!(tiles[1].y >= tiles[0].y + tiles[0].height);
        } else {
            assert_eq!(tiles[0].y, tiles[1].y);
        }
    }
}
