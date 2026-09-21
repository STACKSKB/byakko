//! Native macro editor. Device transactions run on worker threads.

use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
};

use eframe::egui::{self, Color32, RichText};
use serde::{Deserialize, Serialize};

use crate::{
    device, layout,
    macros::{self, Macro, MacroEvent},
};

const INK: Color32 = Color32::from_rgb(33, 42, 46);
const MUTED: Color32 = Color32::from_rgb(96, 107, 109);
const PANEL: Color32 = Color32::from_rgb(252, 251, 246);
const ACCENT: Color32 = Color32::from_rgb(199, 91, 45);

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

#[derive(Serialize, Deserialize)]
struct MacroFile {
    format_version: u32,
    slot: u8,
    name: String,
    play_mode: u8,
    macro_data: Macro,
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

pub struct MacroEditor {
    slot: u8,
    draft: Macro,
    loaded: Option<Macro>,
    observed: Option<Vec<u8>>,
    names: Vec<String>,
    play_modes: Vec<u8>,
    io_path: String,
    backup_dir: PathBuf,
    status: String,
    error: bool,
    busy: bool,
    trusted: bool,
    tx: Sender<WorkerResult>,
    rx: Receiver<WorkerResult>,
}

impl MacroEditor {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            slot: 0,
            draft: Macro {
                repeat_count: 1,
                events: Vec::new(),
            },
            loaded: None,
            observed: None,
            names: (0..50).map(|slot| format!("Macro {}", slot + 1)).collect(),
            play_modes: vec![0; 50],
            io_path: String::new(),
            backup_dir: std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("backups"),
            status: "Choose a slot, then load it from the keyboard.".into(),
            error: false,
            busy: false,
            trusted: false,
            tx,
            rx,
        }
    }

    pub fn busy(&self) -> bool {
        self.busy
    }

    fn dirty(&self) -> bool {
        self.loaded
            .as_ref()
            .is_some_and(|loaded| *loaded != self.draft)
    }

    fn switch_slot(&mut self, next: u8) {
        if next == self.slot {
            return;
        }
        if self.dirty() {
            self.set_error("Save or revert this draft before changing slots.");
            return;
        }
        self.slot = next;
        self.loaded = None;
        self.observed = None;
        self.trusted = false;
        self.draft = Macro {
            repeat_count: 1,
            events: Vec::new(),
        };
        self.set_status(format!("Slot {next} selected. Load it before editing."));
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
        if self.busy || self.dirty() {
            return;
        }
        let slot = self.slot;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.busy = true;
        self.set_status(format!("Reading macro slot {slot}…"));
        std::thread::spawn(move || {
            let result = device::read_macro(slot).map_err(|error| error.to_string());
            let _ = tx.send(WorkerResult::Loaded { slot, result });
            ctx.request_repaint();
        });
    }

    fn apply(&mut self, ctx: &egui::Context) {
        if self.busy || !self.dirty() {
            return;
        }
        let Some(expected) = self.observed.clone() else {
            self.set_error("Load this macro slot before applying changes.");
            return;
        };
        if let Err(error) = macros::encode(&self.draft) {
            self.set_error(error);
            return;
        }
        let slot = self.slot;
        let draft = self.draft.clone();
        let backup_dir = self.backup_dir.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.busy = true;
        self.set_status(format!("Backing up, writing, and verifying slot {slot}…"));
        std::thread::spawn(move || {
            let result = device::apply_macro(slot, &expected, &draft, &backup_dir)
                .map_err(|error| error.to_string());
            let _ = tx.send(WorkerResult::Applied { slot, result });
            ctx.request_repaint();
        });
    }

    fn poll_worker(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            self.busy = false;
            match message {
                WorkerResult::Loaded { slot, result } if slot == self.slot => {
                    match result {
                        Ok(bytes) => match macros::decode(&bytes) {
                            Ok(value) => {
                                self.loaded = Some(value.clone());
                                self.draft = value;
                                self.observed = Some(bytes);
                                self.trusted = true;
                                self.set_status(format!("Slot {slot} loaded. Edit events or bind it to the selected key."));
                            }
                            Err(error) => {
                                self.trusted = false;
                                self.set_error(format!("Slot {slot} could not be decoded: {error}"))
                            }
                        },
                        Err(error) => {
                            self.trusted = false;
                            self.set_error(format!("Slot {slot} read failed: {error}"));
                        }
                    }
                }
                WorkerResult::Applied { slot, result } if slot == self.slot => match result {
                    Ok(bytes) => {
                        self.observed = Some(bytes);
                        self.loaded = Some(self.draft.clone());
                        self.trusted = true;
                        self.set_status(format!(
                            "Slot {slot} saved and read back. Backup in {}",
                            self.backup_dir.display()
                        ));
                    }
                    Err(error) => {
                        self.trusted = false;
                        self.set_error(format!("Slot {slot} apply failed: {error}"));
                    }
                },
                _ => {
                    self.trusted = false;
                    self.set_error("A macro operation returned for another slot; draft preserved.")
                }
            }
        }
    }

    fn revert(&mut self) {
        if let Some(value) = self.loaded.clone() {
            self.draft = value;
            self.set_status("Draft reverted to the last loaded or verified macro.");
        }
    }

    fn export_json(&mut self) {
        let path = self.io_path.trim();
        if path.is_empty() {
            self.set_error("Enter an export file path first.");
            return;
        }
        if let Err(error) = macros::encode(&self.draft) {
            self.set_error(error);
            return;
        }
        let file = MacroFile {
            format_version: 1,
            slot: self.slot,
            name: self.names[self.slot as usize].clone(),
            play_mode: self.play_modes[self.slot as usize],
            macro_data: self.draft.clone(),
        };
        let result = (|| -> Result<(), String> {
            let mut handle = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|error| error.to_string())?;
            serde_json::to_writer_pretty(&mut handle, &file).map_err(|error| error.to_string())?;
            handle.write_all(b"\n").map_err(|error| error.to_string())?;
            handle.sync_all().map_err(|error| error.to_string())?;
            Ok(())
        })();
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
        if self.loaded.is_none() {
            self.set_error(
                "Load this slot before importing, so its current bytes are backed up before apply.",
            );
            return;
        }
        if self.dirty() {
            self.set_error("Save or revert the current draft before importing another file.");
            return;
        }
        let result = (|| -> Result<MacroFile, String> {
            let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
            let file: MacroFile = serde_json::from_str(&text).map_err(|error| error.to_string())?;
            if file.format_version != 1 || file.play_mode > 2 || file.slot > 49 {
                return Err("Unsupported macro JSON version, slot, or play mode".into());
            }
            macros::encode(&file.macro_data)?;
            Ok(file)
        })();
        match result {
            Ok(file) => {
                self.draft = file.macro_data;
                self.names[self.slot as usize] = file.name;
                self.play_modes[self.slot as usize] = file.play_mode;
                self.set_status(format!(
                    "Imported draft from {path} into slot {}. Review it before applying.",
                    self.slot
                ));
            }
            Err(error) => self.set_error(format!("Import failed: {error}")),
        }
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
                        for label in ["#", "EVENT", "VALUE", "STATE / Y", "DELAY MS", "", "", ""] {
                            ui.label(RichText::new(label).small().strong().color(MUTED));
                        }
                        ui.end_row();
                        let count = self.draft.events.len();
                        for index in 0..count {
                            ui.label(format!("{:02}", index + 1));
                            let event = &mut self.draft.events[index];
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
            Some(RowAction::Up(index)) => self.draft.events.swap(index, index - 1),
            Some(RowAction::Down(index)) => self.draft.events.swap(index, index + 1),
            Some(RowAction::Remove(index)) => {
                self.draft.events.remove(index);
            }
            None => {}
        }
    }

    /// Returns a four-byte macro binding when the user chooses to bind the
    /// loaded, verified slot to the key selected in the surrounding workbench.
    pub fn ui(&mut self, ui: &mut egui::Ui, blocked: bool) -> Option<[u8; 4]> {
        self.poll_worker();
        let mut binding = None;
        let can_work = !blocked && !self.busy;
        egui::Frame::NONE.fill(PANEL).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
            ui.label(RichText::new("MACRO STUDIO").size(17.0).strong().color(INK));
            ui.label(RichText::new("Build an event stream, save it to a slot, then bind that slot to the selected key.").color(MUTED));
            ui.add_space(9.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("SLOT").small().strong().color(MUTED));
                let mut candidate = self.slot;
                let changed = ui.add_enabled(can_work, egui::DragValue::new(&mut candidate).range(0..=49).speed(1)).changed();
                if changed {
                    self.switch_slot(candidate);
                }
                let load_enabled = can_work && !self.dirty();
                if ui.add_enabled(load_enabled, egui::Button::new("LOAD SELECTED")).clicked() {
                    self.load(ui.ctx());
                }
                if self.busy {
                    ui.spinner();
                }
                ui.label(if self.loaded.is_some() { "Loaded" } else { "Not loaded" });
            });
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.add_enabled(can_work, egui::TextEdit::singleline(&mut self.names[self.slot as usize]).desired_width(220.0));
                ui.label(RichText::new("Local label").small().color(MUTED));
            });
            ui.horizontal(|ui| {
                ui.label("Play mode");
                ui.add_enabled_ui(can_work, |ui| {
                    egui::ComboBox::from_id_salt("macro_play_mode")
                        .selected_text(mode_name(self.play_modes[self.slot as usize]))
                        .show_ui(ui, |ui| {
                            for mode in 0..=2 {
                                ui.selectable_value(&mut self.play_modes[self.slot as usize], mode, mode_name(mode));
                            }
                        });
                });
                ui.label("Repeat count");
                ui.add_enabled(can_work && self.loaded.is_some(), egui::DragValue::new(&mut self.draft.repeat_count).range(0..=u16::MAX));
            });
            ui.add_space(8.0);

            ui.label(RichText::new("EVENT STREAM").small().strong().color(MUTED));
            ui.add_enabled_ui(can_work && self.loaded.is_some(), |ui| {
                self.event_grid(ui);
                ui.horizontal(|ui| {
                    if ui.button("+ KEY PAIR").clicked() {
                        self.draft.events.extend([
                            MacroEvent::Key { usage: 4, down: true, delay_ms: 50 },
                            MacroEvent::Key { usage: 4, down: false, delay_ms: 0 },
                        ]);
                    }
                    if ui.button("+ MOUSE PAIR").clicked() {
                        self.draft.events.extend([
                            MacroEvent::MouseButton { button: 240, down: true, delay_ms: 50 },
                            MacroEvent::MouseButton { button: 240, down: false, delay_ms: 0 },
                        ]);
                    }
                    if ui.button("+ MOVE").clicked() {
                        self.draft.events.push(EventKind::Move.default_event());
                    }
                });
            });
            let encoded = macros::encode(&self.draft);
            let encoded_size = encoded_size(&self.draft);
            match &encoded {
                Ok(_) => ui.label(RichText::new(format!("{} event(s) · {encoded_size}/248 encoded bytes", self.draft.events.len())).color(MUTED)),
                Err(error) => ui.label(RichText::new(format!("{encoded_size}/248 encoded bytes · Cannot save: {error}")).color(ACCENT)),
            };
            ui.add_space(9.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(can_work && self.dirty() && encoded.is_ok(), egui::Button::new("SAVE TO KEYBOARD")).clicked() {
                    self.apply(ui.ctx());
                }
                if ui.add_enabled(can_work && self.dirty(), egui::Button::new("REVERT DRAFT")).clicked() {
                    self.revert();
                }
                let clean = self.trusted && self.observed.is_some() && self.loaded.is_some() && !self.dirty();
                if ui.add_enabled(can_work && clean, egui::Button::new("BIND SELECTED KEY")).clicked() {
                    binding = Some([9, self.play_modes[self.slot as usize], self.slot, 0]);
                    self.set_status(format!("Macro slot {} selected for the current key. Apply the keymap to persist its binding.", self.slot));
                }
            });
            ui.label(RichText::new(format!("Verified writes create a before-image in {}", self.backup_dir.display())).small().color(MUTED));
            ui.add_space(8.0);
            ui.separator();
            ui.label(RichText::new("JSON FILE").small().strong().color(MUTED));
            ui.horizontal(|ui| {
                ui.add_enabled(can_work, egui::TextEdit::singleline(&mut self.io_path).hint_text("Full .json path").desired_width(300.0));
                if ui.add_enabled(can_work && self.loaded.is_some() && !self.dirty(), egui::Button::new("IMPORT DRAFT")).clicked() {
                    self.import_json();
                }
                if ui.add_enabled(can_work && self.loaded.is_some() && encoded.is_ok(), egui::Button::new("EXPORT NEW FILE")).clicked() {
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
