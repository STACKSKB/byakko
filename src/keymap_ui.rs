//! Reusable native keymap editor. This module knows no device transport,
//! firmware packet, fixed matrix size, or fixed layer count.
use crate::backend::{self, Action, Change, Descriptor, KeymapBackend, State};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
};

pub struct KeymapEditor {
    backend: Arc<dyn KeymapBackend>,
    descriptor: Descriptor,
    observed: Option<State>,
    draft: BTreeMap<String, BTreeMap<String, Action>>,
    selected: Option<String>,
    layer: String,
    search: String,
    modifiers: [bool; 4],
    busy: bool,
    status: String,
    error: bool,
    backup_dir: PathBuf,
    tx: Sender<Result<State, String>>,
    rx: Receiver<Result<State, String>>,
}

impl KeymapEditor {
    pub fn new(backend: Arc<dyn KeymapBackend>, backup_dir: PathBuf) -> Self {
        let descriptor = backend.descriptor();
        let layer = descriptor
            .layers
            .first()
            .map(|l| l.id.clone())
            .unwrap_or_default();
        let (tx, rx) = mpsc::channel();
        Self {
            backend,
            descriptor,
            observed: None,
            draft: BTreeMap::new(),
            selected: None,
            layer,
            search: String::new(),
            modifiers: [false; 4],
            busy: false,
            status: "Waiting for device state".into(),
            error: false,
            backup_dir,
            tx,
            rx,
        }
    }

    pub fn observed(&self) -> Option<&State> {
        self.observed.as_ref()
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
    pub fn dirty_count(&self) -> usize {
        self.changes().len()
    }

    pub fn load(&mut self, state: State) -> Result<(), String> {
        backend::validate_state(&self.descriptor, &state)?;
        self.draft = state.bindings.clone();
        self.observed = Some(state);
        self.error = false;
        self.status = "Select a key, choose an action, then review and apply.".into();
        Ok(())
    }

    pub fn stage_state(&mut self, desired: State) -> Result<(), String> {
        if self.busy {
            return Err("A keymap operation is running".into());
        }
        backend::validate_state(&self.descriptor, &desired)?;
        let expected = self.observed.as_ref().ok_or("Read the keyboard first")?;
        let changes = differences(expected, &desired.bindings);
        self.backend.validate(expected, &changes)?;
        self.draft = desired.bindings;
        Ok(())
    }

    pub fn changes(&self) -> Vec<Change> {
        self.observed
            .as_ref()
            .map(|s| differences(s, &self.draft))
            .unwrap_or_default()
    }

    fn stage(&mut self, action: Action) -> Result<(), String> {
        let key = self.selected.clone().ok_or("Select a key first")?;
        let expected = self.observed.as_ref().ok_or("Read the keyboard first")?;
        let change = Change {
            layer: self.layer.clone(),
            key: key.clone(),
            action: action.clone(),
        };
        self.backend.validate(expected, &[change])?;
        self.draft
            .get_mut(&self.layer)
            .ok_or("Unknown layer")?
            .insert(key, action);
        self.error = false;
        self.status = "Change staged locally.".into();
        Ok(())
    }

    pub fn start_apply(&mut self, ctx: &egui::Context) {
        if self.busy || self.dirty_count() == 0 {
            return;
        }
        let Some(expected) = self.observed.clone() else {
            return;
        };
        let changes = self.changes();
        if let Err(error) = self.backend.validate(&expected, &changes) {
            self.status = error;
            self.error = true;
            return;
        }
        self.busy = true;
        self.status = format!("Applying and verifying {} key changes…", changes.len());
        let backend = self.backend.clone();
        let backup_dir = self.backup_dir.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| backend.apply(&expected, &changes, &backup_dir)))
                .unwrap_or_else(|_| Err("Backend apply panicked; device state is unverified. Inspect the backup before retrying.".into()));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    pub fn poll(&mut self) -> Option<State> {
        let mut completed = None;
        while let Ok(result) = self.rx.try_recv() {
            self.busy = false;
            match result.and_then(|state| {
                backend::validate_state(&self.descriptor, &state)?;
                Ok(state)
            }) {
                Ok(state) => {
                    if state.bindings != self.draft {
                        self.status = "Backend returned unexpected bindings; device state is unverified. Draft retained.".into();
                        self.error = true;
                        continue;
                    }
                    // Retain the draft until a validated backend result exists.
                    self.load(state.clone()).expect("state just validated");
                    self.status = "Applied and verified.".into();
                    completed = Some(state);
                }
                Err(error) => {
                    self.status = error;
                    self.error = true;
                }
            }
        }
        completed
    }

    pub fn handle_close(&self, ctx: &egui::Context) {
        if self.busy && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }

    fn action_label(&self, action: &Action) -> String {
        if let Some(choice) = self.descriptor.actions.iter().find(|c| &c.action == action) {
            return choice.label.clone();
        }
        match action {
            Action::Disabled => "Disabled".into(),
            Action::Key(usage) => format!("Key {usage}"),
            Action::Macro { slot, mode } => format!("Macro {slot} · mode {mode}"),
            Action::Named { id } => id.clone(),
            Action::Opaque { label, .. } => label.clone(),
            Action::Shortcut { modifiers, key } => format!("Shortcut {modifiers:?} + key {key}"),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, blocked: bool) {
        self.handle_close(ui.ctx());
        let editable = !blocked && !self.busy && self.observed.is_some();
        ui.horizontal(|ui| {
            ui.label("LAYER");
            for layer in &self.descriptor.layers {
                if ui
                    .add_enabled(
                        editable,
                        egui::Button::new(&layer.label).selected(self.layer == layer.id),
                    )
                    .clicked()
                {
                    self.layer = layer.id.clone();
                }
            }
            ui.label(format!("{} staged changes", self.dirty_count()));
        });
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width((ui.available_width() - 290.0).max(300.0));
                let keys: Vec<_> = self.descriptor.keys.iter().filter(|k| k.visible).collect();
                let min_x = keys.iter().map(|k| k.x).fold(0.0_f32, f32::min);
                let min_y = keys.iter().map(|k| k.y).fold(0.0_f32, f32::min);
                let width = keys.iter().map(|k| k.x + k.width).fold(1.0_f32, f32::max) - min_x;
                let height = keys.iter().map(|k| k.y + k.height).fold(1.0_f32, f32::max) - min_y;
                let unit = (ui.available_width() / width).min(50.0);
                let (canvas, _) =
                    ui.allocate_exact_size(Vec2::new(width * unit, height * unit), Sense::hover());
                for key in keys {
                    let rect = Rect::from_min_size(
                        Pos2::new(
                            canvas.left() + (key.x - min_x) * unit,
                            canvas.top() + (key.y - min_y) * unit,
                        ),
                        Vec2::new(key.width * unit - 2.0, key.height * unit - 2.0),
                    );
                    let response = ui.interact(rect, ui.id().with(&key.id), Sense::click());
                    if response.clicked() && editable {
                        self.selected = Some(key.id.clone());
                    }
                    let selected = self.selected.as_ref() == Some(&key.id);
                    ui.painter().rect_filled(
                        rect,
                        3.0,
                        if selected {
                            Color32::from_rgb(250, 225, 192)
                        } else {
                            Color32::from_rgb(252, 251, 246)
                        },
                    );
                    ui.painter().rect_stroke(
                        rect,
                        3.0,
                        Stroke::new(1.0, Color32::GRAY),
                        StrokeKind::Inside,
                    );
                    ui.painter().text(
                        rect.center_top() + Vec2::new(0.0, 8.0),
                        Align2::CENTER_TOP,
                        &key.label,
                        FontId::proportional(12.0),
                        Color32::BLACK,
                    );
                    if let Some(action) = self.draft.get(&self.layer).and_then(|b| b.get(&key.id)) {
                        response.on_hover_text(self.action_label(action));
                    }
                }
            });
            ui.vertical(|ui| {
                ui.set_width(270.0);
                let selected = self
                    .selected
                    .as_ref()
                    .and_then(|id| self.descriptor.keys.iter().find(|k| &k.id == id));
                let writable = selected.is_some_and(|key| key.writable);
                ui.heading(selected.map(|k| k.label.as_str()).unwrap_or("Choose a key"));
                if let Some(action) = self
                    .selected
                    .as_ref()
                    .and_then(|id| self.draft.get(&self.layer)?.get(id))
                {
                    ui.label(self.action_label(action));
                }
                if selected.is_some() && !writable {
                    ui.label("This key is read-only.");
                }
                ui.text_edit_singleline(&mut self.search);
                ui.horizontal(|ui| {
                    for (index, label) in ["Ctrl", "Shift", "Alt", "Win"].into_iter().enumerate() {
                        ui.checkbox(&mut self.modifiers[index], label);
                    }
                });
                let query = self.search.to_lowercase();
                let choices: Vec<_> = self
                    .descriptor
                    .actions
                    .iter()
                    .filter(|c| c.label.to_lowercase().contains(&query))
                    .cloned()
                    .collect();
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for choice in choices {
                            if ui
                                .add_enabled(editable && writable, egui::Button::new(&choice.label))
                                .clicked()
                            {
                                let action = match choice.action {
                                    Action::Key(key) if self.modifiers.iter().any(|&m| m) => {
                                        Action::Shortcut {
                                            modifiers: self
                                                .modifiers
                                                .iter()
                                                .enumerate()
                                                .filter(|(_, enabled)| **enabled)
                                                .map(|(i, _)| 224 + i as u16)
                                                .collect(),
                                            key,
                                        }
                                    }
                                    other => other,
                                };
                                if let Err(error) = self.stage(action) {
                                    self.status = error;
                                    self.error = true;
                                }
                            }
                        }
                    });
            });
        });
        egui::ScrollArea::vertical()
            .max_height(100.0)
            .show(ui, |ui| {
                for change in self.changes() {
                    ui.monospace(format!(
                        "{} / {} → {}",
                        change.layer,
                        change.key,
                        self.action_label(&change.action)
                    ));
                }
            });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    editable && self.dirty_count() > 0,
                    egui::Button::new("APPLY TO KEYBOARD  Ctrl+S"),
                )
                .clicked()
            {
                self.start_apply(ui.ctx());
            }
            if ui
                .add_enabled(
                    editable && self.dirty_count() > 0,
                    egui::Button::new("REVERT DRAFT"),
                )
                .clicked()
                && let Some(state) = self.observed.clone()
            {
                let _ = self.load(state);
            }
            ui.colored_label(
                if self.error {
                    Color32::from_rgb(180, 50, 30)
                } else {
                    Color32::DARK_GRAY
                },
                &self.status,
            );
        });
        self.handle_close(ui.ctx());
    }
}

fn differences(
    observed: &State,
    draft: &BTreeMap<String, BTreeMap<String, Action>>,
) -> Vec<Change> {
    let mut changes = Vec::new();
    for (layer, bindings) in draft {
        for (key, action) in bindings {
            if observed.bindings.get(layer).and_then(|b| b.get(key)) != Some(action) {
                changes.push(Change {
                    layer: layer.clone(),
                    key: key.clone(),
                    action: action.clone(),
                });
            }
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{ActionChoice, Layer, PhysicalKey};
    use std::{path::Path, sync::Mutex};

    struct FakeBackend {
        state: Mutex<State>,
    }
    impl FakeBackend {
        fn new() -> Self {
            let bindings = ["alpha", "symbols", "navigation"]
                .into_iter()
                .map(|layer| {
                    (
                        layer.into(),
                        (0..12)
                            .map(|key| (format!("pad-{key}"), Action::Key(4)))
                            .collect(),
                    )
                })
                .collect();
            Self {
                state: Mutex::new(State {
                    revision: vec![0],
                    bindings,
                }),
            }
        }
    }
    impl KeymapBackend for FakeBackend {
        fn descriptor(&self) -> Descriptor {
            Descriptor {
                backend_id: "simulation".into(),
                device_name: "Twelve-pad controller".into(),
                keys: (0..12)
                    .map(|i| PhysicalKey {
                        id: format!("pad-{i}"),
                        label: format!("Pad {i}"),
                        x: (i % 4) as f32,
                        y: (i / 4) as f32,
                        width: 1.0,
                        height: 1.0,
                        visible: true,
                        writable: i != 11,
                    })
                    .collect(),
                layers: ["alpha", "symbols", "navigation"]
                    .into_iter()
                    .map(|id| Layer {
                        id: id.into(),
                        label: id.into(),
                    })
                    .collect(),
                actions: vec![
                    ActionChoice {
                        label: "A".into(),
                        action: Action::Key(4),
                    },
                    ActionChoice {
                        label: "B".into(),
                        action: Action::Key(5),
                    },
                ],
            }
        }
        fn read(&self) -> Result<State, String> {
            Ok(self.state.lock().unwrap().clone())
        }
        fn validate(&self, expected: &State, changes: &[Change]) -> Result<(), String> {
            backend::validate_state(&self.descriptor(), expected)?;
            backend::validate_changes(&self.descriptor(), changes)?;
            if changes
                .iter()
                .any(|c| !matches!(c.action, Action::Key(4 | 5)))
            {
                return Err("unsupported simulated action".into());
            }
            Ok(())
        }
        fn apply(&self, expected: &State, changes: &[Change], _: &Path) -> Result<State, String> {
            self.validate(expected, changes)?;
            let mut state = self.state.lock().unwrap();
            if &*state != expected {
                return Err("stale simulated revision".into());
            }
            for change in changes {
                state
                    .bindings
                    .get_mut(&change.layer)
                    .unwrap()
                    .insert(change.key.clone(), change.action.clone());
            }
            state.revision[0] += 1;
            Ok(state.clone())
        }
    }

    fn editor() -> (KeymapEditor, Arc<FakeBackend>) {
        let backend = Arc::new(FakeBackend::new());
        let mut editor =
            KeymapEditor::new(backend.clone(), PathBuf::from("unused-simulation-backups"));
        editor.load(backend.read().unwrap()).unwrap();
        (editor, backend)
    }

    #[test]
    fn renders_twelve_keys_and_three_backend_layers_without_nia87_geometry() {
        let (mut editor, _) = editor();
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 700.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| editor.ui(ui, false));
        output.textures_delta.clear();
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::epaint::Shape::Text(t) => Some(t.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        for label in ["Pad 0", "Pad 11", "alpha", "symbols", "navigation"] {
            assert!(texts.contains(&label), "missing rendered {label}");
        }
        assert_eq!(editor.descriptor.keys.len(), 12);
    }

    #[test]
    fn edits_third_layer_and_retains_draft_on_conflict_or_bad_readback() {
        let (mut editor, backend) = editor();
        editor.layer = "navigation".into();
        editor.selected = Some("pad-3".into());
        editor.stage(Action::Key(5)).unwrap();
        let before = editor.observed().unwrap().clone();
        let changes = editor.changes();
        assert_eq!(changes[0].layer, "navigation");
        let applied = backend
            .apply(&before, &changes, Path::new("unused"))
            .unwrap();
        assert!(
            backend
                .apply(&before, &changes, Path::new("unused"))
                .unwrap_err()
                .contains("stale")
        );
        editor.busy = true;
        editor
            .tx
            .send(Err("stale simulated revision".into()))
            .unwrap();
        assert!(editor.poll().is_none());
        assert_eq!(editor.dirty_count(), 1);
        editor.tx.send(Ok(before)).unwrap();
        assert!(editor.poll().is_none());
        assert!(editor.error);
        assert_eq!(editor.dirty_count(), 1);
        editor.tx.send(Ok(applied)).unwrap();
        assert!(editor.poll().is_some());
        assert_eq!(editor.dirty_count(), 0);
    }

    #[test]
    fn unsupported_actions_and_read_only_keys_leave_draft_unchanged() {
        let (mut editor, _) = editor();
        editor.selected = Some("pad-11".into());
        assert!(editor.stage(Action::Key(5)).is_err());
        editor.selected = Some("pad-0".into());
        assert!(editor.stage(Action::Macro { slot: 0, mode: 0 }).is_err());
        assert_eq!(editor.dirty_count(), 0);
    }
}
