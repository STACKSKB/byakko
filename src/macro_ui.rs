//! Native macro editor. Device transactions run on worker threads.

use std::{
    num::NonZeroU16,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
};

use eframe::egui::{self, Color32, RichText};

use crate::{
    device, layout,
    macro_file::{self, MacroFile},
    macro_recorder::{DelayPolicy, Recorder},
    macro_state::MacroState,
    macros::{self, Macro, MacroEvent},
};

const INK: Color32 = Color32::from_rgb(33, 42, 46);
const MUTED: Color32 = Color32::from_rgb(96, 107, 109);
const PANEL: Color32 = Color32::from_rgb(252, 251, 246);
const ACCENT: Color32 = Color32::from_rgb(199, 91, 45);

fn macro_worker<T>(work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|_| {
        Err("Macro worker panicked; device state and restoration are unverified. Inspect the backup before retrying.".into())
    })
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn non_counted_modes_require_explicit_count_change_before_apply() {
        for mode in [1, 2] {
            let mut editor = MacroEditor::new();
            editor.state.seed_verified(editor.state.draft().clone());
            editor.state.draft_mut().repeat_count = 5;
            editor.play_modes[0] = mode;
            let draft = editor.state.draft().clone();
            editor.apply(&egui::Context::default());
            assert!(!editor.busy() && editor.error);
            assert_eq!(editor.state.draft(), &draft);
            assert!(!editor.playback_count_valid());
            editor.state.draft_mut().repeat_count = 1;
            assert!(editor.playback_count_valid());
            editor.play_modes[0] = 0;
            editor.state.draft_mut().repeat_count = 5;
            assert!(editor.playback_count_valid());
        }
    }

    #[test]
    fn local_labels_survive_editor_restart_without_loading_device_data() {
        let directory = std::env::temp_dir().join(format!(
            "byakko-label-editor-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut first = MacroEditor::new();
        first.load_local_labels(directory.clone());
        first.names[49] = "Editor shortcuts".into();
        let draft = first.state.draft().clone();
        first.save_labels();
        assert!(!first.labels_error);
        assert_eq!(first.names, first.saved_names);
        assert_eq!(first.state.draft(), &draft);
        assert!(!first.state.loaded());
        let mut second = MacroEditor::new();
        second.load_local_labels(directory);
        assert_eq!(second.names[49], "Editor shortcuts");
        assert_eq!(second.names, second.saved_names);
        assert!(!second.state.loaded());
        let saved = second.saved_names.clone();
        second.names[0] = "a".repeat(257);
        second.save_labels();
        assert!(second.labels_error);
        assert_eq!(second.saved_names, saved);
        assert_eq!(second.names[0].len(), 257);
    }

    fn pending() -> MacroEditor {
        let mut editor = MacroEditor::new();
        editor.state.seed_verified(Macro {
            repeat_count: 1,
            events: Vec::new(),
        });
        editor.state.draft_mut().repeat_count = 2;
        editor.state.begin_apply().unwrap();
        editor
    }

    #[test]
    fn same_frame_close_and_apply_error_preserve_draft_and_error() {
        let mut editor = pending();
        let draft = editor.state.draft().clone();
        editor
            .tx
            .send(WorkerResult::Applied {
                slot: 0,
                result: Err("readback failed".into()),
            })
            .unwrap();
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let mut output = ctx.run_ui(input, |ui| {
            editor.handle_close(ui.ctx());
            editor.poll_worker();
        });
        output.textures_delta.clear();
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::CancelClose)
        );
        assert!(!editor.busy());
        assert!(!editor.state.trusted());
        assert!(editor.error && editor.status.contains("readback failed"));
        assert_eq!(editor.state.draft(), &draft);
        assert!(editor.dirty());
    }

    #[test]
    fn mismatching_success_cannot_mark_macro_saved() {
        let mut editor = pending();
        editor
            .tx
            .send(WorkerResult::Applied {
                slot: 0,
                result: Ok(vec![0; 256]),
            })
            .unwrap();
        editor.poll_worker();
        assert!(!editor.state.trusted());
        assert!(editor.error && editor.dirty());
        assert!(editor.state.begin_apply().is_err());
        let mut editor = pending();
        editor
            .tx
            .send(WorkerResult::Applied {
                slot: 0,
                result: macros::encode(editor.state.draft()),
            })
            .unwrap();
        editor.poll_worker();
        assert!(editor.state.trusted());
        assert!(!editor.error && !editor.dirty());
    }

    #[test]
    fn worker_panic_returns_an_unverified_completion() {
        let result: Result<(), String> = macro_worker(|| panic!("test panic"));
        assert!(result.unwrap_err().contains("restoration are unverified"));
    }

    #[test]
    fn invalidation_preserves_macro_draft_and_requires_reload() {
        let mut editor = MacroEditor::new();
        editor.state.seed_verified(editor.state.draft().clone());
        editor.state.draft_mut().repeat_count = 4;
        let draft = editor.state.draft().clone();
        editor.invalidate_device_read();
        assert_eq!(editor.state.draft(), &draft);
        assert!(editor.state.loaded() && editor.dirty() && !editor.busy());
        assert!(!editor.state.trusted());
        assert!(editor.state.begin_apply().is_err());
        assert!(editor.state.revert());
        assert!(!editor.state.trusted());
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EventKind {
    Key,
    Mouse,
    Move,
}

impl EventKind {
    fn of(event: &MacroEvent) -> Self {
        match event {
            MacroEvent::Key { .. } => Self::Key,
            MacroEvent::MouseButton { .. } => Self::Mouse,
            MacroEvent::Move { .. } => Self::Move,
        }
    }

    fn default_event(self) -> MacroEvent {
        match self {
            Self::Key => MacroEvent::Key {
                usage: 4,
                down: true,
                delay_ms: 50,
            },
            Self::Mouse => MacroEvent::MouseButton {
                button: 240,
                down: true,
                delay_ms: 50,
            },
            Self::Move => MacroEvent::Move {
                dx: 1,
                dy: 0,
                delay_ms: 10,
            },
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Key => "Key",
            Self::Mouse => "Mouse button",
            Self::Move => "Move",
        }
    }
}

enum WorkerResult {
    Loaded {
        slot: u8,
        result: Result<Vec<u8>, String>,
    },
    Applied {
        slot: u8,
        result: Result<Vec<u8>, String>,
    },
}

#[derive(Clone, Copy)]
enum RowAction {
    Up(usize),
    Down(usize),
    Remove(usize),
}

struct Recording {
    core: Recorder,
    modifiers: egui::Modifiers,
    skip_start_frame: bool,
}

pub struct MacroEditor {
    state: MacroState,
    names: Vec<String>,
    play_modes: Vec<u8>,
    io_path: String,
    backup_dir: PathBuf,
    labels_dir: Option<PathBuf>,
    saved_names: Vec<String>,
    labels_status: String,
    labels_error: bool,
    status: String,
    error: bool,
    recording: Option<Recording>,
    record_fixed_delay: bool,
    record_delay_ms: NonZeroU16,
    tx: Sender<WorkerResult>,
    rx: Receiver<WorkerResult>,
}

impl MacroEditor {
    pub fn new_with_backup_dir(backup_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            state: MacroState::new(),
            names: (0..50).map(|slot| format!("Macro {}", slot + 1)).collect(),
            play_modes: vec![0; 50],
            io_path: String::new(),
            backup_dir,
            labels_dir: None,
            saved_names: (0..50).map(|slot| format!("Macro {}", slot + 1)).collect(),
            labels_status: String::new(),
            labels_error: false,
            status: "Choose a slot, then load it from the keyboard.".into(),
            error: false,
            recording: None,
            record_fixed_delay: false,
            record_delay_ms: NonZeroU16::MIN,
            tx,
            rx,
        }
    }

    #[cfg(test)]
    pub fn new() -> Self {
        Self::new_with_backup_dir(std::env::temp_dir().join("byakko-test-backups"))
    }

    pub fn busy(&self) -> bool {
        self.state.busy() || self.recording.is_some()
    }

    fn playback_count_valid(&self) -> bool {
        match self.play_modes[self.state.slot() as usize] {
            0 => true,
            1 | 2 => self.state.draft().repeat_count == 1,
            _ => false,
        }
    }

    pub(crate) fn invalidate_device_read(&mut self) {
        self.state.mark_unverified();
        if self.state.loaded() {
            self.set_error("Device data may have changed. Draft retained; export it if needed, revert, then reload the slot before saving or binding.");
        }
    }

    /// Labels are local slot preferences, not data read from a keyboard.
    pub fn load_local_labels(&mut self, directory: PathBuf) {
        self.labels_dir = Some(directory.clone());
        match crate::macro_labels::load_latest(&directory) {
            Ok(Some(labels)) => {
                self.names = labels.names;
                self.saved_names = self.names.clone();
                self.labels_status = "Local labels loaded.".into();
                self.labels_error = false;
            }
            Ok(None) => {}
            Err(error) => {
                self.labels_status = format!("Local labels could not be loaded: {error}");
                self.labels_error = true;
            }
        }
    }

    fn save_labels(&mut self) {
        let Some(directory) = &self.labels_dir else {
            return;
        };
        let result = crate::macro_labels::Labels::new(self.names.clone())
            .and_then(|labels| crate::macro_labels::save_new(directory, &labels));
        match result {
            Ok(path) => {
                self.saved_names = self.names.clone();
                self.labels_status = format!("Local labels saved to {}", path.display());
                self.labels_error = false;
            }
            Err(error) => {
                self.labels_status = format!("Local label save failed: {error}");
                self.labels_error = true;
            }
        }
    }

    pub fn handle_close(&self, ctx: &egui::Context) {
        if self.state.busy() && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }

    fn dirty(&self) -> bool {
        self.state.dirty()
    }

    fn switch_slot(&mut self, next: u8) {
        match self.state.switch_slot(next) {
            Ok(true) => self.set_status(format!("Slot {next} selected. Load it before editing.")),
            Ok(false) => {}
            Err(error) => self.set_error(error),
        }
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.error = false;
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.error = true;
    }

    fn load(&mut self, ctx: &egui::Context) {
        if self.recording.is_some() {
            return;
        }
        let Ok(slot) = self.state.begin_read() else {
            return;
        };
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.set_status(format!("Reading macro slot {slot}…"));
        std::thread::spawn(move || {
            let result =
                macro_worker(|| device::read_macro(slot).map_err(|error| error.to_string()));
            let _ = tx.send(WorkerResult::Loaded { slot, result });
            ctx.request_repaint();
        });
    }

    fn apply(&mut self, ctx: &egui::Context) {
        if !self.playback_count_valid() {
            self.set_error("Toggle and hold modes require a stored repeat count of 1. Stage that count before saving.");
            return;
        }
        if self.busy() || !self.dirty() {
            return;
        }
        let request = match self.state.begin_apply() {
            Ok(request) => request,
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        let backup_dir = self.backup_dir.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.set_status(format!(
            "Backing up, writing, and verifying slot {}…",
            request.slot
        ));
        std::thread::spawn(move || {
            let result = macro_worker(|| {
                device::apply_macro(request.slot, &request.expected, &request.draft, &backup_dir)
                    .map_err(|error| error.to_string())
            });
            let _ = tx.send(WorkerResult::Applied {
                slot: request.slot,
                result,
            });
            ctx.request_repaint();
        });
    }

    fn poll_worker(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            match message {
                WorkerResult::Loaded { slot, result } => {
                    match self.state.complete_read(slot, result) {
                        Ok(()) => self.set_status(format!(
                            "Slot {slot} loaded. Edit events or bind it to the selected key."
                        )),
                        Err(error) => self.set_error(error),
                    }
                }
                WorkerResult::Applied { slot, result } => {
                    match self.state.complete_apply(slot, result) {
                        Ok(()) => self.set_status(format!(
                            "Slot {slot} saved and read back. Backup in {}",
                            self.backup_dir.display()
                        )),
                        Err(error) => self.set_error(error),
                    }
                }
            }
        }
    }

    fn revert(&mut self) {
        if self.state.revert() {
            self.set_status("Draft reverted to the last loaded or verified macro.");
        }
    }
    fn export_json(&mut self) {
        let path = self.io_path.trim();
        if path.is_empty() {
            self.set_error("Enter an export file path first.");
            return;
        }
        if let Err(error) = macros::encode(self.state.draft()) {
            self.set_error(error);
            return;
        }
        let file = MacroFile {
            format_version: 1,
            slot: self.state.slot(),
            name: self.names[self.state.slot() as usize].clone(),
            play_mode: self.play_modes[self.state.slot() as usize],
            macro_data: self.state.draft().clone(),
        };
        let result = macro_file::save_new(std::path::Path::new(path), &file);
        match result {
            Ok(()) => self.set_status(format!("Draft exported to {path}.")),
            Err(error) => self.set_error(format!("Export failed: {error}")),
        }
    }

    fn import_json(&mut self) {
        let path = self.io_path.trim();
        if path.is_empty() {
            self.set_error("Enter an import file path first.");
            return;
        }
        if let Err(error) = self.state.can_import() {
            self.set_error(error);
            return;
        }
        let result = macro_file::load(std::path::Path::new(path));
        match result {
            Ok(file) => {
                if let Err(error) = self.state.import_draft(file.macro_data) {
                    self.set_error(error);
                    return;
                }
                self.names[self.state.slot() as usize] = file.name;
                self.play_modes[self.state.slot() as usize] = file.play_mode;
                self.set_status(format!(
                    "Imported draft from {path} into slot {}. Review it before applying.",
                    self.state.slot()
                ));
            }
            Err(error) => self.set_error(format!("Import failed: {error}")),
        }
    }

    fn start_recording(&mut self, ctx: &egui::Context) -> bool {
        if self.busy() || !self.state.loaded() {
            self.set_error("Load a slot and finish other work before recording.");
            return false;
        }
        if let Err(error) = macros::encode(self.state.draft()) {
            self.set_error(format!("Cannot append recording to this draft: {error}"));
            return false;
        }
        let (now, modifiers) = ctx.input(|input| (input.time, input.modifiers));
        if modifiers.ctrl
            || modifiers.shift
            || modifiers.alt
            || modifiers.command
            || modifiers.mac_cmd
        {
            self.set_error("Release modifier keys before starting the recorder.");
            return false;
        }
        self.recording = Some(Recording {
            core: Recorder::new(
                now,
                if self.record_fixed_delay {
                    DelayPolicy::Fixed(self.record_delay_ms)
                } else {
                    DelayPolicy::Measured
                },
            ),
            modifiers: egui::Modifiers::NONE,
            skip_start_frame: true,
        });
        self.set_status("Recording focused keyboard and mouse events. Click STOP or leave the capture pad to finish.");
        true
    }

    fn stop_recording(&mut self, ctx: &egui::Context, reason: &str, is_error: bool) {
        let Some(recording) = self.recording.take() else {
            return;
        };
        let now = ctx.input(|input| input.time);
        let outcome = recording.core.stop(self.state.draft_mut(), now);
        let suffix = if outcome == crate::macro_recorder::StopOutcome::PauseTooLong {
            " A pause exceeded 65,535 ms; the final held interval was set to zero."
        } else {
            ""
        };
        if is_error || outcome == crate::macro_recorder::StopOutcome::PauseTooLong {
            self.set_error(format!("{reason}{suffix}"));
        } else {
            self.set_status(format!("{reason}{suffix}"));
        }
    }

    fn record_transition(
        &mut self,
        recording: &mut Recording,
        usage: u8,
        down: bool,
        now: f64,
    ) -> Result<(), String> {
        recording
            .core
            .transition(self.state.draft_mut(), usage, down, now)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn sync_modifiers(
        &mut self,
        recording: &mut Recording,
        modifiers: egui::Modifiers,
        now: f64,
    ) -> Result<(), String> {
        let before = recording.modifiers;
        let before_win = before.mac_cmd || (before.command && !before.ctrl);
        let after_win = modifiers.mac_cmd || (modifiers.command && !modifiers.ctrl);
        for (old, new, usage) in [
            (before.ctrl, modifiers.ctrl, 224),
            (before.shift, modifiers.shift, 225),
            (before.alt, modifiers.alt, 226),
            (before_win, after_win, 227),
        ] {
            if old != new {
                self.record_transition(recording, usage, new, now)?;
            }
        }
        recording.modifiers = modifiers;
        Ok(())
    }

    fn process_recording(
        &mut self,
        ctx: &egui::Context,
        capture_id: egui::Id,
        capture_rect: egui::Rect,
    ) {
        let Some(mut recording) = self.recording.take() else {
            return;
        };
        let (focused, now, events) =
            ctx.input(|input| (input.focused, input.time, input.events.clone()));
        if !focused {
            self.recording = Some(recording);
            self.stop_recording(
                ctx,
                "Recording stopped when the capture pad lost focus.",
                false,
            );
            return;
        }
        if !ctx.memory(|memory| memory.has_focus(capture_id)) {
            // egui uses Tab to move focus before this panel sees the frame.
            // Preserve that physical press, then release it as recording stops.
            if !recording.skip_start_frame
                && let Some(egui::Event::Key {
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                }) = events.iter().find(|event| {
                    matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Tab,
                            pressed: true,
                            repeat: false,
                            ..
                        }
                    )
                })
                && let Err(error) = self
                    .sync_modifiers(&mut recording, *modifiers, now)
                    .and_then(|()| self.record_transition(&mut recording, 43, true, now))
            {
                self.recording = Some(recording);
                self.stop_recording(ctx, &error, true);
                return;
            }
            self.recording = Some(recording);
            self.stop_recording(
                ctx,
                "Recording stopped when the capture pad lost focus.",
                false,
            );
            return;
        }
        if recording.skip_start_frame {
            recording.skip_start_frame = false;
            self.recording = Some(recording);
            return;
        }
        for event in events {
            let result = match event {
                egui::Event::ModifiersChanged(modifiers) => {
                    self.sync_modifiers(&mut recording, modifiers, now)
                }
                egui::Event::Key {
                    key,
                    physical_key,
                    pressed,
                    repeat,
                    modifiers,
                } => {
                    if let Err(error) = self.sync_modifiers(&mut recording, modifiers, now) {
                        Err(error)
                    } else if is_modifier_key(physical_key.unwrap_or(key)) || (pressed && repeat) {
                        Ok(())
                    } else if let Some(usage) = key_usage(physical_key.unwrap_or(key)) {
                        self.record_transition(&mut recording, usage, pressed, now)
                    } else {
                        Err(format!("Unsupported key {key:?}; recording stopped"))
                    }
                }
                egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) if !text.is_empty() => {
                    Err("IME composition cannot be recorded as physical key events".into())
                }
                egui::Event::Ime(egui::ImeEvent::Commit(_))
                | egui::Event::Ime(egui::ImeEvent::DeleteSurrounding { .. }) => {
                    Err("IME input cannot be recorded as physical key events".into())
                }
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                } => {
                    if !capture_rect.contains(pos) {
                        self.recording = Some(recording);
                        self.stop_recording(
                            ctx,
                            "Recording stopped outside the capture pad.",
                            false,
                        );
                        return;
                    }
                    mouse_usage(button).map_or(Ok(()), |usage| {
                        self.sync_modifiers(&mut recording, modifiers, now)
                            .and_then(|()| {
                                self.record_transition(&mut recording, usage, pressed, now)
                            })
                    })
                }
                _ => Ok(()),
            };
            if let Err(error) = result {
                self.recording = Some(recording);
                self.stop_recording(ctx, &error, true);
                return;
            }
        }
        self.recording = Some(recording);
    }

    fn event_grid(&mut self, ui: &mut egui::Ui) {
        let mut action = None;
        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                egui::Grid::new("macro_event_grid")
                    .striped(true)
                    .num_columns(8)
                    .spacing([10.0, 7.0])
                    .show(ui, |ui| {
                        for label in [
                            "#",
                            "EVENT",
                            "VALUE",
                            "STATE / Y",
                            "WAIT AFTER MS",
                            "",
                            "",
                            "",
                        ] {
                            ui.label(RichText::new(label).small().strong().color(MUTED));
                        }
                        ui.end_row();
                        let count = self.state.draft().events.len();
                        for index in 0..count {
                            ui.label(format!("{:02}", index + 1));
                            let event = &mut self.state.draft_mut().events[index];
                            let mut kind = EventKind::of(event);
                            egui::ComboBox::from_id_salt(("macro_kind", index))
                                .selected_text(kind.label())
                                .width(95.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut kind, EventKind::Key, "Key");
                                    ui.selectable_value(
                                        &mut kind,
                                        EventKind::Mouse,
                                        "Mouse button",
                                    );
                                    ui.selectable_value(&mut kind, EventKind::Move, "Move");
                                });
                            if kind != EventKind::of(event) {
                                *event = kind.default_event();
                            }
                            match event {
                                MacroEvent::Key {
                                    usage,
                                    down,
                                    delay_ms,
                                } => {
                                    ui.horizontal(|ui| {
                                        ui.add(egui::DragValue::new(usage).range(4..=239).speed(1));
                                        ui.label(layout::usage_label(*usage));
                                    });
                                    ui.checkbox(down, "Down");
                                    ui.add(
                                        egui::DragValue::new(delay_ms).range(0..=u16::MAX).speed(1),
                                    );
                                }
                                MacroEvent::MouseButton {
                                    button,
                                    down,
                                    delay_ms,
                                } => {
                                    egui::ComboBox::from_id_salt(("macro_mouse", index))
                                        .selected_text(mouse_name(*button))
                                        .width(95.0)
                                        .show_ui(ui, |ui| {
                                            for value in 240..=248 {
                                                ui.selectable_value(
                                                    button,
                                                    value,
                                                    mouse_name(value),
                                                );
                                            }
                                        });
                                    ui.checkbox(down, "Down");
                                    ui.add(
                                        egui::DragValue::new(delay_ms).range(0..=u16::MAX).speed(1),
                                    );
                                }
                                MacroEvent::Move { dx, dy, delay_ms } => {
                                    ui.horizontal(|ui| {
                                        ui.label("X");
                                        ui.add(egui::DragValue::new(dx).range(i8::MIN..=i8::MAX));
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Y");
                                        ui.add(egui::DragValue::new(dy).range(i8::MIN..=i8::MAX));
                                    });
                                    ui.add(
                                        egui::DragValue::new(delay_ms).range(0..=u16::MAX).speed(1),
                                    );
                                }
                            }
                            if ui.add_enabled(index > 0, egui::Button::new("↑")).clicked() {
                                action = Some(RowAction::Up(index));
                            }
                            if ui
                                .add_enabled(index + 1 < count, egui::Button::new("↓"))
                                .clicked()
                            {
                                action = Some(RowAction::Down(index));
                            }
                            if ui.button("×").clicked() {
                                action = Some(RowAction::Remove(index));
                            }
                            ui.end_row();
                        }
                    });
            });
        match action {
            Some(RowAction::Up(index)) => self.state.draft_mut().events.swap(index, index - 1),
            Some(RowAction::Down(index)) => self.state.draft_mut().events.swap(index, index + 1),
            Some(RowAction::Remove(index)) => {
                self.state.draft_mut().events.remove(index);
            }
            None => {}
        }
    }

    /// Returns a four-byte macro binding when the user chooses to bind the
    /// loaded, verified slot to the key selected in the surrounding workbench.
    pub fn ui(&mut self, ui: &mut egui::Ui, blocked: bool) -> Option<[u8; 4]> {
        self.handle_close(ui.ctx());
        self.poll_worker();
        let load_shortcut = ui.input(|input| {
            input.events.iter().any(|event| {
                matches!(event,
                    egui::Event::Key { key: egui::Key::L, pressed: true, repeat: false, modifiers, .. }
                    if modifiers.ctrl)
            })
        });
        if load_shortcut && !blocked && !self.busy() && !self.dirty() {
            self.load(ui.ctx());
        }
        let mut binding = None;
        let can_work = !blocked && !self.busy();
        egui::Frame::NONE.fill(PANEL).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
            ui.label(RichText::new("MACRO STUDIO").size(17.0).strong().color(INK));
            ui.label(RichText::new("Build an event stream, save it to a slot, then bind that slot to the selected key.").color(MUTED));
            ui.add_space(9.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("SLOT").small().strong().color(MUTED));
                let mut candidate = self.state.slot();
                let changed = ui.add_enabled(can_work, egui::DragValue::new(&mut candidate).range(0..=49).speed(1)).changed();
                if changed {
                    self.switch_slot(candidate);
                }
                let load_enabled = can_work && !self.dirty();
                if ui.add_enabled(load_enabled, egui::Button::new("LOAD SELECTED · Ctrl+L")).clicked() {
                    self.load(ui.ctx());
                }
                if self.state.busy() {
                    ui.spinner();
                }
                ui.label(if self.state.loaded() { "Loaded" } else { "Not loaded" });
            });
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.add_enabled(can_work, egui::TextEdit::singleline(&mut self.names[self.state.slot() as usize]).desired_width(220.0));
                ui.label(RichText::new("Local label").small().color(MUTED));
                if ui.add_enabled(can_work && self.labels_dir.is_some() && self.names != self.saved_names,
                    egui::Button::new("SAVE LABELS")).clicked() {
                    self.save_labels();
                }
            });
            ui.label(RichText::new("Labels belong to Nia87 slots on this computer and are shared across Nia87 keyboards.").small().color(MUTED));
            if self.names != self.saved_names {
                ui.label(RichText::new("Local label changes have not been saved.").small().color(ACCENT));
            }
            if !self.labels_status.is_empty() {
                ui.label(RichText::new(&self.labels_status).small().color(if self.labels_error { ACCENT } else { MUTED }));
            }
            ui.horizontal(|ui| {
                ui.label("Binding mode");
                ui.add_enabled_ui(can_work, |ui| {
                    egui::ComboBox::from_id_salt("macro_play_mode")
                        .selected_text(mode_name(self.play_modes[self.state.slot() as usize]))
                        .show_ui(ui, |ui| {
                            for mode in 0..=2 {
                                ui.selectable_value(&mut self.play_modes[self.state.slot() as usize], mode, mode_name(mode));
                            }
                        });
                });
                ui.label("Repeat count");
                let counted = self.play_modes[self.state.slot() as usize] == 0;
                ui.add_enabled(can_work && self.state.loaded() && counted, egui::DragValue::new(&mut self.state.draft_mut().repeat_count).range(0..=u16::MAX));
            });
            ui.label(RichText::new("Mode is applied with the key binding. Repeat count is shared by every key using this macro slot.").small().color(MUTED));
            if !self.playback_count_valid() {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Toggle and hold modes use a stored count of 1. Save this change before binding.");
                    if ui.add_enabled(can_work && self.state.loaded(), egui::Button::new("STAGE COUNT 1")).clicked() {
                        self.state.draft_mut().repeat_count = 1;
                    }
                });
            }
            ui.add_space(8.0);

            ui.label(RichText::new("EVENT STREAM").small().strong().color(MUTED));
            ui.add_enabled_ui(can_work && self.state.loaded(), |ui| {
                self.event_grid(ui);
                ui.horizontal(|ui| {
                    if ui.button("+ KEY PAIR").clicked() {
                        self.state.draft_mut().events.extend([
                            MacroEvent::Key { usage: 4, down: true, delay_ms: 50 },
                            MacroEvent::Key { usage: 4, down: false, delay_ms: 0 },
                        ]);
                    }
                    if ui.button("+ MOUSE PAIR").clicked() {
                        self.state.draft_mut().events.extend([
                            MacroEvent::MouseButton { button: 240, down: true, delay_ms: 50 },
                            MacroEvent::MouseButton { button: 240, down: false, delay_ms: 0 },
                        ]);
                    }
                    if ui.button("+ MOVE").clicked() {
                        self.state.draft_mut().events.push(EventKind::Move.default_event());
                    }
                    if ui.add_enabled(!self.state.draft().events.is_empty(), egui::Button::new("CLEAR DRAFT")).clicked() {
                        match self.state.clear_draft() {
                            Ok(()) => self.set_status("Draft events cleared. Revert to undo, or Save to keyboard to apply."),
                            Err(error) => self.set_error(error),
                        }
                    }
                });
            });
            let encoded = macros::encode(self.state.draft());
            let encoded_size = encoded_size(self.state.draft());
            match &encoded {
                Ok(_) => ui.label(RichText::new(format!("{} event(s) · {encoded_size}/248 encoded bytes", self.state.draft().events.len())).color(MUTED)),
                Err(error) => ui.label(RichText::new(format!("{encoded_size}/248 encoded bytes · Cannot save: {error}")).color(ACCENT)),
            };
            ui.add_space(8.0);
            ui.label(RichText::new("FOCUSED KEYBOARD / MOUSE RECORDER").small().strong().color(MUTED));
            ui.horizontal(|ui| {
                ui.add_enabled(can_work, egui::Checkbox::new(&mut self.record_fixed_delay, "Fixed delay"));
                let mut milliseconds = self.record_delay_ms.get();
                if ui.add_enabled(can_work && self.record_fixed_delay,
                    egui::DragValue::new(&mut milliseconds).range(1..=u16::MAX).suffix(" ms")).changed()
                    && let Some(delay) = NonZeroU16::new(milliseconds)
                {
                    self.record_delay_ms = delay;
                }
                let tail = if self.record_fixed_delay { self.record_delay_ms.get() } else { 50 };
                ui.label(RichText::new(format!("New recordings · final wait {tail} ms · existing events unchanged")).small().color(MUTED));
            });
            let mut just_started = false;
            ui.horizontal(|ui| {
                if self.recording.is_some() {
                    if ui.button("STOP RECORDING").clicked() {
                        self.stop_recording(ui.ctx(), "Recording stopped; held keys were released in the draft.", false);
                    }
                } else {
                    let label = if self.state.draft().events.is_empty() {
                        "START INTO EMPTY DRAFT"
                    } else {
                        "APPEND RECORDING"
                    };
                    if ui.add_enabled(can_work && self.state.loaded() && encoded.is_ok(), egui::Button::new(label)).clicked() {
                        just_started = self.start_recording(ui.ctx());
                    }
                }
                ui.label(RichText::new("Window-focused keys and mouse buttons only · no device write").small().color(MUTED));
            });
            if self.recording.is_some() {
                let capture_id = ui.make_persistent_id("macro_keyboard_capture");
                let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), 34.0));
                let response = ui.interact(rect, capture_id, egui::Sense::click());
                if just_started || response.clicked() {
                    response.request_focus();
                }
                ui.painter().rect_filled(rect, 3.0, Color32::from_rgb(236, 239, 230));
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER,
                    "RECORDING · keep this pad focused · click STOP to finish",
                    egui::FontId::proportional(12.0), INK);
                self.process_recording(ui.ctx(), capture_id, rect);
            }
            ui.label(RichText::new("Delays use egui frame time (millisecond rounding); events in one frame share a timestamp. Keypad and left/right modifier identity may be unavailable.").small().color(MUTED));
            ui.add_space(9.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(can_work && self.state.trusted() && self.dirty() && encoded.is_ok() && self.playback_count_valid(), egui::Button::new("SAVE TO KEYBOARD")).clicked() {
                    self.apply(ui.ctx());
                }
                if ui.add_enabled(can_work && self.dirty(), egui::Button::new("REVERT DRAFT")).clicked() {
                    self.revert();
                }
                let clean = self.state.trusted() && !self.dirty() && self.playback_count_valid();
                if ui.add_enabled(can_work && clean, egui::Button::new("BIND SELECTED KEY")).clicked() {
                    binding = Some([9, self.play_modes[self.state.slot() as usize], self.state.slot(), 0]);
                    self.set_status(format!("Macro slot {} selected for the current key. Apply the keymap to persist its binding.", self.state.slot()));
                }
            });
            ui.label(RichText::new(format!("Verified writes create a before-image in {}", self.backup_dir.display())).small().color(MUTED));
            ui.add_space(8.0);
            ui.separator();
            ui.label(RichText::new("JSON FILE").small().strong().color(MUTED));
            ui.horizontal(|ui| {
                ui.add_enabled(can_work, egui::TextEdit::singleline(&mut self.io_path).hint_text("Full .json path").desired_width(300.0));
                if ui.add_enabled(can_work && self.state.loaded() && !self.dirty(), egui::Button::new("IMPORT DRAFT")).clicked() {
                    self.import_json();
                }
                if ui.add_enabled(can_work && self.state.loaded() && encoded.is_ok(), egui::Button::new("EXPORT NEW FILE")).clicked() {
                    self.export_json();
                }
            });
            ui.label(RichText::new("Export creates a new file and never replaces an existing one. Import stages edits for review.").small().color(MUTED));
            ui.add_space(8.0);
            ui.label(RichText::new(&self.status).color(if self.error { ACCENT } else { INK }));
        });
        binding
    }
}

#[cfg(test)]
impl Default for MacroEditor {
    fn default() -> Self {
        Self::new()
    }
}

fn mode_name(mode: u8) -> &'static str {
    match mode {
        0 => "Repeat count",
        1 => "Toggle on/off",
        2 => "Hold to repeat",
        _ => "Unknown",
    }
}

fn mouse_name(button: u8) -> &'static str {
    match button {
        240 => "Left",
        241 => "Right",
        242 => "Middle",
        243 => "Back",
        244 => "Forward",
        245 => "Wheel left",
        246 => "Wheel right",
        247 => "Wheel forward",
        248 => "Wheel back",
        _ => "Unknown",
    }
}

fn encoded_size(macro_data: &Macro) -> usize {
    2 + macro_data
        .events
        .iter()
        .map(|event| match event {
            MacroEvent::Key { delay_ms, .. } | MacroEvent::MouseButton { delay_ms, .. } => {
                if (1..=127).contains(delay_ms) { 2 } else { 4 }
            }
            MacroEvent::Move { delay_ms, .. } => {
                if (1..=127).contains(delay_ms) {
                    4
                } else {
                    6
                }
            }
        })
        .sum::<usize>()
}

fn is_modifier_key(key: egui::Key) -> bool {
    matches!(
        key,
        egui::Key::ControlLeft
            | egui::Key::ControlRight
            | egui::Key::ShiftLeft
            | egui::Key::ShiftRight
            | egui::Key::AltLeft
            | egui::Key::AltRight
            | egui::Key::SuperLeft
            | egui::Key::SuperRight
    )
}

/// HID keyboard usages for egui's supported physical keys. The modifier
/// variants are handled separately from `egui::Modifiers` transitions.
fn key_usage(key: egui::Key) -> Option<u8> {
    use egui::Key::*;
    Some(match key {
        A => 4,
        B => 5,
        C => 6,
        D => 7,
        E => 8,
        F => 9,
        G => 10,
        H => 11,
        I => 12,
        J => 13,
        K => 14,
        L => 15,
        M => 16,
        N => 17,
        O => 18,
        P => 19,
        Q => 20,
        R => 21,
        S => 22,
        T => 23,
        U => 24,
        V => 25,
        W => 26,
        X => 27,
        Y => 28,
        Z => 29,
        Num1 | Exclamationmark => 30,
        Num2 => 31,
        Num3 => 32,
        Num4 => 33,
        Num5 => 34,
        Num6 => 35,
        Num7 => 36,
        Num8 => 37,
        Num9 => 38,
        Num0 => 39,
        Enter => 40,
        Escape => 41,
        Backspace => 42,
        Tab => 43,
        Space => 44,
        Minus => 45,
        Equals | Plus => 46,
        OpenBracket | OpenCurlyBracket => 47,
        CloseBracket | CloseCurlyBracket => 48,
        Backslash | Pipe => 49,
        Semicolon | Colon => 51,
        Quote => 52,
        Backtick => 53,
        Comma => 54,
        Period => 55,
        Slash | Questionmark => 56,
        IntlBackslash => 100,
        F1 => 58,
        F2 => 59,
        F3 => 60,
        F4 => 61,
        F5 => 62,
        F6 => 63,
        F7 => 64,
        F8 => 65,
        F9 => 66,
        F10 => 67,
        F11 => 68,
        F12 => 69,
        F13 => 104,
        F14 => 105,
        F15 => 106,
        F16 => 107,
        F17 => 108,
        F18 => 109,
        F19 => 110,
        F20 => 111,
        F21 => 112,
        F22 => 113,
        F23 => 114,
        F24 => 115,
        Insert => 73,
        Home => 74,
        PageUp => 75,
        Delete => 76,
        End => 77,
        PageDown => 78,
        ArrowRight => 79,
        ArrowLeft => 80,
        ArrowDown => 81,
        ArrowUp => 82,
        _ => return None,
    })
}

fn mouse_usage(button: egui::PointerButton) -> Option<u8> {
    Some(match button {
        egui::PointerButton::Primary => 240,
        egui::PointerButton::Secondary => 241,
        egui::PointerButton::Middle => 242,
        egui::PointerButton::Extra1 => 243,
        egui::PointerButton::Extra2 => 244,
    })
}

#[cfg(test)]
mod recording_tests {
    use super::{MacroEditor, Recording, key_usage};
    use crate::macro_recorder::{DelayPolicy, Recorder};
    use crate::macros::{self, MacroEvent};
    use eframe::egui::Key;

    #[test]
    fn editor_passes_fixed_delay_policy_to_the_recording_session() {
        let ctx = eframe::egui::Context::default();
        let mut editor = MacroEditor::new();
        editor.state.seed_verified(editor.state.draft().clone());
        editor.record_fixed_delay = true;
        editor.record_delay_ms = std::num::NonZeroU16::new(10).unwrap();
        assert!(editor.start_recording(&ctx));
        let recording = editor.recording.as_mut().unwrap();
        recording
            .core
            .transition(editor.state.draft_mut(), 4, true, 1.0)
            .unwrap();
        recording
            .core
            .transition(editor.state.draft_mut(), 4, false, 1.15)
            .unwrap();
        editor.stop_recording(&ctx, "Stopped", false);
        assert_eq!(
            editor.state.draft().events,
            vec![
                MacroEvent::Key {
                    usage: 4,
                    down: true,
                    delay_ms: 10
                },
                MacroEvent::Key {
                    usage: 4,
                    down: false,
                    delay_ms: 10
                },
            ]
        );
    }

    #[test]
    fn egui_recording_tracks_physical_keys_and_releases_on_focus_loss() {
        use eframe::egui::{self, Event, Modifiers};
        let ctx = egui::Context::default();
        let mut editor = MacroEditor::new();
        editor.state.seed_verified(editor.state.draft().clone());
        let id = egui::Id::new("test capture pad");
        let mut frame = |time, focused, events, start| {
            let input = egui::RawInput {
                time: Some(time),
                focused,
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let response = ui.interact(ui.max_rect(), id, egui::Sense::click());
                if start {
                    response.request_focus();
                    assert!(editor.start_recording(ui.ctx()));
                }
                editor.process_recording(ui.ctx(), id, ui.max_rect());
            });
            output.textures_delta.clear();
        };
        frame(1.0, true, vec![], true);
        let ctrl = Modifiers {
            ctrl: true,
            command: true,
            ..Modifiers::NONE
        };
        let key = |repeat| Event::Key {
            key: Key::Z,
            physical_key: Some(Key::A),
            pressed: true,
            repeat,
            modifiers: ctrl,
        };
        frame(
            1.1,
            true,
            vec![Event::ModifiersChanged(ctrl), key(false)],
            false,
        );
        frame(1.15, true, vec![key(true)], false);
        frame(1.2, false, vec![], false);
        assert!(editor.recording.is_none());
        assert!(!editor.busy());
        let keys: Vec<_> = editor
            .state
            .draft()
            .events
            .iter()
            .map(|event| match event {
                MacroEvent::Key { usage, down, .. } => (*usage, *down),
                _ => panic!("Unexpected non-key event"),
            })
            .collect();
        assert_eq!(keys, [(224, true), (4, true), (4, false), (224, false)]);
        assert!(macros::encode(editor.state.draft()).is_ok());
    }

    #[test]
    fn maps_physical_keys_and_rejects_unavailable_usages() {
        assert_eq!(key_usage(Key::A), Some(4));
        assert_eq!(key_usage(Key::F24), Some(115));
        assert_eq!(key_usage(Key::ArrowLeft), Some(80));
        assert_eq!(key_usage(Key::F25), None);
        assert_eq!(key_usage(Key::BrowserBack), None);
    }

    #[test]
    fn records_mouse_press_and_release_inside_capture_pad() {
        use eframe::egui::{self, Event, PointerButton, Pos2};
        let ctx = egui::Context::default();
        let mut editor = MacroEditor::new();
        editor.state.seed_verified(editor.state.draft().clone());
        let id = egui::Id::new("mouse capture pad");
        let mut frame = |time, events| {
            let input = egui::RawInput {
                time: Some(time),
                focused: true,
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let rect = ui.max_rect();
                let response = ui.interact(rect, id, egui::Sense::click());
                if time == 1.0 {
                    response.request_focus();
                    editor.start_recording(ui.ctx());
                }
                editor.process_recording(ui.ctx(), id, rect);
            });
            output.textures_delta.clear();
        };
        frame(1.0, vec![]);
        frame(
            1.1,
            vec![Event::PointerButton {
                pos: Pos2::new(1.0, 1.0),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..egui::Modifiers::NONE
                },
            }],
        );
        frame(
            1.2,
            vec![Event::PointerButton {
                pos: Pos2::new(1.0, 1.0),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..egui::Modifiers::NONE
                },
            }],
        );
        let events: Vec<_> = editor
            .state
            .draft()
            .events
            .iter()
            .filter_map(|event| match event {
                MacroEvent::MouseButton { button, down, .. } => Some((*button, *down)),
                _ => None,
            })
            .collect();
        assert_eq!(events, [(240, true), (240, false)]);
        assert!(matches!(
            editor.state.draft().events.first(),
            Some(MacroEvent::Key {
                usage: 224,
                down: true,
                ..
            })
        ));
        assert!(macros::encode(editor.state.draft()).is_ok());
    }

    #[test]
    fn outside_mouse_press_stops_and_releases_held_mouse_button() {
        use eframe::egui::{self, Event, PointerButton, Pos2};
        let ctx = egui::Context::default();
        let mut editor = MacroEditor::new();
        editor.state.seed_verified(editor.state.draft().clone());
        let id = egui::Id::new("outside mouse pad");
        let mut frame = |time, events| {
            let input = egui::RawInput {
                time: Some(time),
                focused: true,
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let rect = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 40.0));
                let response = ui.interact(rect, id, egui::Sense::click());
                if time == 1.0 {
                    response.request_focus();
                    editor.start_recording(ui.ctx());
                }
                editor.process_recording(ui.ctx(), id, rect);
            });
            output.textures_delta.clear();
        };
        frame(1.0, vec![]);
        frame(
            1.1,
            vec![Event::PointerButton {
                pos: Pos2::new(1.0, 1.0),
                button: PointerButton::Secondary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        frame(
            1.2,
            vec![Event::PointerButton {
                pos: Pos2::new(150.0, 50.0),
                button: PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(editor.recording.is_none());
        assert!(matches!(
            editor.state.draft().events.last(),
            Some(MacroEvent::MouseButton {
                button: 241,
                down: false,
                ..
            })
        ));
    }

    #[test]
    fn focus_loss_releases_held_mouse_button() {
        use eframe::egui::{self, Pos2};
        let ctx = egui::Context::default();
        let mut editor = MacroEditor::new();
        editor.state.seed_verified(editor.state.draft().clone());
        let mut core = Recorder::new(1.0, DelayPolicy::Measured);
        core.transition(editor.state.draft_mut(), 242, true, 1.0)
            .unwrap();
        editor.recording = Some(Recording {
            core,
            modifiers: egui::Modifiers::NONE,
            skip_start_frame: false,
        });
        let id = egui::Id::new("focus loss mouse pad");
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(1.1),
                focused: false,
                ..Default::default()
            },
            |ui| {
                editor.process_recording(
                    ui.ctx(),
                    id,
                    egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 40.0)),
                );
            },
        );
        output.textures_delta.clear();
        assert!(editor.recording.is_none());
        assert!(matches!(
            editor.state.draft().events.last(),
            Some(MacroEvent::MouseButton {
                button: 242,
                down: false,
                ..
            })
        ));
    }
}
