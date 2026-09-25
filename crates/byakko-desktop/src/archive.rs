//! Read-only native configuration capture and diagnostic export UI.
use super::{Closing, Desktop, Message as AppMessage};
use crate::panels;
use byakko_core::archive::{ArchiveProblem, ArchiveState, NativeArchive};
use iced::{
    Element, Task,
    widget::{button, column, row, text, text_input},
};
use std::{fs::OpenOptions, io::Write, path::PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FileState {
    Idle,
    Exporting { generation: u64 },
}

#[derive(Clone, Debug)]
pub(super) enum Message {
    Path(String),
    Capture,
    Export,
    FileComplete(FileState, Result<(), String>),
}

impl Desktop {
    pub(super) fn update_archive(&mut self, message: Message) -> Task<AppMessage> {
        if let Message::FileComplete(state, result) = message {
            return self.complete_archive_file(state, result);
        }
        if self.busy() {
            return Task::none();
        }
        match message {
            Message::Path(path) => {
                self.archive_path = path;
            }
            Message::Capture => {
                let request = self.session.request_archive_capture();
                self.submit(request);
            }
            Message::Export => return self.export_archive_file(),
            Message::FileComplete(..) => unreachable!("completion handled above"),
        }
        Task::none()
    }

    fn export_archive_file(&mut self) -> Task<AppMessage> {
        if self.archive_path.trim().is_empty() {
            self.notice = Some("Enter a local export path".into());
            return Task::none();
        }
        let Some(archive) = captured(self.session.archive()).cloned() else {
            self.notice = Some("Capture the current configuration before exporting".into());
            return Task::none();
        };
        let path = PathBuf::from(self.archive_path.trim());
        let state = FileState::Exporting {
            generation: self.session.generation(),
        };
        self.archive_file = state;
        self.notice = None;
        file_task(state, move || write_new(&path, &archive.bytes))
    }

    fn complete_archive_file(
        &mut self,
        state: FileState,
        result: Result<(), String>,
    ) -> Task<AppMessage> {
        if self.archive_file != state || state == FileState::Idle {
            return Task::none();
        }
        self.archive_file = FileState::Idle;
        if let FileState::Exporting { generation } = state
            && generation != self.session.generation()
        {
            self.closing = Closing::Open;
            self.notice = Some("Device changed while saving the captured configuration".into());
            return Task::none();
        }
        match result {
            Err(error) => {
                self.closing = Closing::Open;
                self.notice = Some(error);
            }
            Ok(()) => {
                self.notice = Some("Exported the captured configuration to a new file".into());
                if self.closing == Closing::Waiting {
                    return self.close();
                }
            }
        }
        Task::none()
    }
}

fn captured(state: Option<&ArchiveState>) -> Option<&NativeArchive> {
    match state? {
        ArchiveState::Captured(archive) => Some(archive),
        ArchiveState::Ready(review) => Some(&review.before),
        ArchiveState::Idle | ArchiveState::Unverified { .. } => None,
    }
}

fn write_new(path: &PathBuf, bytes: &[u8]) -> Result<(), String> {
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    output
        .write_all(bytes)
        .and_then(|()| output.sync_all())
        .map_err(|error| {
            format!(
                "Archive export did not finish at {}: {error}. The partial file was retained; choose a new destination.",
                path.display()
            )
        })
}

fn file_task(
    state: FileState,
    work: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Task<AppMessage> {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    if let Err(error) = std::thread::Builder::new()
        .name("byakko-archive-file".into())
        .spawn(move || {
            let _ = sender.send(work());
        })
    {
        return Task::done(AppMessage::Archive(Message::FileComplete(
            state,
            Err(error.to_string()),
        )));
    }
    Task::perform(
        async move {
            receiver
                .await
                .unwrap_or_else(|_| Err("Archive file worker stopped without a result".into()))
        },
        move |result| AppMessage::Archive(Message::FileComplete(state, result)),
    )
}

pub(super) fn view(app: &Desktop) -> Element<'_, AppMessage> {
    if app.session.archive().is_none() {
        return text("Configuration capture is unavailable on this device").into();
    }
    panels::panel(&app.ui, "Diagnostic capture", file_controls(app))
}

fn file_controls(app: &Desktop) -> Element<'_, AppMessage> {
    let state = app.session.archive().expect("archive capability");
    let can_export = captured(Some(state)).is_some() && !app.busy();
    let has_path = !app.archive_path.trim().is_empty();
    let path = text_input("New capture file path", &app.archive_path)
        .on_input_maybe((!app.busy()).then_some(|value| AppMessage::Archive(Message::Path(value))))
        .width(app.ui.fields.regular);
    let mut controls = column![
        path,
        text("Capture the current configuration for diagnostics."),
        text("Whole-configuration restore is unavailable in this pre-alpha."),
        row![
            button("Capture current")
                .on_press_maybe((!app.busy()).then_some(AppMessage::Archive(Message::Capture))),
            button("Save captured file").on_press_maybe(
                (can_export && has_path).then_some(AppMessage::Archive(Message::Export))
            ),
        ]
        .spacing(app.ui.spacing.s),
    ]
    .spacing(app.ui.spacing.m);
    if let Some(message) = status(app, state) {
        controls = controls.push(text(message));
    }
    controls.into()
}

fn status(app: &Desktop, state: &ArchiveState) -> Option<String> {
    match app.archive_file {
        FileState::Exporting { .. } => return Some("Saving captured configuration…".into()),
        FileState::Idle => {}
    }
    if app.session.busy() {
        return Some(super::view::status(app));
    }
    match state {
        ArchiveState::Idle | ArchiveState::Captured(_) | ArchiveState::Ready(_) => None,
        ArchiveState::Unverified { problem, .. } => match problem {
            ArchiveProblem::ReadRequired => Some("Capture the current configuration again".into()),
            ArchiveProblem::Capture(reason)
            | ArchiveProblem::Review(reason)
            | ArchiveProblem::InvalidResult(reason) => Some(reason.clone()),
            ArchiveProblem::Apply(failure) => Some(super::view::apply_failure_label(failure)),
            ArchiveProblem::ApplyReadbackMismatch => Some(
                "Previous archive verification failed. Capture the current configuration again."
                    .into(),
            ),
        },
    }
}
