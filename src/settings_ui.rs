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
    draft_sleep: Option<[u16; 4]>,
    draft_backlight: Option<bool>,
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
    pub fn new_with_backup_dir(backup_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            observed: None,
            draft_debounce: None,
            draft_auto: None,
            draft_sleep: None,
            draft_backlight: None,
            backup_dir,
            status: "Open Settings to read the keyboard's scalar settings.".into(),
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

    fn sleep_dirty(&self) -> bool {
        self.observed
            .as_ref()
            .zip(self.draft_sleep)
            .is_some_and(|(current, draft)| current.sleep_seconds() != draft)
    }

    fn backlight_dirty(&self) -> bool {
        self.observed
            .as_ref()
            .zip(self.draft_backlight)
            .is_some_and(|(current, draft)| current.with_backlight(draft) != *current)
    }

    fn valid_sleep(sleep: [u16; 4]) -> bool {
        sleep.iter().enumerate().all(|(index, &seconds)| {
            seconds == 0
                || (seconds % 60 == 0
                    && seconds <= 3600
                    && seconds >= if index < 2 { 60 } else { 600 })
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
        self.set_status("Reading settings twice with identity barriers…");
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                device::read_settings().map_err(|error| error.to_string())
            }))
            .unwrap_or_else(|_| Err("Settings read panicked; device state is unverified.".into()));
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
            Setting::Sleep(value) => self.sleep_dirty() && Self::valid_sleep(value),
            Setting::Backlight(_) => self.backlight_dirty(),
        };
        if !dirty
            || (!matches!(setting, Setting::Backlight(_))
                && settings::write_report(setting).is_err())
        {
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
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                device::apply_setting(&expected, setting, &backup_dir)
                    .map_err(|error| error.to_string())
            }))
            .unwrap_or_else(|_| Err("Settings apply panicked; restoration is unverified. Inspect the backup before retrying.".into()));
            let _ = tx.send(WorkerResult::Applied(setting, result));
            ctx.request_repaint();
        });
    }

    fn accept_read(&mut self, updated: Settings, applied: Option<Setting>) {
        let debounce_clean = !self.debounce_dirty();
        let auto_clean = !self.auto_dirty();
        let sleep_clean = !self.sleep_dirty();
        let backlight_clean = !self.backlight_dirty();
        if debounce_clean || matches!(applied, Some(Setting::Debounce(_))) {
            self.draft_debounce = Some(updated.debounce());
        }
        if auto_clean || matches!(applied, Some(Setting::AutoOs(_))) {
            self.draft_auto = Some(updated.auto_os());
        }
        if sleep_clean || matches!(applied, Some(Setting::Sleep(_))) {
            self.draft_sleep = Some(updated.sleep_seconds());
        }
        if backlight_clean || matches!(applied, Some(Setting::Backlight(_))) {
            self.draft_backlight = Some(updated.backlight_enabled());
        }
        self.observed = Some(updated);
        self.trusted = true;
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
                    let matches = match setting {
                        Setting::Debounce(value) => {
                            self.draft_debounce == Some(value) && updated.debounce() == value
                        }
                        Setting::AutoOs(value) => {
                            self.draft_auto == Some(value) && updated.auto_os() == value
                        }
                        Setting::Sleep(value) => {
                            self.draft_sleep == Some(value) && updated.sleep_seconds() == value
                        }
                        Setting::Backlight(value) => {
                            self.draft_backlight == Some(value)
                                && updated.backlight_enabled() == value
                        }
                    };
                    if !matches {
                        self.trusted = false;
                        self.set_error("Setting worker returned an unexpected value; device state is unverified. Drafts retained; re-read before another apply.");
                        continue;
                    }
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
        self.handle_close(ui.ctx());
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
                    let device_sleep = observed.sleep_seconds();
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
                    ui.label(RichText::new("SLEEP TIMERS / MINUTES").small().strong().color(MUTED));
                    egui::Grid::new("settings_sleep")
                        .num_columns(4)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            for (index, label) in ["Bluetooth sleep", "2.4 GHz sleep", "Bluetooth deep sleep", "2.4 GHz deep sleep"].into_iter().enumerate() {
                                let seconds = device_sleep[index];
                                ui.label(label);
                                ui.label(if seconds == 0 { "Device disabled".into() } else { format!("Device {seconds} s") });
                                if let Some(draft) = self.draft_sleep.as_mut() {
                                    let raw = draft[index];
                                    let valid = raw == 0 || (raw % 60 == 0 && raw <= 3600 && raw >= if index < 2 { 60 } else { 600 });
                                    ui.add_enabled_ui(can_work && self.trusted, |ui| {
                                        ui.horizontal(|ui| {
                                            if ui.selectable_label(raw == 0 && valid, "Disabled (0)").clicked() {
                                                draft[index] = 0;
                                            }
                                            let minimum = if index < 2 { 1 } else { 10 };
                                            if ui.selectable_label(valid && raw != 0, "Timed").clicked() && (raw == 0 || !valid) {
                                                draft[index] = minimum * 60;
                                            }
                                            let mut minutes = if valid && raw != 0 { raw / 60 } else { minimum };
                                            if ui.add(egui::DragValue::new(&mut minutes).range(minimum..=60).suffix(" min").speed(1.0)).changed() {
                                                draft[index] = minutes * 60;
                                            }
                                        });
                                    });
                                    if !valid {
                                        ui.label(RichText::new(format!("Unrecognized draft: {raw} s; choose a valid value")).color(ACCENT));
                                    } else {
                                        ui.label("");
                                    }
                                } else {
                                    ui.label("");
                                    ui.label("");
                                }
                                ui.end_row();
                            }
                        });
                    ui.horizontal(|ui| {
                        let valid = self.draft_sleep.is_some_and(Self::valid_sleep);
                        if ui.add_enabled(can_work && self.trusted && self.sleep_dirty() && valid, egui::Button::new("APPLY SLEEP TIMERS")).clicked()
                            && let Some(value) = self.draft_sleep
                        {
                            self.start_apply(ui.ctx(), Setting::Sleep(value));
                        }
                        if ui.add_enabled(can_work && self.sleep_dirty(), egui::Button::new("REVERT SLEEP TIMERS")).clicked() {
                            self.draft_sleep = Some(device_sleep);
                        }
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new("BACKLIGHT").small().strong().color(MUTED));
                    ui.horizontal(|ui| {
                        ui.label(if observed.backlight_enabled() { "Device on" } else { "Device off" });
                        if let Some(draft) = self.draft_backlight.as_mut() {
                            ui.add_enabled_ui(can_work && self.trusted, |ui| {
                                ui.checkbox(draft, "Enable backlight");
                            });
                        }
                        if ui.add_enabled(can_work && self.trusted && self.backlight_dirty(), egui::Button::new("APPLY BACKLIGHT")).clicked()
                            && let Some(value) = self.draft_backlight
                        {
                            self.start_apply(ui.ctx(), Setting::Backlight(value));
                        }
                        if ui.add_enabled(can_work && self.backlight_dirty(), egui::Button::new("REVERT")).clicked() {
                            self.draft_backlight = Some(observed.backlight_enabled());
                        }
                    });
                    ui.label(RichText::new("Enabling also turns off the keyboard's power-save option, which suppresses lighting. Other option bits are preserved.").small().color(MUTED));
                    ui.add_space(8.0);
                    ui.label(RichText::new("READ-ONLY / STORED VALUES").small().strong().color(MUTED));
                    egui::Grid::new("settings_read_only")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Keyboard options");
                            ui.monospace(format!("profile {} · flags {:02X} · Fn matrix {} · power save {}",
                                observed.option_profile(), observed.option_flags(),
                                if observed.fn_matrix_enabled() { "on" } else { "off" },
                                observed.power_save_value()));
                            ui.end_row();
                        });
                    ui.label(RichText::new("Sleep timer values are stored in seconds. Each timer accepts 0 to disable it; active normal timers use 1–60 minutes and deep sleep timers use 10–60 minutes.").small().color(MUTED));
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
        self.handle_close(ui.ctx());
    }
}

#[cfg(test)]
impl Default for SettingsEditor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(debounce: u8) -> Settings {
        let mut replies = [[0u8; 64]; 4];
        for (reply, opcode) in replies.iter_mut().zip([0x91, 0x97, 0x92, 0x86]) {
            reply[0] = opcode;
        }
        replies[0][2] = debounce;
        Settings::decode(&replies[0], &replies[1], &replies[2], &replies[3]).unwrap()
    }

    #[test]
    fn mismatched_success_retains_all_setting_drafts() {
        for setting in [
            Setting::Debounce(5),
            Setting::AutoOs(true),
            Setting::Sleep([60, 60, 600, 600]),
            Setting::Backlight(false),
        ] {
            let mut editor = SettingsEditor::new();
            let baseline = fixture(1);
            editor.accept_read(baseline.clone(), None);
            editor.draft_debounce = Some(5);
            editor.draft_auto = Some(true);
            editor.draft_sleep = Some([60, 60, 600, 600]);
            editor.draft_backlight = Some(false);
            editor.busy = true;
            editor
                .tx
                .send(WorkerResult::Applied(setting, Ok(baseline.clone())))
                .unwrap();
            editor.poll_worker();
            assert!(!editor.busy && !editor.trusted && editor.error);
            assert_eq!(editor.observed, Some(baseline));
            assert_eq!(editor.draft_debounce, Some(5));
            assert_eq!(editor.draft_auto, Some(true));
            assert_eq!(editor.draft_sleep, Some([60, 60, 600, 600]));
            assert_eq!(editor.draft_backlight, Some(false));
        }
    }

    #[test]
    fn matching_success_accepts_only_applied_setting_and_keeps_other_drafts() {
        let mut editor = SettingsEditor::new();
        editor.accept_read(fixture(1), None);
        editor.draft_debounce = Some(5);
        editor.draft_auto = Some(true);
        editor
            .tx
            .send(WorkerResult::Applied(Setting::Debounce(5), Ok(fixture(5))))
            .unwrap();
        editor.poll_worker();
        assert!(editor.trusted && !editor.error);
        assert!(!editor.debounce_dirty());
        assert!(editor.auto_dirty());
        assert_eq!(editor.draft_auto, Some(true));
    }

    fn close_frame(editor: &mut SettingsEditor) -> egui::FullOutput {
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
        let mut editor = SettingsEditor::new();
        let mut replies = [[0u8; 64]; 4];
        for (reply, opcode) in replies.iter_mut().zip([0x91, 0x97, 0x92, 0x86]) {
            reply[0] = opcode;
        }
        let settings =
            Settings::decode(&replies[0], &replies[1], &replies[2], &replies[3]).unwrap();
        editor.observed = Some(settings);
        editor.draft_debounce = Some(5);
        editor.draft_auto = Some(true);
        editor.draft_sleep = Some([60, 60, 600, 600]);
        editor.draft_backlight = Some(true);
        let drafts_before = (
            editor.draft_debounce,
            editor.draft_auto,
            editor.draft_sleep,
            editor.draft_backlight,
        );
        editor.busy = true;
        editor
            .tx
            .send(WorkerResult::Applied(
                Setting::Debounce(5),
                Err("restore failed".into()),
            ))
            .unwrap();
        let output = close_frame(&mut editor);
        editor.poll_worker();
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(commands.contains(&egui::ViewportCommand::CancelClose));
        assert!(!editor.busy);
        assert!(editor.error);
        assert_eq!(
            (
                editor.draft_debounce,
                editor.draft_auto,
                editor.draft_sleep,
                editor.draft_backlight
            ),
            drafts_before
        );
    }
}
