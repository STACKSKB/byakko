//! Native Nia87 scalar-settings workbench. All device transactions run on
//! workers; each writable setting has its own staged value and apply action.

use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
};

use eframe::egui::{self, Color32, RichText};

use crate::{
    device,
    settings::{self, Setting, Settings},
};

const INK: Color32 = Color32::from_rgb(33, 42, 46);
const MUTED: Color32 = Color32::from_rgb(96, 107, 109);
const PANEL: Color32 = Color32::from_rgb(252, 251, 246);
const ACCENT: Color32 = Color32::from_rgb(199, 91, 45);

enum WorkerResult {
    Read(Result<Settings, String>),
    Applied(Setting, Result<Settings, String>),
}

pub struct SettingsEditor {
    observed: Option<Settings>,
    draft_debounce: Option<u8>,
    draft_auto: Option<bool>,
    backup_dir: PathBuf,
    status: String,
    error: bool,
    busy: bool,
    trusted: bool,
    first_read_started: bool,
    tx: Sender<WorkerResult>,
    rx: Receiver<WorkerResult>,
}

impl SettingsEditor {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            observed: None,
            draft_debounce: None,
            draft_auto: None,
            backup_dir: std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("backups"),
            status: "Open Settings to read the keyboard's scalar settings.".into(),
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

    fn debounce_dirty(&self) -> bool {
        self.observed
            .as_ref()
            .zip(self.draft_debounce)
            .is_some_and(|(current, draft)| current.debounce() != draft)
    }

    fn auto_dirty(&self) -> bool {
        self.observed
            .as_ref()
            .zip(self.draft_auto)
            .is_some_and(|(current, draft)| current.auto_os() != draft)
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
        self.set_status("Reading four settings twice with identity barriers…");
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = device::read_settings().map_err(|error| error.to_string());
            let _ = tx.send(WorkerResult::Read(result));
            ctx.request_repaint();
        });
    }

    fn start_apply(&mut self, ctx: &egui::Context, setting: Setting) {
        if self.busy || !self.trusted {
            return;
        }
        let dirty = match setting {
            Setting::Debounce(_) => self.debounce_dirty(),
            Setting::AutoOs(_) => self.auto_dirty(),
        };
        if !dirty || settings::write_report(setting).is_err() {
            return;
        }
        let Some(expected) = self.observed.clone() else {
            return;
        };
        self.busy = true;
        self.set_status("Backing up, writing, and verifying one setting…");
        let backup_dir = self.backup_dir.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = device::apply_setting(&expected, setting, &backup_dir)
                .map_err(|error| error.to_string());
            let _ = tx.send(WorkerResult::Applied(setting, result));
            ctx.request_repaint();
        });
    }

    fn accept_read(&mut self, updated: Settings, applied: Option<Setting>) {
        let debounce_clean = !self.debounce_dirty();
        let auto_clean = !self.auto_dirty();
        if debounce_clean || matches!(applied, Some(Setting::Debounce(_))) {
            self.draft_debounce = Some(updated.debounce());
        }
        if auto_clean || matches!(applied, Some(Setting::AutoOs(_))) {
            self.draft_auto = Some(updated.auto_os());
        }
        self.observed = Some(updated);
        self.trusted = true;
    }

    fn poll_worker(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            self.busy = false;
            match message {
                WorkerResult::Read(Ok(updated)) => {
                    self.accept_read(updated, None);
                    self.set_status(
                        "Device settings matched across two reads. Drafts are local until applied.",
                    );
                }
                WorkerResult::Read(Err(error)) => {
                    self.trusted = false;
                    self.set_error(format!("Settings read failed: {error}"));
                }
                WorkerResult::Applied(setting, Ok(updated)) => {
                    self.accept_read(updated, Some(setting));
                    self.set_status(format!(
                        "Setting readback matched. Backup saved in {}",
                        self.backup_dir.display()
                    ));
                }
                WorkerResult::Applied(_, Err(error)) => {
                    self.trusted = false;
                    self.set_error(format!(
                        "Setting apply failed: {error}. Re-read before another apply."
                    ));
                }
            }
        }
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
                ui.label(RichText::new("SETTINGS / NIA87").size(17.0).strong().color(INK));
                ui.label(RichText::new("Profile 0 · raw device reads · staged writes").color(MUTED));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(can_work, egui::Button::new("RE-READ DEVICE")).clicked() {
                        self.start_read(ui.ctx());
                    }
                    if self.busy { ui.spinner(); }
                    ui.label(if self.trusted { "Verified read" } else { "Writes unavailable until verified read" });
                });
                ui.separator();

                if let Some(observed) = self.observed.clone() {
                    let device_debounce = observed.debounce();
                    let device_auto = observed.auto_os();
                    ui.label(RichText::new("WRITABLE / EACH CHANGE IS SEPARATE").small().strong().color(MUTED));
                    egui::Grid::new("settings_writable")
                        .num_columns(4)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Debounce");
                            ui.label(format!("Device {device_debounce} ms"));
                            if let Some(draft) = self.draft_debounce.as_mut() {
                                ui.add_enabled(can_work && self.trusted, egui::Slider::new(draft, 1..=10).text("ms"));
                            }
                            ui.horizontal(|ui| {
                                if ui.add_enabled(can_work && self.trusted && self.debounce_dirty(), egui::Button::new("APPLY DEBOUNCE")).clicked()
                                    && let Some(value) = self.draft_debounce
                                {
                                    self.start_apply(ui.ctx(), Setting::Debounce(value));
                                }
                                if ui.add_enabled(can_work && self.debounce_dirty(), egui::Button::new("REVERT")).clicked() {
                                    self.draft_debounce = Some(device_debounce);
                                }
                            });
                            ui.end_row();

                            ui.label("Auto OS");
                            ui.label(if device_auto { "Device on" } else { "Device off" });
                            if let Some(draft) = self.draft_auto.as_mut() {
                                ui.add_enabled_ui(can_work && self.trusted, |ui| {
                                    ui.checkbox(draft, "Automatic detection");
                                });
                            }
                            ui.horizontal(|ui| {
                                if ui.add_enabled(can_work && self.trusted && self.auto_dirty(), egui::Button::new("APPLY AUTO OS")).clicked()
                                    && let Some(value) = self.draft_auto
                                {
                                    self.start_apply(ui.ctx(), Setting::AutoOs(value));
                                }
                                if ui.add_enabled(can_work && self.auto_dirty(), egui::Button::new("REVERT")).clicked() {
                                    self.draft_auto = Some(device_auto);
                                }
                            });
                            ui.end_row();
                        });
                    ui.add_space(8.0);
                    ui.label(RichText::new("READ-ONLY / STORED VALUES").small().strong().color(MUTED));
                    let sleep = observed.sleep_seconds();
                    egui::Grid::new("settings_read_only")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            for (label, seconds) in [
                                ("Bluetooth sleep", sleep[0]),
                                ("2.4 GHz sleep", sleep[1]),
                                ("Bluetooth deep sleep", sleep[2]),
                                ("2.4 GHz deep sleep", sleep[3]),
                            ] {
                                ui.label(label);
                                ui.label(if seconds == 0 { "Disabled".into() } else { format!("{seconds} s") });
                                ui.end_row();
                            }
                            ui.label("Keyboard options");
                            ui.monospace(format!("profile {} · flags {:02X} · Fn matrix {} · power save {}",
                                observed.option_profile(), observed.option_flags(),
                                if observed.fn_matrix_enabled() { "on" } else { "off" },
                                observed.power_save_value()));
                            ui.end_row();
                        });
                    ui.label(RichText::new("Debounce and Auto OS writes passed reversible readback tests on the attached Nia87. Sleep and keyboard-option writes remain unavailable; the sleep setter's checksum placement is unresolved.").small().color(MUTED));
                    ui.add_space(8.0);
                    ui.collapsing("RAW SETTINGS REPLIES · read-only", |ui| {
                        for (name, opcode) in [
                            ("Debounce", settings::DEBOUNCE_READ),
                            ("Auto OS", settings::AUTO_OS_READ),
                            ("Sleep", settings::SLEEP_READ),
                            ("Options", settings::OPTIONS_READ),
                        ] {
                            ui.label(RichText::new(name).small().strong().color(MUTED));
                            if let Some(raw) = observed.raw_reply(opcode) {
                                for chunk in raw.chunks(16) {
                                    ui.monospace(chunk.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(" "));
                                }
                            }
                        }
                    });
                } else {
                    ui.label("Waiting for a matching device read.");
                }
                ui.add_space(8.0);
                ui.label(RichText::new(&self.status).color(if self.error { ACCENT } else { INK }));
            });
    }
}

impl Default for SettingsEditor {
    fn default() -> Self {
        Self::new()
    }
}
