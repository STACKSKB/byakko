//! Native custom-picture editor. All 128 matrix colors remain in the draft;
//! physical keys are a view over slots resolved by the fixed board map.

use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::{
    board, device,
    layout::{self, PhysicalKey},
};

const INK: Color32 = Color32::from_rgb(33, 42, 46);
const MUTED: Color32 = Color32::from_rgb(96, 107, 109);
const PANEL: Color32 = Color32::from_rgb(252, 251, 246);
const RULE: Color32 = Color32::from_rgb(200, 205, 200);
const ACCENT: Color32 = Color32::from_rgb(199, 91, 45);

enum WorkerResult {
    Read(Result<Vec<[u8; 3]>, String>),
    Applied(Result<Vec<[u8; 3]>, String>),
}

pub struct PictureEditor {
    keys: Vec<PhysicalKey>,
    selected: u8,
    observed: Option<Vec<[u8; 3]>>,
    draft: Vec<[u8; 3]>,
    backup_dir: PathBuf,
    status: String,
    error: bool,
    busy: bool,
    trusted: bool,
    first_read_started: bool,
    tx: Sender<WorkerResult>,
    rx: Receiver<WorkerResult>,
}

impl PictureEditor {
    pub fn new_with_backup_dir(backup_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            keys: layout::nia87_keys(),
            selected: 0x29, // Esc has a verified slot at index 0.
            observed: None,
            draft: Vec::new(),
            backup_dir,
            status: "Open Picture to read the keyboard's stored custom colors.".into(),
            error: false,
            busy: false,
            trusted: false,
            first_read_started: false,
            tx,
            rx,
        }
    }

    #[cfg(test)]
    pub fn new() -> Self {
        Self::new_with_backup_dir(std::env::temp_dir().join("byakko-test-backups"))
    }

    pub fn busy(&self) -> bool {
        self.busy
    }

    fn selected_slot(&self) -> Option<usize> {
        board::slot_for_usage(self.selected).filter(|slot| *slot < self.draft.len())
    }

    fn dirty_count(&self) -> usize {
        self.observed.as_ref().map_or(0, |observed| {
            observed
                .iter()
                .zip(&self.draft)
                .filter(|(before, after)| before != after)
                .count()
        })
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.error = false;
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.error = true;
    }

    fn start_read(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        self.first_read_started = true;
        self.busy = true;
        self.set_status("Reading all 128 picture colors twice…");
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                device::read_picture().map_err(|error| error.to_string())
            }))
            .unwrap_or_else(|_| Err("Picture read panicked; device state is unverified.".into()));
            let _ = tx.send(WorkerResult::Read(result));
            ctx.request_repaint();
        });
    }

    fn start_apply(&mut self, ctx: &egui::Context) {
        if self.busy || !self.trusted || self.dirty_count() == 0 {
            return;
        }
        let Some(expected) = self.observed.clone() else {
            return;
        };
        let desired = self.draft.clone();
        let backup_dir = self.backup_dir.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.busy = true;
        self.set_status("Backing up, writing changed colors, and verifying all 128 slots…");
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                device::apply_picture(&expected, &desired, &backup_dir)
                    .map_err(|error| error.to_string())
            }))
            .unwrap_or_else(|_| Err("Picture apply panicked; restoration is unverified. Inspect the backup before retrying.".into()));
            let _ = tx.send(WorkerResult::Applied(result));
            ctx.request_repaint();
        });
    }

    pub fn handle_close(&self, ctx: &egui::Context) {
        if self.busy && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }

    fn poll_worker(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            self.busy = false;
            match message {
                WorkerResult::Read(Ok(colors)) => {
                    if colors.len() != 128 {
                        self.trusted = false;
                        self.set_error(format!(
                            "Expected 128 picture colors; got {}.",
                            colors.len()
                        ));
                        continue;
                    }
                    self.draft = colors.clone();
                    self.observed = Some(colors);
                    self.trusted = true;
                    self.set_status(
                        "Picture read twice and matched. Select a key to edit its stored color.",
                    );
                }
                WorkerResult::Applied(Ok(colors)) => {
                    if colors.len() != 128 || self.draft.len() != 128 || colors != self.draft {
                        self.trusted = false;
                        self.set_error("Picture apply returned a mismatched readback; device state is unverified. Draft retained. Re-read before another apply.");
                        continue;
                    }
                    self.draft = colors.clone();
                    self.observed = Some(colors);
                    self.trusted = true;
                    self.set_status(format!(
                        "Picture readback matched all 128 slots. Backup in {}",
                        self.backup_dir.display()
                    ));
                }
                WorkerResult::Read(Err(error)) => {
                    self.trusted = false;
                    self.set_error(format!("Picture read failed: {error}"));
                }
                WorkerResult::Applied(Err(error)) => {
                    self.trusted = false;
                    self.set_error(format!(
                        "Picture apply failed: {error}. Re-read before another apply."
                    ));
                }
            }
        }
    }

    fn revert(&mut self) {
        if let Some(observed) = &self.observed {
            self.draft.clone_from(observed);
            self.set_status("Picture draft reverted to the last verified read.");
        }
    }

    fn keyboard(&mut self, ui: &mut egui::Ui, width: f32) {
        let unit = (width / 18.5).clamp(27.0, 46.0);
        let gap = 2.0;
        let (canvas, _) =
            ui.allocate_exact_size(Vec2::new(18.5 * unit, 6.5 * unit), Sense::hover());
        let mut clicked = None;
        for key in &self.keys {
            let Some(slot) =
                board::slot_for_usage(key.usage).filter(|slot| *slot < self.draft.len())
            else {
                continue;
            };
            let rect = Rect::from_min_size(
                Pos2::new(
                    canvas.min.x + key.x * unit + gap,
                    canvas.min.y + key.y * unit + gap,
                ),
                Vec2::new(key.width * unit - 2.0 * gap, unit - 2.0 * gap),
            );
            let response = ui.interact(
                rect,
                ui.id().with(("picture_key", key.usage)),
                Sense::click(),
            );
            let rgb = self.draft[slot];
            let changed = self
                .observed
                .as_ref()
                .is_some_and(|before| before[slot] != rgb);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    ui.is_enabled(),
                    format!(
                        "{}: color #{:02X}{:02X}{:02X}, matrix slot {}",
                        key.label, rgb[0], rgb[1], rgb[2], slot
                    ),
                )
            });
            let activate = response.has_focus()
                && ui.input(|input| {
                    input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
                });
            if response.clicked() || activate {
                clicked = Some(key.usage);
            }
            let selected = self.selected == key.usage;
            let fill = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            let text_color =
                if (rgb[0] as u32 * 299 + rgb[1] as u32 * 587 + rgb[2] as u32 * 114) / 1000 > 145 {
                    INK
                } else {
                    Color32::WHITE
                };
            ui.painter().rect(
                rect,
                2.0,
                fill,
                Stroke::new(
                    if selected || response.has_focus() {
                        2.5
                    } else if changed {
                        2.0
                    } else {
                        1.0
                    },
                    if selected || response.has_focus() {
                        ACCENT
                    } else if changed {
                        INK
                    } else {
                        RULE
                    },
                ),
                StrokeKind::Inside,
            );
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                key.label,
                FontId::proportional(11.0),
                text_color,
            );
        }
        if let Some(usage) = clicked {
            self.selected = usage;
        }
    }

    fn inspector(&mut self, ui: &mut egui::Ui, enabled: bool) {
        ui.set_min_width(230.0);
        ui.label(
            egui::RichText::new("SELECTED KEY")
                .small()
                .strong()
                .color(MUTED),
        );
        ui.separator();
        let Some(slot) = self.selected_slot() else {
            ui.label("Select a key in the layout.");
            return;
        };
        let label = self
            .keys
            .iter()
            .find(|key| key.usage == self.selected)
            .map_or("Key", |key| key.label);
        ui.label(egui::RichText::new(label).size(20.0).strong().color(INK));
        ui.label(
            egui::RichText::new(format!("MATRIX SLOT {slot:03}"))
                .small()
                .color(MUTED),
        );
        let original = self.observed.as_ref().map(|colors| colors[slot]);
        if let Some(original) = original {
            ui.label(format!(
                "Device  #{:02X}{:02X}{:02X}",
                original[0], original[1], original[2]
            ));
        }
        ui.add_space(9.0);
        ui.label(
            egui::RichText::new("DRAFT COLOR")
                .small()
                .strong()
                .color(MUTED),
        );
        ui.add_enabled_ui(enabled, |ui| {
            let rgb = &mut self.draft[slot];
            ui.color_edit_button_srgb(rgb);
            ui.horizontal(|ui| {
                for (index, label) in ["R", "G", "B"].iter().enumerate() {
                    ui.label(*label);
                    ui.add(
                        egui::DragValue::new(&mut rgb[index])
                            .range(0..=255)
                            .speed(1),
                    );
                }
            });
        });
        let color = self.draft[slot];
        ui.monospace(format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]));
        ui.add_space(12.0);
        if ui
            .add_enabled(enabled, egui::Button::new("FILL ALL PHYSICAL KEYS"))
            .clicked()
        {
            for key in &self.keys {
                if let Some(index) = board::slot_for_usage(key.usage).filter(|index| *index < 126) {
                    self.draft[index] = color;
                }
            }
            self.set_status(
                "The selected color is staged on all physical keys. Review before applying.",
            );
        }
        ui.label(
            egui::RichText::new("Unshown matrix slots retain their original colors.")
                .small()
                .color(MUTED),
        );
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, blocked: bool) {
        self.handle_close(ui.ctx());
        self.poll_worker();
        if !self.first_read_started && !blocked && !self.busy {
            self.start_read(ui.ctx());
        }
        let can_work = !blocked && !self.busy;
        egui::Frame::NONE.fill(PANEL).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
            ui.label(egui::RichText::new("PICTURE STUDIO").size(17.0).strong().color(INK));
            ui.label(egui::RichText::new("Edit the stored custom picture. The keyboard may show another effect until you select a custom picture as its global mode.").color(MUTED));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = if self.dirty_count() > 0 { "RE-READ / DISCARD DRAFT" } else { "RE-READ DEVICE" };
                if ui.add_enabled(can_work, egui::Button::new(label)).clicked() { self.start_read(ui.ctx()); }
                if self.busy { ui.spinner(); }
                ui.label(if self.trusted { "Verified 128 colors" } else { "Read-only until verified" });
                ui.label(egui::RichText::new(format!("{} staged slot(s)", self.dirty_count())).color(if self.dirty_count() > 0 { ACCENT } else { MUTED }));
            });
            ui.separator();
            if self.observed.is_some() && self.draft.len() == 128 {
                let board_width = (ui.available_width() - 255.0).max(500.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(board_width);
                        self.keyboard(ui, board_width);
                    });
                    ui.add_space(10.0);
                    self.inspector(ui, can_work && self.trusted);
                });
                ui.add_space(9.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(can_work && self.trusted && self.dirty_count() > 0, egui::Button::new("APPLY PICTURE TO KEYBOARD")).clicked() {
                        self.start_apply(ui.ctx());
                    }
                    if ui.add_enabled(can_work && self.dirty_count() > 0, egui::Button::new("REVERT DRAFT")).clicked() {
                        self.revert();
                    }
                });
                ui.label(egui::RichText::new(format!("Apply stores a before-image in {} and verifies all 128 colors.", self.backup_dir.display())).small().color(MUTED));
            } else {
                ui.label("Waiting for a matching picture read.");
            }
            ui.add_space(8.0);
            ui.label(egui::RichText::new(&self.status).color(if self.error { ACCENT } else { INK }));
        });
        self.handle_close(ui.ctx());
    }
}

#[cfg(test)]
impl Default for PictureEditor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close_frame(editor: &mut PictureEditor) -> egui::FullOutput {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let mut output = ctx.run_ui(input, |ui| editor.handle_close(ui.ctx()));
        output.textures_delta.clear();
        output
    }

    #[test]
    fn close_is_cancelled_before_same_frame_error_completion() {
        let mut editor = PictureEditor::new();
        editor.observed = Some(vec![[1, 2, 3]; 128]);
        editor.draft = vec![[4, 5, 6]; 128];
        let draft_before = editor.draft.clone();
        editor.busy = true;
        editor
            .tx
            .send(WorkerResult::Applied(Err("restore failed".into())))
            .unwrap();
        let output = close_frame(&mut editor);
        editor.poll_worker();
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(commands.contains(&egui::ViewportCommand::CancelClose));
        assert!(!editor.busy);
        assert!(editor.error);
        assert_eq!(editor.draft, draft_before);
    }

    #[test]
    fn applied_mismatch_retains_picture_draft_and_prior_observation() {
        let mut editor = PictureEditor::new();
        let prior = vec![[1, 2, 3]; 128];
        let draft = vec![[4, 5, 6]; 128];
        editor.observed = Some(prior.clone());
        editor.draft = draft.clone();
        let mut wrong = draft.clone();
        wrong[91] = [9, 9, 9];
        editor.tx.send(WorkerResult::Applied(Ok(wrong))).unwrap();
        editor.poll_worker();
        assert_eq!(editor.observed, Some(prior));
        assert_eq!(editor.draft, draft);
        assert!(!editor.trusted && editor.error && editor.status.contains("unverified"));
    }

    #[test]
    fn applied_exact_picture_accepts_all_slots() {
        let mut editor = PictureEditor::new();
        editor.observed = Some(vec![[1, 2, 3]; 128]);
        editor.draft = vec![[4, 5, 6]; 128];
        editor
            .tx
            .send(WorkerResult::Applied(Ok(editor.draft.clone())))
            .unwrap();
        editor.poll_worker();
        assert_eq!(editor.observed.as_ref(), Some(&editor.draft));
        assert!(editor.trusted && !editor.error);
    }
}
