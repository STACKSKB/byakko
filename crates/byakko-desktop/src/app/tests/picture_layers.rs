//! A selector-backed device fixture: each layer has independent persistent colors.
use super::*;
use byakko_core::model::{lighting as light, picture as rgb};
use std::collections::BTreeMap;

struct Storage {
    lighting: light::Snapshot,
    layers: Vec<rgb::Snapshot>,
    calls: Vec<String>,
    fail_selector: bool,
}

impl Storage {
    fn selected(&self) -> usize {
        usize::from(self.lighting.picture_context[0])
    }
}

struct LayerDevice {
    memory: MemoryDevice,
    storage: Arc<Mutex<Storage>>,
}

impl Device for LayerDevice {
    fn read(&mut self) -> Result<State, String> {
        self.memory.read()
    }

    fn apply(
        &mut self,
        before: &State,
        edits: &[Change],
        backup: &Path,
    ) -> Result<State, ApplyFailure> {
        self.memory.apply(before, edits, backup)
    }

    fn read_lighting(&mut self) -> Result<light::Snapshot, String> {
        let mut state = self.storage.lock().unwrap();
        state.calls.push("read-lighting".into());
        Ok(light::Snapshot {
            evidence: light::Evidence::Readback,
            ..state.lighting.clone()
        })
    }

    fn apply_lighting(
        &mut self,
        before: &light::Snapshot,
        desired: &light::Setting,
        _: &Path,
    ) -> Result<light::Snapshot, ApplyFailure> {
        let mut state = self.storage.lock().unwrap();
        let mut current = state.lighting.clone();
        current.evidence = before.evidence;
        assert_eq!(before, &current);
        let index = match desired.option.as_deref() {
            Some("layer-1") => 0,
            Some("layer-2") => 1,
            Some("layer-3") => 2,
            other => panic!("unexpected selector {other:?}"),
        };
        state.calls.push(format!("select-{}", index + 1));
        if state.fail_selector {
            return Err(ApplyFailure {
                message: "Selector write failed".into(),
                recovery: Recovery::Verified,
            });
        }
        state.lighting.content = light::Content::Editable(desired.clone());
        state.lighting.picture_context = vec![index];
        state.lighting.revision.push(index);
        state.lighting.evidence = light::Evidence::TransportAccepted;
        Ok(state.lighting.clone())
    }

    fn read_picture(&mut self) -> Result<rgb::Snapshot, String> {
        let mut state = self.storage.lock().unwrap();
        let index = state.selected();
        state.calls.push(format!("read-layer-{}", index + 1));
        Ok(rgb::Snapshot {
            evidence: rgb::Evidence::Readback,
            ..state.layers[index].clone()
        })
    }

    fn apply_picture(
        &mut self,
        before: &rgb::Snapshot,
        colors: &BTreeMap<String, [u8; 3]>,
        _: &Path,
    ) -> Result<rgb::Snapshot, ApplyFailure> {
        let mut state = self.storage.lock().unwrap();
        let index = state.selected();
        let mut current = state.layers[index].clone();
        current.evidence = before.evidence;
        assert_eq!(
            before, &current,
            "an upload must use the selected layer's before-image"
        );
        state.calls.push(format!("upload-layer-{}", index + 1));
        let layer = &mut state.layers[index];
        layer.content = rgb::Content::Editable(colors.clone());
        layer.revision.push(1);
        layer.evidence = rgb::Evidence::TransportAccepted;
        Ok(layer.clone())
    }
}

fn layered_app() -> (App, Arc<Mutex<Storage>>) {
    let device = memory::demo().unwrap();
    let mut caps = device.lighting_capabilities().unwrap().clone();
    caps.effects.retain(|effect| effect.id == "picture");
    caps.effects[0].options = (1..=3)
        .map(|index| light::Choice {
            id: format!("layer-{index}"),
            label: format!("Layer {index}"),
        })
        .collect();
    let session = Session::new(device.descriptor().clone())
        .unwrap()
        .with_lighting(caps)
        .unwrap()
        .with_picture(device.picture_capabilities().unwrap().clone())
        .unwrap();
    let storage = Arc::new(Mutex::new(Storage {
        lighting: light::Snapshot {
            backend_id: "memory".into(),
            revision: vec![10],
            picture_context: vec![0],
            evidence: light::Evidence::Readback,
            content: light::Content::Editable(light::Setting {
                effect: "picture".into(),
                brightness: Some(3),
                speed: None,
                option: Some("layer-1".into()),
                color: None,
            }),
        },
        layers: (0..3)
            .map(|index| rgb::Snapshot {
                backend_id: "memory".into(),
                revision: vec![20, index],
                context_revision: vec![index],
                evidence: rgb::Evidence::Readback,
                content: rgb::Content::Editable(BTreeMap::from([
                    ("Alpha".into(), [index + 1, 10, 20]),
                    ("Beta".into(), [index + 1, 30, 40]),
                ])),
            })
            .collect(),
        calls: vec![],
        fail_selector: false,
    }));
    let worker_storage = storage.clone();
    let mut app = App::new(
        session,
        Box::new(move |_| {
            Ok((
                "layers".into(),
                Executor::spawn(
                    LayerDevice {
                        memory: memory::demo()?,
                        storage: worker_storage.clone(),
                    },
                    Default::default(),
                )
                .map_err(|error| error.to_string())?,
            ))
        }),
    );
    app.config.short_edit_delay = Duration::from_secs(60);
    app.config.auto_save_delay = Duration::from_secs(60);
    let _ = app.update(Message::Read);
    drain(&mut app);
    let _ = app.update(Message::Lighting(lighting::Message::Mode(
        lighting::Mode::PerKey,
    )));
    drain(&mut app);
    assert!(picture_is_displayed(&app.session));
    storage.lock().unwrap().calls.clear();
    (app, storage)
}

fn select(app: &mut App, layer: u8) {
    let _ = app.update(Message::Lighting(lighting::Message::Edit(
        light::Edit::Option(format!("layer-{layer}")),
    )));
}

#[test]
fn layer_switches_load_target_once_and_upload_only_selected_colors() {
    let (mut app, storage) = layered_app();
    let _ = app.update(Message::Picture(picture::Message::Select("Alpha".into())));
    for layer in [2, 3, 1] {
        select(&mut app, layer);
        elapsed(&mut app);
        drain(&mut app);
        assert_eq!(
            app.session.picture().unwrap().draft().unwrap()["Alpha"],
            [layer, 10, 20]
        );
        assert_eq!(
            app.picture.color(app.session.picture().unwrap()),
            Some([layer, 10, 20])
        );
        assert!(!app.session.picture().unwrap().dirty());
    }
    assert_eq!(
        storage.lock().unwrap().calls,
        [
            "select-2",
            "read-layer-2",
            "select-3",
            "read-layer-3",
            "select-1",
            "read-layer-1"
        ]
    );
    let mut expected = [[1, 10, 20], [2, 10, 20], [3, 10, 20]];
    for layer in [2, 3, 1] {
        select(&mut app, layer);
        elapsed(&mut app);
        drain(&mut app);
        storage.lock().unwrap().calls.clear();
        let color = [layer, 80, 70];
        let _ = app.update(Message::Picture(picture::Message::Color(color)));
        elapsed(&mut app);
        drain(&mut app);
        expected[usize::from(layer - 1)] = color;
        let state = storage.lock().unwrap();
        assert_eq!(state.calls, [format!("upload-layer-{layer}")]);
        for (index, expected) in expected.iter().enumerate() {
            let rgb::Content::Editable(colors) = &state.layers[index].content else {
                panic!()
            };
            assert_eq!(&colors["Alpha"], expected);
            assert_eq!(colors["Beta"], [index as u8 + 1, 30, 40]);
        }
    }
    for layer in [2, 3, 1] {
        storage.lock().unwrap().calls.clear();
        select(&mut app, layer);
        elapsed(&mut app);
        drain(&mut app);
        assert_eq!(
            storage.lock().unwrap().calls,
            [format!("select-{layer}"), format!("read-layer-{layer}")]
        );
        let editor = app.session.picture().unwrap();
        assert_eq!(editor.status(), &Status::Ready);
        assert_eq!(editor.baseline().unwrap().evidence, rgb::Evidence::Readback);
        assert_eq!(
            editor.draft().unwrap()["Alpha"],
            expected[usize::from(layer - 1)]
        );
        assert_eq!(
            app.picture.color(editor),
            Some(expected[usize::from(layer - 1)])
        );
    }
}

#[test]
fn dirty_layer_switch_is_rejected_and_retains_colors() {
    let (mut app, storage) = layered_app();
    let _ = app.update(Message::Picture(picture::Message::Select("Alpha".into())));
    let _ = app.update(Message::Picture(picture::Message::Color([9, 8, 7])));
    select(&mut app, 2);
    assert_eq!(
        app.session
            .lighting()
            .unwrap()
            .draft()
            .unwrap()
            .option
            .as_deref(),
        Some("layer-1")
    );
    assert_eq!(
        app.session.picture().unwrap().draft().unwrap()["Alpha"],
        [9, 8, 7]
    );
    assert!(app.session.picture().unwrap().dirty());
    assert!(storage.lock().unwrap().calls.is_empty());
}

#[test]
fn layer_choices_coalesce_before_one_selector_write_and_picture_read() {
    let (mut app, storage) = layered_app();
    select(&mut app, 2);
    select(&mut app, 3);
    assert!(storage.lock().unwrap().calls.is_empty());
    elapsed(&mut app);
    drain(&mut app);
    assert_eq!(storage.lock().unwrap().calls, ["select-3", "read-layer-3"]);
    assert_eq!(
        app.session.picture().unwrap().draft().unwrap()["Alpha"],
        [3, 10, 20]
    );
}

#[test]
fn failed_selector_does_not_load_or_upload_under_the_wrong_layer_label() {
    let (mut app, storage) = layered_app();
    let _ = app.update(Message::Picture(picture::Message::Select("Alpha".into())));
    storage.lock().unwrap().fail_selector = true;
    select(&mut app, 2);
    elapsed(&mut app);
    drain(&mut app);
    let _ = app.update(Message::Picture(picture::Message::Color([9, 8, 7])));
    elapsed(&mut app);
    drain(&mut app);
    assert_eq!(storage.lock().unwrap().calls, ["select-2"]);
    assert_eq!(storage.lock().unwrap().selected(), 0);
    assert_eq!(
        app.session.picture().unwrap().draft().unwrap()["Alpha"],
        [1, 10, 20]
    );
}
