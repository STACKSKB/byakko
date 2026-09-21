//! Dense native editor for the Nia87's global lighting setting.
//! Device transactions run on workers; controls edit a local draft only.

use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
};

use eframe::egui::{self, Color32, RichText};

use crate::{
    device,
    lighting::{self, Effect, Lighting, LightingSetting},
};

const INK: Color32 = Color32::from_rgb(33, 42, 46);
const MUTED: Color32 = Color32::from_rgb(96, 107, 109);
const PANEL: Color32 = Color32::from_rgb(252, 251, 246);
const ACCENT: Color32 = Color32::from_rgb(199, 91, 45);

enum WorkerResult {
    Read(Result<Lighting, String>),
    Applied(Result<Lighting, String>),
}

pub struct LightingEditor {
    observed: Option<Lighting>,
    loaded: Option<LightingSetting>,
    draft: Option<LightingSetting>,
    backup_dir: PathBuf,
    status: String,
    error: bool,
    busy: bool,
    trusted: bool,
    first_read_started: bool,
    tx: Sender<WorkerResult>,
    rx: Receiver<WorkerResult>,
}

impl LightingEditor {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            observed: None,
            loaded: None,
            draft: None,
            backup_dir: std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("backups"),
            status: "Open Lighting to read the keyboard's current mode.".into(),
            error: false,
            busy: false,
            trusted: false,
            first_read_started: false,
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
            .zip(self.draft.as_ref())
            .is_some_and(|(loaded, draft)| loaded != draft)
    }

    fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
        self.error = false;
    }

    fn set_error(&mut self, status: impl Into<String>) {
        self.status = status.into();
        self.error = true;
    }

    fn start_read(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        self.first_read_started = true;
        self.busy = true;
        self.set_status("Reading global LED settings twice with identity barriers…");
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = device::read_lighting().map_err(|error| error.to_string());
            let _ = tx.send(WorkerResult::Read(result));
            ctx.request_repaint();
        });
    }

    fn start_apply(&mut self, ctx: &egui::Context) {
        if self.busy || !self.trusted || !self.dirty() {
            return;
        }
        let (Some(expected), Some(setting)) = (self.observed.clone(), self.draft.clone()) else {
            return;
        };
        if let Err(error) = lighting::write_report(&setting) {
            self.set_error(error);
            return;
        }
        self.busy = true;
        self.set_status("Backing up, writing, and verifying the lighting setting…");
        let backup_dir = self.backup_dir.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = device::apply_lighting(&expected, &setting, &backup_dir)
                .map_err(|error| error.to_string());
            let _ = tx.send(WorkerResult::Applied(result));
            ctx.request_repaint();
        });
    }

    fn poll_worker(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            self.busy = false;
            match message {
                WorkerResult::Read(Ok(observed)) => {
                    let decoded = observed.recognized_setting();
                    self.trusted = decoded.is_some();
                    self.loaded = decoded.clone();
                    self.draft = decoded;
                    self.observed = Some(observed);
                    if self.trusted {
                        self.set_status(
                            "Lighting read twice and matched. Edit a draft, then apply explicitly.",
                        );
                    } else {
                        self.set_error("This LED response is not in the Nia87 mode catalog. Raw bytes are shown read-only.");
                    }
                }
                WorkerResult::Read(Err(error)) => {
                    self.trusted = false;
                    self.set_error(format!("Lighting read failed: {error}"));
                }
                WorkerResult::Applied(Ok(observed)) => {
                    let decoded = observed.recognized_setting();
                    self.trusted = decoded.is_some();
                    self.loaded = decoded.clone();
                    self.draft = decoded;
                    self.observed = Some(observed);
                    if self.trusted {
                        self.set_status(format!(
                            "Lighting settings read back and matched. Backup in {}",
                            self.backup_dir.display()
                        ));
                    } else {
                        self.set_error(
                            "Write readback contained an unrecognized mode. Reload before editing.",
                        );
                    }
                }
                WorkerResult::Applied(Err(error)) => {
                    self.trusted = false;
                    self.set_error(format!(
                        "Lighting apply failed: {error}. Re-read before another apply."
                    ));
                }
            }
        }
    }

    fn revert(&mut self) {
        if let Some(loaded) = self.loaded.clone() {
            self.draft = Some(loaded);
            self.set_status("Lighting draft reverted to the last verified read.");
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui, enabled: bool) {
        let Some(draft) = self.draft.as_mut() else {
            return;
        };
        let effect = lighting::effect_by_id(draft.effect_id).expect("draft has catalog effect");
        ui.add_enabled_ui(enabled, |ui| {
            egui::Grid::new("lighting_controls")
                .num_columns(2)
                .spacing([16.0, 7.0])
                .show(ui, |ui| {
                    ui.label("Effect");
                    let mut selected = draft.effect_id;
                    egui::ComboBox::from_id_salt("lighting_effect")
                        .selected_text(friendly_name(effect))
                        .width(215.0)
                        .show_ui(ui, |ui| {
                            for candidate in lighting::EFFECTS {
                                ui.selectable_value(
                                    &mut selected,
                                    candidate.id,
                                    friendly_name(candidate),
                                );
                            }
                        });
                    if selected != draft.effect_id {
                        let candidate =
                            lighting::effect_by_id(selected).expect("catalog selection");
                        *draft = default_for(candidate, Some(draft));
                    }
                    ui.end_row();

                    let effect =
                        lighting::effect_by_id(draft.effect_id).expect("catalog selection");
                    if let Some(value) = draft.value.as_mut() {
                        ui.label("Brightness");
                        ui.add(egui::Slider::new(value, 0..=4).show_value(true));
                        ui.end_row();
                    }
                    if let Some(speed) = draft.speed.as_mut() {
                        ui.label("Speed");
                        ui.add(egui::Slider::new(speed, 0..=4).show_value(true));
                        ui.end_row();
                    }
                    if let Some(option) = draft.option.as_mut() {
                        ui.label("Pattern");
                        egui::ComboBox::from_id_salt("lighting_option")
                            .selected_text(effect.options[*option as usize])
                            .width(160.0)
                            .show_ui(ui, |ui| {
                                for (index, label) in effect.options.iter().enumerate() {
                                    ui.selectable_value(option, index as u8, *label);
                                }
                            });
                        ui.end_row();
                    }
                    if let Some(rgb) = draft.rgb.as_mut() {
                        ui.label("Color");
                        ui.horizontal(|ui| {
                            ui.color_edit_button_srgb(rgb);
                            for (index, label) in ["R", "G", "B"].iter().enumerate() {
                                ui.label(*label);
                                ui.add(
                                    egui::DragValue::new(&mut rgb[index])
                                        .range(0..=255)
                                        .speed(1),
                                );
                            }
                        });
                        ui.end_row();
                    }
                    if effect.dazzle {
                        ui.label("Dazzle");
                        ui.checkbox(&mut draft.dazzle, "Enabled");
                        ui.end_row();
                    }
                });
        });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, blocked: bool) {
        self.poll_worker();
        if !self.first_read_started && !blocked && !self.busy {
            self.start_read(ui.ctx());
        }
        let can_work = !blocked && !self.busy;
        egui::Frame::NONE
            .fill(PANEL)
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.label(RichText::new("LIGHTING LAB").size(17.0).strong().color(INK));
                ui.label(RichText::new("Global LED mode · Nia87 profile 0").color(MUTED));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let reload_label = if self.dirty() { "RE-READ / DISCARD DRAFT" } else { "RE-READ DEVICE" };
                    if ui.add_enabled(can_work, egui::Button::new(reload_label)).clicked() {
                        self.start_read(ui.ctx());
                    }
                    if self.busy { ui.spinner(); }
                    ui.label(if self.trusted { "Verified read" } else { "Read-only until verified" });
                });
                ui.separator();

                if let Some(observed) = self.observed.as_ref() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("DEVICE").small().strong().color(MUTED));
                        let label = observed.effect().map(friendly_name).unwrap_or("Unknown effect");
                        ui.label(RichText::new(label).strong().color(INK));
                        ui.monospace(format!("ID {:02X}", observed.effect_id()));
                    });
                }
                if self.loaded.is_some() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("DRAFT").small().strong().color(MUTED));
                    self.editor(ui, can_work && self.trusted);
                    ui.add_space(8.0);
                    if let (Some(loaded), Some(draft)) = (&self.loaded, &self.draft) {
                        let before = summary(loaded);
                        let after = summary(draft);
                        ui.label(RichText::new(format!("Device  {before}")).color(MUTED));
                        ui.label(RichText::new(format!("Draft   {after}")).color(if self.dirty() { ACCENT } else { MUTED }));
                    }
                    let validation = self.draft.as_ref().map(lighting::write_report);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(can_work && self.trusted && self.dirty() && validation.as_ref().is_some_and(Result::is_ok), egui::Button::new("APPLY TO KEYBOARD")).clicked() {
                            self.start_apply(ui.ctx());
                        }
                        if ui.add_enabled(can_work && self.dirty(), egui::Button::new("REVERT DRAFT")).clicked() {
                            self.revert();
                        }
                    });
                    if let Some(Err(error)) = validation {
                        ui.label(RichText::new(format!("Cannot apply: {error}")).color(ACCENT));
                    }
                    ui.label(RichText::new(format!("Apply stores a before-image in {} and verifies readback.", self.backup_dir.display())).small().color(MUTED));
                } else if self.observed.is_some() {
                    ui.label("The response is available below. Editing is disabled because its fields are not recognized.");
                } else {
                    ui.label("Waiting for a matching device read.");
                }
                if let Some(observed) = self.observed.as_ref() {
                    ui.add_space(9.0);
                    ui.collapsing("RAW LED RESPONSE · read-only", |ui| {
                        for chunk in observed.raw().chunks(16) {
                            ui.monospace(chunk.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(" "));
                        }
                    });
                }
                ui.add_space(8.0);
                ui.label(RichText::new(&self.status).color(if self.error { ACCENT } else { INK }));
            });
    }
}

impl Default for LightingEditor {
    fn default() -> Self {
        Self::new()
    }
}

fn default_for(effect: &Effect, previous: Option<&LightingSetting>) -> LightingSetting {
    LightingSetting {
        effect_id: effect.id,
        value: effect
            .value
            .then(|| previous.and_then(|draft| draft.value).unwrap_or(4)),
        speed: effect
            .speed
            .then(|| previous.and_then(|draft| draft.speed).unwrap_or(2)),
        option: (!effect.options.is_empty()).then_some(0),
        rgb: effect.rgb.then(|| {
            previous
                .and_then(|draft| draft.rgb)
                .unwrap_or([255, 255, 255])
        }),
        dazzle: effect.dazzle && previous.is_some_and(|draft| draft.dazzle),
    }
}

fn friendly_name(effect: &Effect) -> &'static str {
    match effect.id {
        0 => "Off",
        1 => "Steady",
        2 => "Breathing",
        3 => "Neon",
        4 => "Wave",
        5 => "Ripple",
        6 => "Raindrop",
        7 => "Snake",
        8 => "Press action",
        9 => "Convergence",
        10 => "Sine wave",
        11 => "Kaleidoscope",
        12 => "Line wave",
        13 => "User picture",
        14 => "Laser",
        15 => "Circle wave",
        16 => "Dazzling",
        17 => "Rain down",
        18 => "Meteor",
        19 => "Press action off",
        20 => "Music follow 3",
        21 => "Screen color",
        22 => "Music follow 2",
        _ => "Unknown",
    }
}

fn summary(setting: &LightingSetting) -> String {
    let effect = lighting::effect_by_id(setting.effect_id).expect("catalog draft");
    let mut parts = vec![friendly_name(effect).to_owned()];
    if let Some(value) = setting.value {
        parts.push(format!("brightness {value}"));
    }
    if let Some(speed) = setting.speed {
        parts.push(format!("speed {speed}"));
    }
    if let Some(option) = setting.option {
        parts.push(effect.options[option as usize].to_owned());
    }
    if let Some([r, g, b]) = setting.rgb {
        parts.push(format!("#{r:02X}{g:02X}{b:02X}"));
    }
    if setting.dazzle {
        parts.push("dazzle".into());
    }
    parts.join(" · ")
}
