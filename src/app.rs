//! Native keymap workbench. Device I/O is confined to short-lived worker threads.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::{
    actions,
    backend::{self, nia87::Nia87Adapter},
    board,
    device::{self, Snapshot},
    keymap_ui::KeymapEditor,
    layout::{self, FN_PLACEHOLDER_USAGE, PhysicalKey},
    macro_ui::MacroEditor,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum WorkbenchTab {
    Keys,
    Macros,
    Lighting,
    Picture,
    Settings,
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
    ArchiveProgress(usize, usize),
    Archive(Result<crate::configuration::Configuration, String>),
    ReviewProgress(String),
    Review(Result<Box<StagedReview>, String>),
    ApplyReviewProgress(String),
    ApplyReview(Result<Snapshot, String>),
}

#[derive(Clone)]
struct StagedReview {
    current: crate::configuration::Configuration,
    target: crate::configuration::Configuration,
    summary: crate::configuration_plan::ChangeSummary,
    reverse: crate::configuration_plan::ChangeSummary,
}

struct Workbench {
    keys: Vec<PhysicalKey>,
    observed: Option<Snapshot>,
    base: Vec<[u8; 4]>,
    function: Vec<[u8; 4]>,
    selected: Option<u8>,
    layer: Layer,
    tab: WorkbenchTab,
    macro_editor: MacroEditor,
    lighting_editor: crate::lighting_ui::LightingEditor,
    picture_editor: crate::picture_ui::PictureEditor,
    settings_editor: crate::settings_ui::SettingsEditor,
    search: String,
    modifiers: [bool; 4],
    raw_editor: String,
    test_input: String,
    profile_path: String,
    archive_path: String,
    archive_progress: Option<(usize, usize)>,
    archive_summary: Option<String>,
    reviewed_archive: Option<StagedReview>,
    archive_apply_running: bool,
    status: String,
    error: bool,
    busy: bool,
    tx: Sender<WorkerResult>,
    rx: Receiver<WorkerResult>,
    backup_dir: PathBuf,
    keymap_editor: KeymapEditor,
    retry_schedule: crate::discovery::RetrySchedule,
}

impl Workbench {
    fn new(ctx: &egui::Context, data_dir: PathBuf) -> Self {
        let mut app = Self::without_read_at(data_dir);
        app.start_read(ctx);
        app
    }

    #[cfg(test)]
    fn without_read() -> Self {
        Self::without_read_at(std::env::temp_dir().join("byakko-test-data"))
    }

    fn without_read_at(data_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let backup_dir = data_dir.join("backups");
        Self {
            keys: layout::nia87_keys(),
            observed: None,
            base: Vec::new(),
            function: Vec::new(),
            selected: None,
            layer: Layer::Base,
            tab: WorkbenchTab::Keys,
            macro_editor: MacroEditor::new_with_backup_dir(backup_dir.clone()),
            lighting_editor: crate::lighting_ui::LightingEditor::new_with_backup_dir(
                backup_dir.clone(),
            ),
            picture_editor: crate::picture_ui::PictureEditor::new_with_backup_dir(
                backup_dir.clone(),
            ),
            settings_editor: crate::settings_ui::SettingsEditor::new_with_backup_dir(
                backup_dir.clone(),
            ),
            search: String::new(),
            modifiers: [false; 4],
            raw_editor: String::new(),
            test_input: String::new(),
            profile_path: data_dir
                .join("nia87-keymap.json")
                .to_string_lossy()
                .into_owned(),
            archive_path: data_dir
                .join("nia87-configuration.json")
                .to_string_lossy()
                .into_owned(),
            archive_progress: None,
            archive_summary: None,
            reviewed_archive: None,
            archive_apply_running: false,
            status: "Reading connected keyboard…".into(),
            error: false,
            busy: false,
            tx,
            rx,
            keymap_editor: KeymapEditor::new(Arc::new(Nia87Adapter), backup_dir.clone()),
            backup_dir,
            retry_schedule: crate::discovery::RetrySchedule::new(),
        }
    }

    fn start_read(&mut self, ctx: &egui::Context) {
        if self.device_busy() {
            return;
        }
        self.busy = true;
        self.retry_schedule.started();
        self.reviewed_archive = None;
        self.archive_apply_running = false;
        self.error = false;
        self.status = "Reading base and Fn keymaps…".into();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let snapshot = device::snapshot().map_err(|error| error.to_string())?;
                backend::nia87::from_snapshot(&snapshot)?;
                Ok(snapshot)
            }))
            .unwrap_or_else(|_| Err("Device read failed unexpectedly.".into()));
            let _ = tx.send(WorkerResult::Read(result));
            ctx.request_repaint();
        });
    }

    fn start_apply(&mut self, ctx: &egui::Context) {
        let Some(expected) = self.observed.clone() else {
            return;
        };
        if self.device_busy() || self.dirty_count() == 0 {
            return;
        }
        let base = self.base.clone();
        let function = self.function.clone();
        let backup_dir = self.backup_dir.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.busy = true;
        self.reviewed_archive = None;
        self.archive_apply_running = false;
        self.error = false;
        self.status = format!(
            "Backing up and applying {} key changes; allow about one second per change plus verification…",
            self.dirty_count()
        );
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                device::apply_keymaps(&expected, &base, &function, &backup_dir)
                    .map_err(|error| error.to_string())
            })).unwrap_or_else(|_| Err("Keymap apply panicked; device state and restoration are unverified. Inspect the backup before retrying.".into()));
            let _ = tx.send(WorkerResult::Applied(result));
            ctx.request_repaint();
        });
    }

    fn start_archive_capture(&mut self, ctx: &egui::Context) {
        if self.device_busy() {
            return;
        }
        let path = PathBuf::from(self.archive_path.trim());
        if path.as_os_str().is_empty() {
            self.status = "Choose a path for the configuration archive.".into();
            self.error = true;
            return;
        }
        if path.exists() {
            self.status = format!("Archive path already exists: {}", path.display());
            self.error = true;
            return;
        }
        self.busy = true;
        self.reviewed_archive = None;
        self.archive_apply_running = false;
        self.error = false;
        self.archive_progress = Some((0, 100));
        self.archive_summary = None;
        self.status =
            "Capturing complete device configuration; this may take a few minutes…".into();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result: Result<crate::configuration::Configuration, String> =
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    device::capture_configuration(|done, total| {
                        let _ = tx.send(WorkerResult::ArchiveProgress(done, total));
                        ctx.request_repaint();
                    })
                    .map_err(|error| error.to_string())
                    .and_then(|configuration| {
                        crate::configuration::save_new(&path, &configuration)
                            .map(|()| configuration)
                            .map_err(|error| error.to_string())
                    })
                })) {
                    Ok(result) => result,
                    Err(_) => Err("Configuration capture failed unexpectedly.".into()),
                };
            let _ = tx.send(WorkerResult::Archive(result));
            ctx.request_repaint();
        });
    }

    fn inspect_archive(&mut self) {
        self.reviewed_archive = None;
        self.archive_apply_running = false;
        match crate::configuration::load(std::path::Path::new(self.archive_path.trim())) {
            Ok(configuration) => {
                let macro_nonempty = configuration
                    .macros
                    .iter()
                    .filter(|slot| slot.iter().any(|byte| *byte != 0))
                    .count();
                let base_nonempty = configuration
                    .keymaps
                    .base
                    .iter()
                    .filter(|binding| **binding != [0; 4])
                    .count();
                let function_nonempty = configuration
                    .keymaps
                    .function
                    .iter()
                    .filter(|binding| **binding != [0; 4])
                    .count();
                self.archive_summary = Some(format!(
                    "50 macro slots ({macro_nonempty} non-empty) · keymaps: {base_nonempty} base / {function_nonempty} Fn bindings · picture: {} RGB slots · lighting: {} bytes · settings: 4 raw replies",
                    configuration.picture.len(),
                    configuration.lighting.raw().len(),
                ));
                self.status = "Archive inspected; no device or draft state changed.".into();
                self.error = false;
            }
            Err(error) => {
                self.archive_summary = None;
                self.status = error.to_string();
                self.error = true;
            }
        }
    }

    fn start_archive_review(&mut self, ctx: &egui::Context) {
        if self.device_busy() || self.dirty_count() != 0 || self.keymap_editor.dirty_count() != 0 {
            return;
        }
        let path = PathBuf::from(self.archive_path.trim());
        if path.as_os_str().is_empty() {
            self.status = "Choose an archive to review.".into();
            self.error = true;
            return;
        }
        self.busy = true;
        self.error = false;
        self.reviewed_archive = None;
        self.status = "Loading archive and capturing the complete current configuration…".into();
        let tx = self.tx.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let target = crate::configuration::load(&path).map_err(|e| e.to_string())?;
                let current = device::capture_configuration(|done, total| {
                    let _ = tx.send(WorkerResult::ReviewProgress(format!(
                        "Capturing current configuration: {done}/{total} macro reads"
                    )));
                    repaint.request_repaint();
                })
                .map_err(|e| e.to_string())?;
                let summary = crate::configuration_plan::plan(&current, &target)?;
                let reverse = crate::configuration_plan::plan(&target, &current)?;
                Ok(StagedReview {
                    current,
                    target,
                    summary,
                    reverse,
                })
            }));
            let result = match result {
                Ok(result) => result,
                Err(_) => Err("Archive review failed unexpectedly.".into()),
            };
            let _ = tx.send(WorkerResult::Review(result.map(Box::new)));
            repaint.request_repaint();
        });
    }

    fn start_archive_apply(&mut self, ctx: &egui::Context) {
        let Some(review) = self.reviewed_archive.clone() else {
            return;
        };
        if self.device_busy() || self.dirty_count() != 0 || self.keymap_editor.dirty_count() != 0 {
            return;
        }
        self.busy = true;
        self.archive_apply_running = true;
        self.error = false;
        self.status = "Applying reviewed archive; backup, verification, and recovery may take several minutes…".into();
        let backup_dir = self.backup_dir.clone();
        let tx = self.tx.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                device::apply_configuration(
                    &review.current,
                    &review.target,
                    &backup_dir,
                    |message| {
                        let _ = tx.send(WorkerResult::ApplyReviewProgress(message.to_owned()));
                        repaint.request_repaint();
                    },
                )
                .map_err(|e| e.to_string())
            }));
            let result = match result {
                Ok(result) => result,
                Err(_) => Err("Archive apply panicked; restoration is unverified. Inspect the backups directory before retrying.".into()),
            };
            let _ = tx.send(WorkerResult::ApplyReview(
                result.map(|configuration| configuration.keymaps),
            ));
            repaint.request_repaint();
        });
    }

    fn handle_archive_close(&mut self, ctx: &egui::Context) {
        if self.busy && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.archive_apply_running && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.status = "Archive apply is still running; close is held until verification or recovery finishes.".into();
        }
    }

    fn poll_worker(&mut self) {
        if let Some(state) = self.keymap_editor.poll() {
            match backend::nia87::to_snapshot(&state) {
                Ok(snapshot) => {
                    self.load(snapshot);
                    self.status = "Keymap changes applied and verified.".into();
                    self.error = false;
                }
                Err(error) => {
                    self.status = format!("Applied keymap could not be converted: {error}");
                    self.error = true;
                }
            }
        }
        while let Ok(message) = self.rx.try_recv() {
            match message {
                WorkerResult::Read(Ok(snapshot)) => {
                    self.busy = false;
                    match backend::nia87::from_snapshot(&snapshot) {
                        Err(error) => {
                            self.retry_schedule.failed(Instant::now());
                            self.status = format!(
                                "Device read rejected: {error}. Retrying read automatically."
                            );
                            self.error = true;
                        }
                        Ok(_) => {
                            self.retry_schedule.succeeded();
                            if self.dirty_count() > 0 || self.keymap_editor.dirty_count() > 0 {
                                if self.observed.as_ref() == Some(&snapshot) {
                                    self.status = "Device read matched the loaded baseline. Staged key changes were retained.".into();
                                    self.error = false;
                                } else {
                                    self.status = "Device keymaps changed since the draft was staged. Draft and loaded baseline were retained. Export the draft if needed, revert it, then read again.".into();
                                    self.error = true;
                                }
                            } else {
                                self.load(snapshot);
                                self.status =
                                    "Device read complete. Select a key to inspect its binding."
                                        .into();
                                self.error = false;
                            }
                        }
                    }
                }
                WorkerResult::Applied(Ok(snapshot)) => {
                    self.busy = false;
                    self.load(snapshot);
                    self.status = format!(
                        "Changes applied and verified. Backup directory: {}",
                        self.backup_dir.display()
                    );
                    self.error = false;
                }
                WorkerResult::Read(Err(error)) => {
                    self.busy = false;
                    self.retry_schedule.failed(Instant::now());
                    self.status = format!(
                        "{error} Waiting for a supported keyboard; retrying read automatically."
                    );
                    self.error = true;
                }
                WorkerResult::Applied(Err(error)) => {
                    self.busy = false;
                    self.status = error;
                    self.error = true;
                }
                WorkerResult::ArchiveProgress(done, total) => {
                    self.archive_progress = Some((done, total));
                }
                WorkerResult::Archive(Ok(_configuration)) => {
                    self.busy = false;
                    self.archive_progress = Some((100, 100));
                    self.status = format!(
                        "Configuration archive saved to {}. Captured device state only; drafts were not included.",
                        self.archive_path
                    );
                    self.error = false;
                }
                WorkerResult::Archive(Err(error)) => {
                    self.busy = false;
                    self.archive_progress = None;
                    self.status = error;
                    self.error = true;
                }
                WorkerResult::ReviewProgress(message)
                | WorkerResult::ApplyReviewProgress(message) => {
                    self.status = message;
                }
                WorkerResult::Review(Ok(review)) => {
                    self.busy = false;
                    self.archive_summary = Some(format!(
                        "Review ready: {} key bindings · {} macro slots · {} picture keys · lighting {} · {} settings",
                        review.summary.key_bindings,
                        review.summary.macro_slots.len(),
                        review.summary.picture_keys,
                        if review.summary.lighting {
                            "changed"
                        } else {
                            "unchanged"
                        },
                        review.summary.settings.len()
                    ));
                    self.reviewed_archive = Some(*review);
                    self.status =
                        "Archive review complete. Apply only after checking the planned changes."
                            .into();
                    self.error = false;
                }
                WorkerResult::Review(Err(error)) => {
                    self.busy = false;
                    self.reviewed_archive = None;
                    self.status = error;
                    self.error = true;
                }
                WorkerResult::ApplyReview(Ok(snapshot)) => {
                    self.busy = false;
                    self.archive_apply_running = false;
                    self.reviewed_archive = None;
                    self.load(snapshot);
                    self.status = format!(
                        "Reviewed archive applied and verified. Other editor panels must reload. Backups: {}",
                        self.backup_dir.display()
                    );
                    self.error = false;
                }
                WorkerResult::ApplyReview(Err(error)) => {
                    self.busy = false;
                    self.archive_apply_running = false;
                    self.reviewed_archive = None;
                    self.status = error;
                    self.error = true;
                }
            }
        }
    }

    fn maybe_retry_read(&mut self, ctx: &egui::Context) {
        if !self.device_busy() && self.retry_schedule.due(Instant::now()) {
            self.status = "Retrying device read…".into();
            self.start_read(ctx);
        }
        if !self.device_busy()
            && let Some(deadline) = self.retry_schedule.deadline()
        {
            let now = Instant::now();
            let delay = deadline
                .saturating_duration_since(now)
                .max(Duration::from_secs(1));
            ctx.request_repaint_after(delay);
        }
    }

    fn load(&mut self, snapshot: Snapshot) {
        self.base = snapshot.base.clone();
        self.function = snapshot.function.clone();
        self.observed = Some(snapshot);
        if let Some(snapshot) = &self.observed {
            match backend::nia87::from_snapshot(snapshot)
                .and_then(|state| self.keymap_editor.load(state.clone()).map(|()| state))
            {
                Ok(_) => {}
                Err(error) => {
                    self.status = format!("Unable to load generic keymap editor: {error}");
                    self.error = true;
                }
            }
        }
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
        if usage == FN_PLACEHOLDER_USAGE {
            return "Fn (protected)".into();
        }
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
            Some([9, mode @ 0..=2, index @ 0..=49, 0]) => {
                let play = match mode {
                    0 => "count",
                    1 => "toggle",
                    _ => "hold",
                };
                format!("Macro {index} · {play}")
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

    fn device_busy(&self) -> bool {
        self.busy
            || self.keymap_editor.busy()
            || self.macro_editor.busy()
            || self.lighting_editor.busy()
            || self.picture_editor.busy()
            || self.settings_editor.busy()
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
        if self.device_busy() {
            return;
        }
        self.draft_mut(self.layer)[slot] = bytes;
        self.sync_editor();
        if let Some(observed) = &self.observed {
            let mut snapshot = observed.clone();
            snapshot.base = self.base.clone();
            snapshot.function = self.function.clone();
            match backend::nia87::from_snapshot(&snapshot)
                .and_then(|state| self.keymap_editor.stage_state(state))
            {
                Ok(()) => {}
                Err(error) => {
                    self.status = format!("Unable to synchronize keymap draft: {error}");
                    self.error = true;
                    return;
                }
            }
        }
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
        if self.device_busy() {
            return;
        }
        if let Some(snapshot) = &self.observed {
            self.base.clone_from(&snapshot.base);
            self.function.clone_from(&snapshot.function);
            let mut restored = snapshot.clone();
            restored.base = self.base.clone();
            restored.function = self.function.clone();
            match backend::nia87::from_snapshot(&restored)
                .and_then(|state| self.keymap_editor.stage_state(state))
            {
                Ok(()) => {}
                Err(error) => {
                    self.status = format!("Unable to synchronize reverted keymap: {error}");
                    self.error = true;
                    return;
                }
            }
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
                egui::RichText::new("NIA87 / CONFIGURATION WORKBENCH")
                    .size(13.0)
                    .strong()
                    .color(MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_read = !self.device_busy();
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

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("WORKSPACE")
                    .small()
                    .strong()
                    .color(MUTED),
            );
            let can_switch = !self.device_busy() && self.keymap_editor.dirty_count() == 0;
            for (tab, label) in [
                (WorkbenchTab::Keys, "KEYS  Ctrl+1"),
                (WorkbenchTab::Macros, "MACROS  Ctrl+2"),
                (WorkbenchTab::Lighting, "LIGHTING  Ctrl+3"),
                (WorkbenchTab::Picture, "PER-KEY COLOR  Ctrl+4"),
                (WorkbenchTab::Settings, "SETTINGS  Ctrl+5"),
            ] {
                ui.add_enabled_ui(can_switch, |ui| {
                    if ui.selectable_label(self.tab == tab, label).clicked() {
                        self.tab = tab;
                    }
                });
            }
            if self.device_busy() {
                ui.spinner();
                ui.label("Device operation in progress");
            }
        });
    }

    fn layer_bar(&mut self, ui: &mut egui::Ui) {
        if self.tab == WorkbenchTab::Keys {
            return;
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("LAYER").small().strong().color(MUTED));
            for layer in [Layer::Base, Layer::Function] {
                ui.add_enabled_ui(!self.device_busy(), |ui| {
                    if ui
                        .selectable_label(self.layer == layer, layer.name())
                        .clicked()
                    {
                        self.layer = layer;
                        self.sync_editor();
                    }
                });
            }
            ui.separator();
            if self.layer == Layer::Function {
                ui.label(egui::RichText::new("Fn layer · staged key bindings").color(MUTED));
            }
            ui.label(
                egui::RichText::new(format!("{} staged slot(s)", self.dirty_count())).color(
                    if self.dirty_count() > 0 {
                        ACCENT
                    } else {
                        MUTED
                    },
                ),
            );
        });
    }

    fn keyboard(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("PHYSICAL LAYOUT")
                .small()
                .strong()
                .color(MUTED),
        );
        let unit = (ui.available_width() / 18.5).clamp(24.0, 50.0);
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
        if !self.device_busy()
            && let Some(usage) = clicked
        {
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
                    ui.add_enabled_ui(
                        !self.device_busy() && (self.modifiers[index] || count < 2),
                        |ui| {
                            ui.checkbox(&mut self.modifiers[index], *label);
                        },
                    );
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
                    ui.add_enabled_ui(!self.device_busy() && self.observed.is_some(), |ui| {
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
            ui.add_enabled_ui(!self.device_busy() && self.observed.is_some(), |ui| {
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
                        !self.device_busy() && parsed.is_some(),
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
            let can_apply =
                self.observed.is_some() && !self.device_busy() && self.dirty_count() > 0;
            if ui
                .add_enabled(can_apply, egui::Button::new("APPLY TO KEYBOARD  Ctrl+S"))
                .clicked()
            {
                self.start_apply(ui.ctx());
            }
            if ui
                .add_enabled(
                    !self.device_busy() && self.dirty_count() > 0,
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

    fn keys_page(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Local keymap file", |ui| {
            ui.label("Saves both keymaps, including macro slot references. Macro event data and lighting are separate.");
            ui.horizontal(|ui| {
                ui.label("Path");
                ui.text_edit_singleline(&mut self.profile_path);
                if ui.add_enabled(!self.device_busy() && self.observed.is_some(), egui::Button::new("SAVE NEW FILE")).clicked() {
                    let result = self.keymap_editor.observed()
                        .ok_or_else(|| "Read the keyboard before exporting a keymap".to_owned())
                        .and_then(|observed| backend::nia87::draft_snapshot(observed, &self.keymap_editor.changes()))
                        .map_err(|error| error.to_string())
                        .and_then(|snapshot| crate::profiles::save_new(std::path::Path::new(&self.profile_path), &snapshot).map_err(|e| e.to_string()));
                    match result {
                        Ok(()) => { self.status = "Saved keymap draft to a new file.".into(); self.error = false; }
                        Err(e) => { self.status = e; self.error = true; }
                    }
                }
                if ui.add_enabled(!self.device_busy() && self.observed.is_some() && self.dirty_count() == 0 && self.keymap_editor.dirty_count() == 0, egui::Button::new("IMPORT TO DRAFT")).clicked() {
                    let current = self.observed.as_ref().expect("enabled when loaded");
                    match crate::profiles::load_for_device(std::path::Path::new(&self.profile_path), current) {
                        Ok(imported) => {
                            match backend::nia87::from_snapshot(&imported).and_then(|state| self.keymap_editor.stage_state(state)) {
                                Ok(()) => { self.status = "Imported keymap to local draft. Review changes before Apply.".into(); self.error = false; }
                                Err(e) => { self.status = e; self.error = true; }
                            }
                        }
                        Err(e) => { self.status = e.to_string(); self.error = true; }
                    }
                }
            });
        });
        ui.collapsing("Configuration archive", |ui| {
            ui.label("Captures saved device state: both keymaps, all 50 macro slots, current picture, lighting, and settings.");
            ui.label(egui::RichText::new("Unsaved drafts are not included. Review captures a fresh current state before any restore.").small().color(MUTED));
            ui.horizontal(|ui| {
                ui.label("Path");
                ui.add_enabled_ui(!self.device_busy(), |ui| {
                    if ui.text_edit_singleline(&mut self.archive_path).changed() {
                        self.reviewed_archive = None;
                    }
                });
                if ui.add_enabled(!self.device_busy(), egui::Button::new("CAPTURE TO NEW FILE")).clicked() {
                    self.start_archive_capture(ui.ctx());
                }
                if ui.add_enabled(!self.device_busy(), egui::Button::new("INSPECT FILE")).clicked() {
                    self.inspect_archive();
                }
                let can_review = !self.device_busy() && self.dirty_count() == 0 && self.keymap_editor.dirty_count() == 0;
                if ui.add_enabled(can_review, egui::Button::new("REVIEW RESTORE")).clicked() {
                    self.start_archive_review(ui.ctx());
                }
            });
            if let Some((done, total)) = self.archive_progress {
                ui.horizontal(|ui| {
                    ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).desired_width(180.0));
                    ui.label(format!("{done}/{total} macro reads"));
                });
            }
            if let Some(summary) = &self.archive_summary {
                ui.label(egui::RichText::new(summary).color(INK));
            }
            if let Some(review) = &self.reviewed_archive {
                ui.separator();
                ui.label(egui::RichText::new("REVIEWED RESTORE").small().strong().color(MUTED));
                ui.label(format!(
                    "Forward: {} key bindings, {} macro slots, {} picture keys, lighting {}, {} settings",
                    review.summary.key_bindings, review.summary.macro_slots.len(), review.summary.picture_keys,
                    if review.summary.lighting { "changed" } else { "unchanged" }, review.summary.settings.len()
                ));
                ui.label(format!(
                    "Recovery plan: {} key bindings, {} macro slots, {} picture keys, lighting {}, {} settings",
                    review.reverse.key_bindings, review.reverse.macro_slots.len(), review.reverse.picture_keys,
                    if review.reverse.lighting { "changed" } else { "unchanged" }, review.reverse.settings.len()
                ));
                let mut details = Vec::new();
                for (layer, before, after) in [
                    ("BASE", &review.current.keymaps.base, &review.target.keymaps.base),
                    ("FN", &review.current.keymaps.function, &review.target.keymaps.function),
                ] {
                    for (slot, (old, new)) in before.iter().zip(after).enumerate() {
                        if old != new { details.push(format!("{layer} slot {slot}: {} → {}", format_bytes(*old), format_bytes(*new))); }
                    }
                }
                if !review.summary.macro_slots.is_empty() {
                    details.push(format!("Macro slots changed: {}", review.summary.macro_slots.iter().map(|slot| slot.to_string()).collect::<Vec<_>>().join(", ")));
                }
                if !review.summary.settings.is_empty() {
                    details.push(format!("Settings: {:?}", review.summary.settings));
                }
                if review.summary.lighting {
                    details.push(format!("Lighting bytes: {:?} → {:?}", &review.current.lighting.raw()[1..8], &review.target.lighting.raw()[1..8]));
                }
                for (slot, (old, new)) in review.current.picture.iter().zip(&review.target.picture).enumerate() {
                    if old != new { details.push(format!("Picture slot {slot}: {old:?} → {new:?}")); }
                }
                if details.is_empty() { details.push("No writable changes; archive already matches the captured device.".into()); }
                egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                    for detail in details { ui.monospace(detail); }
                });
                let can_apply = !self.device_busy() && self.dirty_count() == 0 && self.keymap_editor.dirty_count() == 0;
                if ui.add_enabled(can_apply, egui::Button::new("APPLY REVIEWED ARCHIVE")).clicked() {
                    self.start_archive_apply(ui.ctx());
                }
            }
        });
        self.keymap_editor.ui(
            ui,
            self.busy
                || self.macro_editor.busy()
                || self.lighting_editor.busy()
                || self.picture_editor.busy()
                || self.settings_editor.busy(),
        );
        // The shared editor is authoritative on this page. Keep the optional
        // native inspector on the same draft before it can stage another edit.
        if let Some(observed) = self.keymap_editor.observed() {
            match backend::nia87::draft_snapshot(observed, &self.keymap_editor.changes()) {
                Ok(snapshot) => {
                    self.base = snapshot.base;
                    self.function = snapshot.function;
                    self.sync_editor();
                }
                Err(error) => {
                    self.status = format!("Unable to synchronize native inspector: {error}");
                    self.error = true;
                }
            }
        }
        ui.collapsing("Nia87 keymap details", |ui| {
            ui.label(
                "Legacy raw binding inspector for compatibility and advanced byte inspection.",
            );
            ui.horizontal(|ui| {
                self.keyboard(ui);
                self.inspector(ui);
            });
        });
        ui.collapsing("Keyboard input test", |ui| {
            ui.label("Click here and type to check host input after applying a binding.");
            ui.add(
                egui::TextEdit::multiline(&mut self.test_input)
                    .hint_text("Type here…")
                    .desired_rows(2),
            );
        });
    }

    fn selected_key_context(&self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("BINDING TARGET")
                .small()
                .strong()
                .color(MUTED),
        );
        ui.separator();
        let Some(usage) = self.selected else {
            ui.label("Choose a physical key at left before binding a saved macro.");
            return;
        };
        let key = self
            .keys
            .iter()
            .find(|key| key.usage == usage)
            .map_or("Key", |key| key.label);
        ui.label(egui::RichText::new(key).size(20.0).strong().color(INK));
        if let Some(slot) = self.slot(usage) {
            ui.label(format!("{} layer · matrix slot {slot}", self.layer.name()));
            ui.label(format!(
                "Current binding: {}",
                self.current_label(self.layer, usage)
            ));
            if let Some(bytes) = self.binding(self.layer, usage) {
                ui.monospace(format_bytes(bytes));
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(
                    "Save a macro below, then bind it here. The keymap change stays staged until Apply to keyboard.",
                )
                .small()
                .color(MUTED),
            );
        } else {
            ui.label("This key has no writable matrix slot.");
        }
    }

    fn macros_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.columns(2, |columns| {
                self.keyboard(&mut columns[0]);
                self.selected_key_context(&mut columns[1]);
            });
            ui.add_space(12.0);
            let blocked = self.busy;
            if let Some(binding) = self.macro_editor.ui(ui, blocked) {
                if self.selected.and_then(|usage| self.slot(usage)).is_some() {
                    self.set_binding(binding);
                } else {
                    self.error = true;
                    self.status =
                        "Choose a writable physical key above before binding the macro.".into();
                }
            }
            ui.add_space(12.0);
            self.changes(ui);
            ui.add_space(10.0);
            self.footer(ui);
        });
    }
}

impl eframe::App for Workbench {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Cancel a close while a stream is still owned, before processing its
        // completion. A restoration error in this frame must remain visible.
        self.handle_archive_close(ui.ctx());
        self.keymap_editor.handle_close(ui.ctx());
        self.macro_editor.handle_close(ui.ctx());
        self.picture_editor.handle_close(ui.ctx());
        self.settings_editor.handle_close(ui.ctx());
        self.lighting_editor.handle_close(ui.ctx());
        self.poll_worker();
        self.maybe_retry_read(ui.ctx());
        if !self.device_busy() && self.keymap_editor.dirty_count() == 0 {
            for (key, tab) in [
                (egui::Key::Num1, WorkbenchTab::Keys),
                (egui::Key::Num2, WorkbenchTab::Macros),
                (egui::Key::Num3, WorkbenchTab::Lighting),
                (egui::Key::Num4, WorkbenchTab::Picture),
                (egui::Key::Num5, WorkbenchTab::Settings),
            ] {
                if ctrl_shortcut(ui, key) {
                    self.tab = tab;
                }
            }
        }
        let ctrl_s = ctrl_shortcut(ui, egui::Key::S);
        let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
        if ctrl_s && self.tab == WorkbenchTab::Keys && !self.device_busy() {
            self.keymap_editor.start_apply(ui.ctx());
        } else if ctrl_s && self.tab == WorkbenchTab::Macros {
            self.start_apply(ui.ctx());
        }
        if escape && self.tab == WorkbenchTab::Keys && !self.device_busy() {
            self.select(None);
        }
        ui.ctx().set_visuals(egui::Visuals::light());
        ui.painter().rect_filled(ui.max_rect(), 0.0, PAPER);
        egui::Frame::NONE
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                self.header(ui);
                self.tab_bar(ui);
                self.layer_bar(ui);
                ui.add_space(12.0);
                match self.tab {
                    WorkbenchTab::Keys => {
                        egui::ScrollArea::vertical()
                            .id_salt("keys_page_scroll")
                            .show(ui, |ui| self.keys_page(ui));
                    }
                    WorkbenchTab::Macros => self.macros_page(ui),
                    WorkbenchTab::Lighting => {
                        let blocked = self.busy || self.macro_editor.busy();
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            self.lighting_editor.ui(ui, blocked);
                        });
                    }
                    WorkbenchTab::Picture => {
                        let blocked =
                            self.busy || self.macro_editor.busy() || self.lighting_editor.busy();
                        egui::ScrollArea::vertical()
                            .show(ui, |ui| self.picture_editor.ui(ui, blocked));
                    }
                    WorkbenchTab::Settings => {
                        let blocked = self.busy
                            || self.macro_editor.busy()
                            || self.lighting_editor.busy()
                            || self.picture_editor.busy();
                        egui::ScrollArea::vertical()
                            .show(ui, |ui| self.settings_editor.ui(ui, blocked));
                    }
                }
            });
        if self.device_busy() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        self.lighting_editor.handle_close(ui.ctx());
    }
}

fn format_bytes(bytes: [u8; 4]) -> String {
    format!(
        "{:02X} {:02X} {:02X} {:02X}",
        bytes[0], bytes[1], bytes[2], bytes[3]
    )
}

fn ctrl_shortcut(ui: &egui::Ui, wanted: egui::Key) -> bool {
    ui.input(|input| {
        input.events.iter().any(|event| {
            matches!(event,
                egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. }
                    if *key == wanted && modifiers.ctrl
            )
        })
    })
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

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn explicit_data_dir_controls_gui_default_paths() {
        let root = std::env::temp_dir().join("byakko-explicit-path-test");
        let app = Workbench::without_read_at(root.clone());
        assert_eq!(app.backup_dir, root.join("backups"));
        assert_eq!(
            PathBuf::from(app.profile_path),
            root.join("nia87-keymap.json")
        );
        assert_eq!(
            PathBuf::from(app.archive_path),
            root.join("nia87-configuration.json")
        );
    }

    fn snapshot(fill: u8) -> Snapshot {
        Snapshot {
            format_version: 1,
            firmware: 0x0100,
            profile: 0,
            base: vec![[fill, 0, 0, 0]; 128],
            function: vec![[fill, 0, 0, 0]; 128],
        }
    }

    fn configuration(fill: u8) -> crate::configuration::Configuration {
        let mut settings = [[0u8; 64]; 4];
        for (reply, opcode) in settings.iter_mut().zip([0x91, 0x97, 0x92, 0x86]) {
            reply[0] = opcode;
        }
        crate::configuration::Configuration {
            keymaps: snapshot(fill),
            macros: vec![vec![0; 256]; 50],
            lighting: crate::lighting::Lighting::decode(&[0x87; 64]).unwrap(),
            picture: vec![[0; 3]; 128],
            settings: crate::settings::Settings::decode(
                &settings[0],
                &settings[1],
                &settings[2],
                &settings[3],
            )
            .unwrap(),
        }
    }

    fn staged_review() -> StagedReview {
        let current = configuration(0);
        let target = configuration(0);
        let summary = crate::configuration_plan::plan(&current, &target).unwrap();
        let reverse = crate::configuration_plan::plan(&target, &current).unwrap();
        StagedReview {
            current,
            target,
            summary,
            reverse,
        }
    }

    fn close_frame(app: &mut Workbench) -> egui::FullOutput {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let mut output = ctx.run_ui(input, |ui| app.handle_archive_close(ui.ctx()));
        output.textures_delta.clear();
        output
    }

    #[test]
    fn archive_apply_close_is_cancelled_even_when_error_completes_same_frame() {
        let mut app = Workbench::without_read();
        app.busy = true;
        app.archive_apply_running = true;
        app.tx
            .send(WorkerResult::ApplyReview(Err("restore failed".into())))
            .unwrap();
        let output = close_frame(&mut app);
        app.poll_worker();
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(commands.contains(&egui::ViewportCommand::CancelClose));
        assert!(!app.archive_apply_running);
        assert!(!app.busy);
        assert!(app.error);
        assert_eq!(app.status, "restore failed");
    }

    #[test]
    fn archive_apply_success_loads_keymaps_and_clears_staging() {
        let mut app = Workbench::without_read();
        app.busy = true;
        app.archive_apply_running = true;
        app.reviewed_archive = Some(staged_review());
        app.tx
            .send(WorkerResult::ApplyReview(Ok(snapshot(7))))
            .unwrap();
        app.poll_worker();
        assert!(!app.busy);
        assert!(!app.archive_apply_running);
        assert!(app.reviewed_archive.is_none());
        assert_eq!(app.base, vec![[7, 0, 0, 0]; 128]);
    }

    #[test]
    fn archive_apply_progress_does_not_clear_busy_or_stage() {
        let mut app = Workbench::without_read();
        app.busy = true;
        app.archive_apply_running = true;
        app.reviewed_archive = Some(staged_review());
        app.tx
            .send(WorkerResult::ApplyReviewProgress("writing".into()))
            .unwrap();
        app.poll_worker();
        assert!(app.busy);
        assert!(app.archive_apply_running);
        assert!(app.reviewed_archive.is_some());
        assert_eq!(app.status, "writing");
    }

    #[test]
    fn legacy_macro_binding_is_bridged_into_generic_draft() {
        let mut app = Workbench::without_read();
        app.load(snapshot(0));
        app.selected = Some(4);
        app.set_binding([9, 0, 5, 0]);
        assert_eq!(app.keymap_editor.dirty_count(), 1);
        assert_eq!(app.keymap_editor.changes()[0].layer, "base");
    }

    #[test]
    fn matching_read_preserves_staged_key_changes() {
        let mut app = Workbench::without_read();
        let baseline = snapshot(0);
        app.load(baseline.clone());
        app.selected = Some(4);
        app.set_binding([0, 0, 5, 0]);
        let staged = app.base.clone();
        let generic_changes = app.keymap_editor.changes();
        app.tx
            .send(WorkerResult::Read(Ok(baseline.clone())))
            .unwrap();
        app.poll_worker();
        assert_eq!(app.base, staged);
        assert_eq!(app.observed, Some(baseline));
        assert_eq!(app.keymap_editor.changes(), generic_changes);
        assert!(!app.error);
    }

    #[test]
    fn differing_read_retains_draft_and_baseline_as_conflict() {
        let mut app = Workbench::without_read();
        let baseline = snapshot(0);
        app.load(baseline.clone());
        app.selected = Some(4);
        app.set_binding([0, 0, 5, 0]);
        let staged = app.base.clone();
        let generic_changes = app.keymap_editor.changes();
        app.tx.send(WorkerResult::Read(Ok(snapshot(1)))).unwrap();
        app.poll_worker();
        assert_eq!(app.base, staged);
        assert_eq!(app.observed, Some(baseline));
        assert_eq!(app.keymap_editor.changes(), generic_changes);
        assert!(app.error && app.status.contains("Export the draft if needed"));
        assert!(app.retry_schedule.deadline().is_none());
    }

    #[test]
    fn failed_read_after_load_retries_without_losing_draft() {
        let mut app = Workbench::without_read();
        let baseline = snapshot(0);
        app.load(baseline.clone());
        app.selected = Some(4);
        app.set_binding([0, 0, 5, 0]);
        let staged = app.base.clone();
        app.tx
            .send(WorkerResult::Read(Err("disconnected".into())))
            .unwrap();
        app.poll_worker();
        assert_eq!(app.base, staged);
        assert_eq!(app.observed, Some(baseline));
        assert!(app.retry_schedule.deadline().is_some());
    }

    #[test]
    fn differing_read_keeps_generic_only_draft() {
        let mut app = Workbench::without_read();
        let baseline = snapshot(0);
        app.load(baseline.clone());
        let mut desired = baseline.clone();
        desired.base[9] = [0, 0, 4, 0];
        app.keymap_editor
            .stage_state(backend::nia87::from_snapshot(&desired).unwrap())
            .unwrap();
        assert_eq!(app.dirty_count(), 0);
        let staged = app.keymap_editor.changes();
        app.tx.send(WorkerResult::Read(Ok(snapshot(1)))).unwrap();
        app.poll_worker();
        assert_eq!(app.observed, Some(baseline));
        assert_eq!(app.keymap_editor.changes(), staged);
        assert!(app.error && app.status.contains("revert it, then read again"));
    }

    #[test]
    fn invalid_read_cannot_replace_loaded_state_and_apply_error_does_not_retry() {
        let mut app = Workbench::without_read();
        let baseline = snapshot(0);
        app.load(baseline.clone());
        let mut invalid = snapshot(1);
        invalid.format_version = 2;
        app.tx.send(WorkerResult::Read(Ok(invalid))).unwrap();
        app.poll_worker();
        assert_eq!(app.observed, Some(baseline.clone()));
        assert!(app.retry_schedule.deadline().is_some());
        app.retry_schedule.succeeded();
        app.tx
            .send(WorkerResult::Applied(Err("write failed".into())))
            .unwrap();
        app.poll_worker();
        assert_eq!(app.observed, Some(baseline));
        assert!(app.retry_schedule.deadline().is_none());
        assert_eq!(app.status, "write failed");
    }

    #[test]
    fn generic_import_bridge_preserves_raw_snapshot_for_export() {
        let mut app = Workbench::without_read();
        let expected = snapshot(0);
        app.load(expected.clone());
        let mut imported = expected.clone();
        imported.base[4] = [0, 0, 5, 0];
        let state = backend::nia87::from_snapshot(&imported).unwrap();
        app.keymap_editor.stage_state(state).unwrap();
        let exported = backend::nia87::draft_snapshot(
            app.keymap_editor.observed().unwrap(),
            &app.keymap_editor.changes(),
        )
        .unwrap();
        assert_eq!(exported.base[4], imported.base[4]);
        assert_eq!(exported.function, imported.function);
    }

    #[test]
    fn native_inspector_preserves_existing_generic_edits_and_reverts() {
        let mut app = Workbench::without_read();
        let original = snapshot(0);
        app.load(original.clone());
        let mut imported = original;
        imported.base[4] = [0, 0, 5, 0];
        app.keymap_editor
            .stage_state(backend::nia87::from_snapshot(&imported).unwrap())
            .unwrap();
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.keys_page(ui));
        output.textures_delta.clear();
        assert_eq!(app.base[4], imported.base[4]);
        app.selected = Some(4);
        app.set_binding([9, 0, 5, 0]);
        assert_eq!(app.keymap_editor.dirty_count(), 2);
        app.revert();
        assert_eq!(app.dirty_count(), 0);
        assert_eq!(app.keymap_editor.dirty_count(), 0);
    }

    #[test]
    fn initial_read_failure_schedules_bounded_retry() {
        let mut app = Workbench::without_read();
        app.tx
            .send(WorkerResult::Read(Err("keyboard unavailable".into())))
            .unwrap();
        app.poll_worker();
        assert!(app.retry_schedule.deadline().is_some());
    }

    #[test]
    fn legacy_keymap_write_close_is_held_until_error_is_visible() {
        let mut app = Workbench::without_read();
        app.load(snapshot(0));
        app.selected = Some(4);
        app.set_binding([9, 0, 5, 0]);
        app.busy = true;
        app.tx
            .send(WorkerResult::Applied(Err("readback failed".into())))
            .unwrap();
        let output = close_frame(&mut app);
        app.poll_worker();
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::CancelClose)
        );
        assert!(app.error && app.dirty_count() > 0);
        assert!(!app.busy);
        assert_eq!(app.status, "readback failed");
    }

    #[test]
    fn successful_initial_read_clears_retry_schedule() {
        let mut app = Workbench::without_read();
        app.retry_schedule.failed(Instant::now());
        app.tx.send(WorkerResult::Read(Ok(snapshot(0)))).unwrap();
        app.poll_worker();
        assert!(app.observed.is_some());
        assert!(app.retry_schedule.deadline().is_none());
    }

    #[test]
    fn observed_read_error_schedules_retry_but_apply_error_does_not() {
        let mut app = Workbench::without_read();
        app.load(snapshot(0));
        app.tx
            .send(WorkerResult::Read(Err("transient read error".into())))
            .unwrap();
        app.poll_worker();
        assert!(app.retry_schedule.deadline().is_some());
        app.retry_schedule.succeeded();
        app.tx
            .send(WorkerResult::Applied(Err("write failed".into())))
            .unwrap();
        app.poll_worker();
        assert!(app.retry_schedule.deadline().is_none());
    }
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
        Box::new(|cc| {
            let data_dir = crate::storage::prepare_user_data_dir()?;
            Ok(Box::new(Workbench::new(&cc.egui_ctx, data_dir)))
        }),
    )
}
