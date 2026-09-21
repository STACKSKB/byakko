//! Native keymap workbench. Device I/O is confined to short-lived worker threads.

use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::{
    actions, board,
    device::{self, Snapshot},
    layout::{self, FN_PLACEHOLDER_USAGE, PhysicalKey},
};

const INK: Color32 = Color32::from_rgb(33, 42, 46);
const MUTED: Color32 = Color32::from_rgb(96, 107, 109);
const PAPER: Color32 = Color32::from_rgb(244, 243, 237);
const PANEL: Color32 = Color32::from_rgb(252, 251, 246);
const RULE: Color32 = Color32::from_rgb(200, 205, 200);
const ACCENT: Color32 = Color32::from_rgb(199, 91, 45);
const SELECTED: Color32 = Color32::from_rgb(250, 225, 192);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Base,
    Function,
}

impl Layer {
    fn name(self) -> &'static str {
        match self {
            Self::Base => "BASE",
            Self::Function => "FN",
        }
    }
}

enum WorkerResult {
    Read(Result<Snapshot, String>),
    Applied(Result<Snapshot, String>),
}

struct Workbench {
    keys: Vec<PhysicalKey>,
    observed: Option<Snapshot>,
    base: Vec<[u8; 4]>,
    function: Vec<[u8; 4]>,
    selected: Option<u8>,
    layer: Layer,
    search: String,
    modifiers: [bool; 4],
    raw_editor: String,
    test_input: String,
    status: String,
    error: bool,
    busy: bool,
    tx: Sender<WorkerResult>,
    rx: Receiver<WorkerResult>,
    backup_dir: PathBuf,
}

impl Workbench {
    fn new(ctx: &egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            keys: layout::nia87_keys(),
            observed: None,
            base: Vec::new(),
            function: Vec::new(),
            selected: None,
            layer: Layer::Base,
            search: String::new(),
            modifiers: [false; 4],
            raw_editor: String::new(),
            test_input: String::new(),
            status: "Reading connected keyboard…".into(),
            error: false,
            busy: false,
            tx,
            rx,
            backup_dir: std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("backups"),
        };
        app.start_read(ctx);
        app
    }

    fn start_read(&mut self, ctx: &egui::Context) {
        if self.busy || self.dirty_count() > 0 {
            return;
        }
        self.busy = true;
        self.error = false;
        self.status = "Reading base and Fn keymaps…".into();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = device::snapshot().map_err(|error| error.to_string());
            let _ = tx.send(WorkerResult::Read(result));
            ctx.request_repaint();
        });
    }

    fn start_apply(&mut self, ctx: &egui::Context) {
        let Some(expected) = self.observed.clone() else {
            return;
        };
        if self.busy || self.dirty_count() == 0 {
            return;
        }
        let base = self.base.clone();
        let function = self.function.clone();
        let backup_dir = self.backup_dir.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.busy = true;
        self.error = false;
        self.status = "Comparing device state, backing up, and applying staged changes…".into();
        std::thread::spawn(move || {
            let result = device::apply_keymaps(&expected, &base, &function, &backup_dir)
                .map_err(|error| error.to_string());
            let _ = tx.send(WorkerResult::Applied(result));
            ctx.request_repaint();
        });
    }

    fn poll_worker(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            self.busy = false;
            match message {
                WorkerResult::Read(Ok(snapshot)) => {
                    self.load(snapshot);
                    self.status =
                        "Device read complete. Select a key to inspect its binding.".into();
                    self.error = false;
                }
                WorkerResult::Applied(Ok(snapshot)) => {
                    self.load(snapshot);
                    self.status = format!(
                        "Changes applied and verified. Backup directory: {}",
                        self.backup_dir.display()
                    );
                    self.error = false;
                }
                WorkerResult::Read(Err(error)) | WorkerResult::Applied(Err(error)) => {
                    self.status = error;
                    self.error = true;
                }
            }
        }
    }

    fn load(&mut self, snapshot: Snapshot) {
        self.base = snapshot.base.clone();
        self.function = snapshot.function.clone();
        self.observed = Some(snapshot);
        self.sync_editor();
    }

    fn draft(&self, layer: Layer) -> &[[u8; 4]] {
        match layer {
            Layer::Base => &self.base,
            Layer::Function => &self.function,
        }
    }

    fn draft_mut(&mut self, layer: Layer) -> &mut Vec<[u8; 4]> {
        match layer {
            Layer::Base => &mut self.base,
            Layer::Function => &mut self.function,
        }
    }

    fn slot(&self, usage: u8) -> Option<usize> {
        if usage == FN_PLACEHOLDER_USAGE {
            return None;
        }
        let slot = board::slot_for_usage(usage)?;
        (slot < self.base.len() && slot < self.function.len()).then_some(slot)
    }

    fn binding(&self, layer: Layer, usage: u8) -> Option<[u8; 4]> {
        self.slot(usage).map(|slot| self.draft(layer)[slot])
    }

    fn current_label(&self, layer: Layer, usage: u8) -> String {
        match self.binding(layer, usage) {
            Some([0, 0, 0, 0]) => "Disabled".into(),
            Some([0, 0, key_usage, 0])
                if key_usage != 0
                    && key_usage != FN_PLACEHOLDER_USAGE
                    && !layout::usage_label(key_usage).starts_with("0x") =>
            {
                layout::usage_label(key_usage)
            }
            Some([0, modifier @ 224..=227, key_usage, 0]) => {
                format!(
                    "{}+{}",
                    modifier_label(modifier),
                    layout::usage_label(key_usage)
                )
            }
            Some([0, first @ 224..=227, second @ 224..=227, key_usage]) if key_usage != 0 => {
                format!(
                    "{}+{}+{}",
                    modifier_label(first),
                    modifier_label(second),
                    layout::usage_label(key_usage)
                )
            }
            Some(bytes) => actions::presets()
                .into_iter()
                .find(|preset| preset.bytes == bytes)
                .map(|preset| preset.label.to_owned())
                .unwrap_or_else(|| format!("Raw {}", format_bytes(bytes))),
            None => "Unmapped".into(),
        }
    }

    fn dirty_count(&self) -> usize {
        let Some(observed) = &self.observed else {
            return 0;
        };
        self.base
            .iter()
            .zip(&observed.base)
            .filter(|(a, b)| a != b)
            .count()
            + self
                .function
                .iter()
                .zip(&observed.function)
                .filter(|(a, b)| a != b)
                .count()
    }

    fn changed(&self, layer: Layer, usage: u8) -> bool {
        let (Some(observed), Some(slot)) = (&self.observed, self.slot(usage)) else {
            return false;
        };
        let before = match layer {
            Layer::Base => &observed.base,
            Layer::Function => &observed.function,
        };
        before
            .get(slot)
            .is_some_and(|value| *value != self.draft(layer)[slot])
    }

    fn sync_editor(&mut self) {
        let binding = self
            .selected
            .and_then(|usage| self.binding(self.layer, usage));
        self.raw_editor = binding.map(format_bytes).unwrap_or_default();
        self.modifiers = [false; 4];
        if let Some([0, first @ 224..=227, second, last]) = binding {
            self.modifiers[(first - 224) as usize] = true;
            if (224..=227).contains(&second) && last != 0 {
                self.modifiers[(second - 224) as usize] = true;
            }
        }
    }

    fn select(&mut self, usage: Option<u8>) {
        self.selected = usage;
        self.search.clear();
        self.sync_editor();
    }

    fn set_binding(&mut self, bytes: [u8; 4]) {
        let Some(usage) = self.selected else {
            return;
        };
        let Some(slot) = self.slot(usage) else {
            return;
        };
        if self.busy {
            return;
        }
        self.draft_mut(self.layer)[slot] = bytes;
        self.sync_editor();
        self.error = false;
        self.status = "Binding staged. Review the change, then apply it explicitly.".into();
    }

    fn stage_key(&mut self, usage: u8) {
        let selected: Vec<_> = self
            .modifiers
            .iter()
            .enumerate()
            .filter_map(|(index, on)| on.then_some(224 + index as u8))
            .collect();
        let bytes = match selected.as_slice() {
            [] => Some(actions::key_binding(usage)),
            [first] => actions::combo_binding(*first, usage).ok(),
            [first, second] => actions::two_modifier_binding(*first, *second, usage).ok(),
            _ => None,
        };
        if let Some(bytes) = bytes {
            self.set_binding(bytes);
        }
    }

    fn revert(&mut self) {
        if self.busy {
            return;
        }
        if let Some(snapshot) = &self.observed {
            self.base.clone_from(&snapshot.base);
            self.function.clone_from(&snapshot.function);
            self.sync_editor();
            self.status = "Staged changes reverted to the last device read.".into();
            self.error = false;
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("BYAKKO").size(24.0).strong().color(INK));
            ui.separator();
            ui.label(
                egui::RichText::new("NIA87 / KEYMAP WORKBENCH")
                    .size(13.0)
                    .strong()
                    .color(MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_read = !self.busy && self.dirty_count() == 0;
                if ui
                    .add_enabled(can_read, egui::Button::new("RECONNECT / READ"))
                    .clicked()
                {
                    self.start_read(ui.ctx());
                }
                if let Some(snapshot) = &self.observed {
                    ui.label(format!(
                        "PROFILE {}  ·  FW {:04X}",
                        snapshot.profile, snapshot.firmware
                    ));
                }
            });
        });
        ui.add_space(5.0);
        ui.separator();
    }

    fn layer_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("LAYER").small().strong().color(MUTED));
            for layer in [Layer::Base, Layer::Function] {
                if ui
                    .selectable_label(self.layer == layer, layer.name())
                    .clicked()
                {
                    self.layer = layer;
                    self.sync_editor();
                }
            }
            ui.separator();
            ui.label(
                egui::RichText::new(format!("{} staged slot(s)", self.dirty_count())).color(
                    if self.dirty_count() > 0 {
                        ACCENT
                    } else {
                        MUTED
                    },
                ),
            );
            if self.busy {
                ui.spinner();
            }
        });
    }

    fn keyboard(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("PHYSICAL LAYOUT")
                .small()
                .strong()
                .color(MUTED),
        );
        let unit = (ui.available_width() / 18.5).clamp(27.0, 50.0);
        let gap = 2.0;
        let height = 6.5 * unit;
        let (canvas, _) = ui.allocate_exact_size(Vec2::new(18.5 * unit, height), Sense::hover());
        let painter = ui.painter();
        let mut clicked = None;
        for key in &self.keys {
            let rect = Rect::from_min_size(
                Pos2::new(
                    canvas.min.x + key.x * unit + gap,
                    canvas.min.y + key.y * unit + gap,
                ),
                Vec2::new(key.width * unit - gap * 2.0, unit - gap * 2.0),
            );
            let response = ui.interact(rect, ui.id().with(("key", key.usage)), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    ui.is_enabled(),
                    format!(
                        "{}: {}",
                        key.label,
                        self.current_label(self.layer, key.usage)
                    ),
                )
            });
            let keyboard_activate = response.has_focus()
                && ui.input(|input| {
                    input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
                });
            if response.clicked() || keyboard_activate {
                clicked = Some(key.usage);
            }
            let selected = self.selected == Some(key.usage);
            let changed = self.changed(self.layer, key.usage);
            let fill = if selected {
                SELECTED
            } else if changed {
                Color32::from_rgb(251, 235, 222)
            } else if response.hovered() {
                Color32::WHITE
            } else {
                PANEL
            };
            painter.rect(
                rect,
                2.0,
                fill,
                Stroke::new(
                    if selected || response.has_focus() {
                        2.0
                    } else {
                        1.0
                    },
                    if selected || response.has_focus() {
                        ACCENT
                    } else {
                        RULE
                    },
                ),
                StrokeKind::Inside,
            );
            painter.text(
                rect.left_top() + Vec2::new(6.0, 5.0),
                Align2::LEFT_TOP,
                key.label,
                FontId::proportional(11.0),
                INK,
            );
            if unit >= 34.0 && key.usage != FN_PLACEHOLDER_USAGE {
                let assigned = self.current_label(self.layer, key.usage);
                let preview: String = assigned
                    .chars()
                    .take((key.width * unit / 7.0) as usize)
                    .collect();
                painter.text(
                    rect.left_bottom() + Vec2::new(6.0, -5.0),
                    Align2::LEFT_BOTTOM,
                    preview,
                    FontId::proportional(9.0),
                    if changed { ACCENT } else { MUTED },
                );
            }
        }
        if let Some(usage) = clicked {
            self.select(Some(usage));
        }
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(285.0);
        ui.label(
            egui::RichText::new("INSPECTOR")
                .small()
                .strong()
                .color(MUTED),
        );
        ui.separator();
        let Some(usage) = self.selected else {
            ui.add_space(12.0);
            ui.label("Select a key in the layout to inspect or stage a binding.");
            return;
        };
        let key_name = self
            .keys
            .iter()
            .find(|key| key.usage == usage)
            .map_or("Key", |key| key.label);
        ui.label(egui::RichText::new(key_name).size(21.0).strong().color(INK));
        if let Some(slot) = self.slot(usage) {
            ui.label(
                egui::RichText::new(format!("MATRIX SLOT {slot:03}  ·  {}", self.layer.name()))
                    .small()
                    .color(MUTED),
            );
            ui.add_space(8.0);
            ui.label(format!(
                "Current: {}",
                self.current_label(self.layer, usage)
            ));
            if let Some(bytes) = self.binding(self.layer, usage) {
                ui.monospace(format_bytes(bytes));
            }
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("ASSIGN A KEY OR SHORTCUT")
                    .small()
                    .strong()
                    .color(MUTED),
            );
            ui.horizontal_wrapped(|ui| {
                ui.label("Modifiers");
                let count = self.modifiers.iter().filter(|on| **on).count();
                for (index, label) in ["Ctrl", "Shift", "Alt", "Win"].iter().enumerate() {
                    ui.add_enabled_ui(!self.busy && (self.modifiers[index] || count < 2), |ui| {
                        ui.checkbox(&mut self.modifiers[index], *label);
                    });
                }
            });
            ui.label(
                egui::RichText::new("Choose up to two modifiers, then select a key.")
                    .small()
                    .color(MUTED),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search keys, including F13–F24…"),
            );
            let query = self.search.trim().to_ascii_lowercase();
            let two_modifiers = self.modifiers.iter().filter(|on| **on).count() == 2;
            let options: Vec<_> = (0x04..=0x73)
                .chain(0xe0..=0xe7)
                .filter(|usage| !two_modifiers || *usage < 0xe0)
                .filter_map(|usage| {
                    let label = layout::usage_label(usage);
                    (!label.starts_with("0x")
                        && (query.is_empty() || label.to_ascii_lowercase().contains(&query)))
                    .then_some((label, usage))
                })
                .collect();
            egui::ScrollArea::vertical()
                .max_height(130.0)
                .show(ui, |ui| {
                    ui.add_enabled_ui(!self.busy && self.observed.is_some(), |ui| {
                        for (label, usage) in options {
                            if ui
                                .selectable_label(false, format!("{}   {:02X}", label, usage))
                                .clicked()
                            {
                                self.stage_key(usage);
                            }
                        }
                    });
                });
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("MEDIA / POINTER / SPECIAL")
                    .small()
                    .strong()
                    .color(MUTED),
            );
            ui.add_enabled_ui(!self.busy && self.observed.is_some(), |ui| {
                if ui.button("DISABLE KEY").clicked() {
                    self.set_binding([0, 0, 0, 0]);
                }
                egui::ScrollArea::vertical()
                    .max_height(110.0)
                    .show(ui, |ui| {
                        for preset in actions::presets() {
                            if ui.selectable_label(false, preset.label).clicked() {
                                self.set_binding(preset.bytes);
                            }
                        }
                    });
            });
            ui.add_space(8.0);
            ui.collapsing("Advanced · four raw bytes", |ui| {
                ui.label("Hex bytes, separated by spaces. Editing replaces this binding only.");
                ui.add(
                    egui::TextEdit::singleline(&mut self.raw_editor)
                        .font(egui::TextStyle::Monospace),
                );
                let parsed = parse_bytes(&self.raw_editor);
                if ui
                    .add_enabled(
                        !self.busy && parsed.is_some(),
                        egui::Button::new("STAGE RAW BINDING"),
                    )
                    .clicked()
                {
                    self.set_binding(parsed.expect("button disabled for invalid bytes"));
                }
                if parsed.is_none() {
                    ui.label(egui::RichText::new("Enter exactly four hex bytes.").color(ACCENT));
                }
            });
        } else {
            ui.label("No verified matrix slot is available for this key.");
        }
    }

    fn changes(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("STAGED CHANGES")
                .small()
                .strong()
                .color(MUTED),
        );
        ui.separator();
        let Some(observed) = &self.observed else {
            ui.label("Waiting for a device read.");
            return;
        };
        let mut rows = Vec::new();
        for layer in [Layer::Base, Layer::Function] {
            let (before, after) = match layer {
                Layer::Base => (&observed.base, &self.base),
                Layer::Function => (&observed.function, &self.function),
            };
            for (slot, (old, new)) in before.iter().zip(after).enumerate() {
                if old != new {
                    let key = self
                        .keys
                        .iter()
                        .find(|key| board::slot_for_usage(key.usage) == Some(slot))
                        .map_or("Unknown", |key| key.label);
                    rows.push(format!(
                        "{} · {} (slot {})   {}  →  {}",
                        layer.name(),
                        key,
                        slot,
                        format_bytes(*old),
                        format_bytes(*new)
                    ));
                }
            }
        }
        if rows.is_empty() {
            ui.label("No changes staged.");
        } else {
            egui::ScrollArea::vertical()
                .max_height(100.0)
                .show(ui, |ui| {
                    for row in rows {
                        ui.monospace(row);
                    }
                });
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal(|ui| {
            let can_apply = self.observed.is_some() && !self.busy && self.dirty_count() > 0;
            if ui
                .add_enabled(can_apply, egui::Button::new("APPLY TO KEYBOARD  Ctrl+S"))
                .clicked()
            {
                self.start_apply(ui.ctx());
            }
            if ui
                .add_enabled(
                    !self.busy && self.dirty_count() > 0,
                    egui::Button::new("REVERT DRAFT"),
                )
                .clicked()
            {
                self.revert();
            }
            ui.label(egui::RichText::new(&self.status).color(if self.error {
                ACCENT
            } else {
                MUTED
            }));
        });
    }
}

impl eframe::App for Workbench {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_worker();
        let ctrl_s = ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::S));
        let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
        if ctrl_s {
            self.start_apply(ui.ctx());
        }
        if escape {
            self.select(None);
        }
        ui.ctx().set_visuals(egui::Visuals::light());
        ui.painter().rect_filled(ui.max_rect(), 0.0, PAPER);
        egui::Frame::NONE
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                self.header(ui);
                self.layer_bar(ui);
                ui.add_space(12.0);
                let board_width = (ui.available_width() - 320.0).max(500.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(board_width);
                        self.keyboard(ui);
                        ui.add_space(12.0);
                        self.changes(ui);
                        ui.add_space(10.0);
                        ui.collapsing("Keyboard input test", |ui| {
                            ui.label(
                                "Click here and type to check host input after applying a binding.",
                            );
                            ui.add(
                                egui::TextEdit::multiline(&mut self.test_input)
                                    .hint_text("Type here…")
                                    .desired_rows(2),
                            );
                        });
                    });
                    ui.add_space(16.0);
                    self.inspector(ui);
                });
                ui.add_space(12.0);
                self.footer(ui);
            });
        if self.busy {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    }
}

fn format_bytes(bytes: [u8; 4]) -> String {
    format!(
        "{:02X} {:02X} {:02X} {:02X}",
        bytes[0], bytes[1], bytes[2], bytes[3]
    )
}

fn modifier_label(usage: u8) -> &'static str {
    match usage {
        224 => "Ctrl",
        225 => "Shift",
        226 => "Alt",
        227 => "Win",
        _ => "?",
    }
}

fn parse_bytes(input: &str) -> Option<[u8; 4]> {
    let parts: Vec<_> = input.split_ascii_whitespace().collect();
    if parts.len() != 4 || parts.iter().any(|part| part.len() != 2) {
        return None;
    }
    let bytes: Vec<_> = parts
        .iter()
        .map(|part| u8::from_str_radix(part, 16))
        .collect::<Result<_, _>>()
        .ok()?;
    bytes.try_into().ok()
}

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1320.0, 760.0])
            .with_min_inner_size([1000.0, 650.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Byakko · Nia87",
        options,
        Box::new(|cc| Ok(Box::new(Workbench::new(&cc.egui_ctx)))),
    )
}
