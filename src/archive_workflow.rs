//! Configuration archive workflow and its device worker boundary.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
};

use eframe::egui;

use crate::{
    configuration::{self, Configuration},
    configuration_plan::{self, ChangeSummary},
    device::{self, Snapshot},
};

pub(crate) struct Review {
    pub current: Configuration,
    pub target: Configuration,
    pub summary: ChangeSummary,
    pub reverse: ChangeSummary,
}

enum WorkerMessage {
    CaptureProgress(usize, usize),
    Capture(Result<(), String>),
    ReviewProgress(String),
    Review(Result<Arc<Review>, String>),
    ApplyProgress(String),
    Apply(Result<Snapshot, String>),
}

enum State {
    Idle,
    Capturing { done: usize, total: usize },
    Captured,
    Reviewing,
    Ready(Arc<Review>),
    Applying(Arc<Review>),
}

pub(crate) struct ArchiveWorkflow {
    path: String,
    backup_dir: PathBuf,
    state: State,
    summary: Option<String>,
    status: String,
    error: bool,
    tx: Sender<WorkerMessage>,
    rx: Receiver<WorkerMessage>,
}

#[derive(Default)]
pub(crate) struct PollResult {
    pub changed: bool,
    pub applied: Option<Snapshot>,
}

impl ArchiveWorkflow {
    pub(crate) fn new(path: String, backup_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            path,
            backup_dir,
            state: State::Idle,
            summary: None,
            status: String::new(),
            error: false,
            tx,
            rx,
        }
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }
    #[cfg(test)]
    pub(crate) fn backup_dir(&self) -> &std::path::Path {
        &self.backup_dir
    }
    pub(crate) fn set_path(&mut self, path: String) {
        if !self.busy() && self.path != path {
            self.path = path;
            self.invalidate_review();
        }
    }
    pub(crate) fn invalidate_review(&mut self) {
        if matches!(self.state, State::Ready(_)) {
            self.state = State::Idle;
        }
    }
    pub(crate) fn busy(&self) -> bool {
        matches!(
            self.state,
            State::Capturing { .. } | State::Reviewing | State::Applying(_)
        )
    }
    pub(crate) fn applying(&self) -> bool {
        matches!(self.state, State::Applying(_))
    }
    pub(crate) fn review(&self) -> Option<&Review> {
        match &self.state {
            State::Ready(review) | State::Applying(review) => Some(review),
            _ => None,
        }
    }
    pub(crate) fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }
    pub(crate) fn progress(&self) -> Option<(usize, usize)> {
        match self.state {
            State::Capturing { done, total } => Some((done, total)),
            State::Captured => Some((100, 100)),
            _ => None,
        }
    }
    pub(crate) fn message(&self) -> (&str, bool) {
        (&self.status, self.error)
    }
    fn success(&mut self, status: impl Into<String>) {
        self.status = status.into();
        self.error = false;
    }
    fn failure(&mut self, status: impl Into<String>) {
        self.status = status.into();
        self.error = true;
    }

    pub(crate) fn start_capture(&mut self, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let path = PathBuf::from(self.path.trim());
        if path.as_os_str().is_empty() {
            self.failure("Choose a path for the configuration archive.");
            return;
        }
        if path.exists() {
            self.failure(format!("Archive path already exists: {}", path.display()));
            return;
        }
        self.state = State::Capturing {
            done: 0,
            total: 100,
        };
        self.summary = None;
        self.success("Capturing complete device configuration; this may take a few minutes…");
        let tx = self.tx.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                device::capture_configuration(|done, total| {
                    let _ = tx.send(WorkerMessage::CaptureProgress(done, total));
                    repaint.request_repaint();
                })
                .map_err(|e| e.to_string())
                .and_then(|configuration| {
                    configuration::save_new(&path, &configuration).map_err(|e| e.to_string())
                })
            }))
            .unwrap_or_else(|_| Err("Configuration capture failed unexpectedly.".into()));
            let _ = tx.send(WorkerMessage::Capture(result));
            repaint.request_repaint();
        });
    }

    pub(crate) fn inspect(&mut self) {
        if self.busy() {
            return;
        }
        self.invalidate_review();
        match configuration::load(std::path::Path::new(self.path.trim())) {
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
                self.summary = Some(format!(
                    "50 macro slots ({macro_nonempty} non-empty) · keymaps: {base_nonempty} base / {function_nonempty} Fn bindings · picture: {} RGB slots · lighting: {} bytes · settings: 4 raw replies",
                    configuration.picture.len(),
                    configuration.lighting.raw().len()
                ));
                self.success("Archive inspected; no device or draft state changed.");
            }
            Err(error) => {
                self.summary = None;
                self.failure(error.to_string());
            }
        }
    }

    pub(crate) fn start_review(&mut self, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let path = PathBuf::from(self.path.trim());
        if path.as_os_str().is_empty() {
            self.failure("Choose an archive to review.");
            return;
        }
        self.state = State::Reviewing;
        self.success("Loading archive and capturing the complete current configuration…");
        let tx = self.tx.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let target = configuration::load(&path).map_err(|e| e.to_string())?;
                let current = device::capture_configuration(|done, total| {
                    let _ = tx.send(WorkerMessage::ReviewProgress(format!(
                        "Capturing current configuration: {done}/{total} macro reads"
                    )));
                    repaint.request_repaint();
                })
                .map_err(|e| e.to_string())?;
                let summary = configuration_plan::plan(&current, &target)?;
                let reverse = configuration_plan::plan(&target, &current)?;
                Ok(Review {
                    current,
                    target,
                    summary,
                    reverse,
                })
            }))
            .unwrap_or_else(|_| Err("Archive review failed unexpectedly.".into()));
            let _ = tx.send(WorkerMessage::Review(result.map(Arc::new)));
            repaint.request_repaint();
        });
    }

    pub(crate) fn start_apply(&mut self, ctx: &egui::Context) {
        if !matches!(self.state, State::Ready(_)) {
            return;
        }
        let State::Ready(review) = std::mem::replace(&mut self.state, State::Idle) else {
            unreachable!()
        };
        self.state = State::Applying(Arc::clone(&review));
        self.success("Applying reviewed archive; backup, verification, and recovery may take several minutes…");
        let backup_dir = self.backup_dir.clone();
        let tx = self.tx.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                device::apply_configuration(&review.current, &review.target, &backup_dir, |message| {
                    let _ = tx.send(WorkerMessage::ApplyProgress(message.to_owned()));
                    repaint.request_repaint();
                }).map(|configuration| configuration.keymaps).map_err(|e| e.to_string())
            })).unwrap_or_else(|_| Err("Archive apply panicked; restoration is unverified. Inspect the backups directory before retrying.".into()));
            let _ = tx.send(WorkerMessage::Apply(result));
            repaint.request_repaint();
        });
    }

    pub(crate) fn handle_close(&mut self, ctx: &egui::Context) -> bool {
        if self.busy() && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.applying() {
                self.success("Archive apply is still running; close is held until verification or recovery finishes.");
                return true;
            }
        }
        false
    }

    pub(crate) fn poll(&mut self) -> PollResult {
        let mut result = PollResult::default();
        while let Ok(message) = self.rx.try_recv() {
            result.changed = true;
            match message {
                WorkerMessage::CaptureProgress(done, total) => {
                    if matches!(self.state, State::Capturing { .. }) {
                        self.state = State::Capturing { done, total };
                    }
                }
                WorkerMessage::Capture(Ok(())) => {
                    self.state = State::Captured;
                    self.success(format!("Configuration archive saved to {}. Captured device state only; drafts were not included.", self.path));
                }
                WorkerMessage::Capture(Err(error)) => {
                    self.state = State::Idle;
                    self.failure(error);
                }
                WorkerMessage::ReviewProgress(message) | WorkerMessage::ApplyProgress(message) => {
                    self.success(message)
                }
                WorkerMessage::Review(Ok(review)) => {
                    self.summary = Some(format!(
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
                    self.state = State::Ready(review);
                    self.success(
                        "Archive review complete. Apply only after checking the planned changes.",
                    );
                }
                WorkerMessage::Review(Err(error)) => {
                    self.state = State::Idle;
                    self.failure(error);
                }
                WorkerMessage::Apply(Ok(snapshot)) => {
                    self.state = State::Idle;
                    self.success(format!("Reviewed archive applied and verified. Other editor panels must reload. Backups: {}", self.backup_dir.display()));
                    result.applied = Some(snapshot);
                }
                WorkerMessage::Apply(Err(error)) => {
                    self.state = State::Idle;
                    self.failure(error);
                }
            }
        }
        result
    }

    #[cfg(test)]
    pub(crate) fn inject_review(&mut self, review: Review) {
        self.state = State::Ready(Arc::new(review));
    }
    #[cfg(test)]
    pub(crate) fn inject_apply_result(&mut self, result: Result<Snapshot, String>) {
        let review = match std::mem::replace(&mut self.state, State::Idle) {
            State::Ready(review) => review,
            State::Applying(review) => review,
            _ => panic!("test apply requires a review"),
        };
        self.state = State::Applying(review);
        self.tx.send(WorkerMessage::Apply(result)).unwrap();
    }
    #[cfg(test)]
    pub(crate) fn inject_apply_progress(&mut self, message: &str) {
        let review = match std::mem::replace(&mut self.state, State::Idle) {
            State::Ready(review) => review,
            State::Applying(review) => review,
            _ => panic!("test apply requires a review"),
        };
        self.state = State::Applying(review);
        self.tx
            .send(WorkerMessage::ApplyProgress(message.into()))
            .unwrap();
    }
}
