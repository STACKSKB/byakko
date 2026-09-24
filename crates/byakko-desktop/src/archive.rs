//! Local native archive capture, file effects and review UI.
use super::{Closing, Desktop, Message as AppMessage};
use crate::panels;
use byakko_core::archive::{ArchiveProblem, ArchiveState, NativeArchive, Review};
use iced::{
    Element, Fill, Task,
    widget::{button, column, row, scrollable, text, text_input},
};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FileState {
    Idle,
    Importing { generation: u64 },
    Exporting { generation: u64 },
}

#[derive(Clone, Debug)]
pub(super) enum Message {
    Path(String),
    Capture,
    ReviewFile,
    Apply,
    Export,
    FileComplete(FileState, Result<Option<Vec<u8>>, String>),
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
                if self.archive_path != path {
                    self.session.clear_archive_review();
                    self.archive_path = path;
                }
            }
            Message::Capture => {
                let request = self.session.request_archive_capture();
                self.submit(request);
            }
            Message::ReviewFile => return self.archive_file_task(true),
            Message::Apply => {
                let request = self.session.request_archive_apply();
                self.submit(request);
            }
            Message::Export => return self.archive_file_task(false),
            Message::FileComplete(..) => unreachable!("completion handled above"),
        }
        Task::none()
    }

    fn archive_file_task(&mut self, import: bool) -> Task<AppMessage> {
        let Some(caps) = self.session.archive_capabilities() else {
            self.notice = Some("Native archives are unavailable".into());
            return Task::none();
        };
        if self.archive_path.trim().is_empty() {
            self.notice = Some("Enter a local archive path".into());
            return Task::none();
        }
        let path = PathBuf::from(self.archive_path.trim());
        let state = if import {
            FileState::Importing {
                generation: self.session.generation(),
            }
        } else {
            FileState::Exporting {
                generation: self.session.generation(),
            }
        };
        let max_bytes = caps.max_bytes;
        let archive = if import {
            None
        } else {
            captured(self.session.archive()).cloned()
        };
        if !import && archive.is_none() {
            self.notice = Some("Capture the current configuration before exporting".into());
            return Task::none();
        }
        self.archive_file = state;
        self.notice = None;
        file_task(state, move || {
            if import {
                read_file(&path, max_bytes).map(Some)
            } else {
                write_new(&path, &archive.expect("checked capture").bytes).map(|()| None)
            }
        })
    }

    fn complete_archive_file(
        &mut self,
        state: FileState,
        result: Result<Option<Vec<u8>>, String>,
    ) -> Task<AppMessage> {
        if self.archive_file != state || state == FileState::Idle {
            return Task::none();
        }
        self.archive_file = FileState::Idle;
        if let FileState::Importing { generation } | FileState::Exporting { generation } = state
            && generation != self.session.generation()
        {
            self.closing = Closing::Open;
            self.notice = Some("Device changed while reading the archive file".into());
            return Task::none();
        }
        match result {
            Err(error) => {
                self.closing = Closing::Open;
                self.notice = Some(error);
            }
            Ok(Some(bytes)) => {
                let Some(caps) = self.session.archive_capabilities() else {
                    self.notice = Some("Native archives are unavailable".into());
                    return Task::none();
                };
                let target = NativeArchive {
                    backend_id: caps.backend_id.clone(),
                    format_id: caps.format_id.clone(),
                    bytes,
                };
                let request = self.session.request_archive_review(target);
                self.submit(request);
                if !self.session.busy() {
                    self.closing = Closing::Open;
                }
            }
            Ok(None) => {
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

fn read_file(path: &PathBuf, max_bytes: u32) -> Result<Vec<u8>, String> {
    let input = File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    input
        .take(u64::from(max_bytes) + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() > max_bytes as usize {
        return Err("Archive file is empty or exceeds the device size limit".into());
    }
    Ok(bytes)
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
    work: impl FnOnce() -> Result<Option<Vec<u8>>, String> + Send + 'static,
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
    let Some(state) = app.session.archive() else {
        return text("Native archives are unavailable on this device").into();
    };
    panels::split(
        &app.ui,
        || panels::panel(&app.ui, "Backup files", file_controls(app)),
        || {
            panels::panel(
                &app.ui,
                "Review",
                scrollable(review(app, state, &app.ui)).height(Fill).into(),
            )
        },
    )
}

fn file_controls(app: &Desktop) -> Element<'_, AppMessage> {
    let state = app.session.archive().expect("archive capability");
    let can_export = captured(Some(state)).is_some() && !app.busy();
    let has_path = !app.archive_path.trim().is_empty();
    let path = text_input("Backup file path", &app.archive_path)
        .on_input_maybe((!app.busy()).then_some(|value| AppMessage::Archive(Message::Path(value))))
        .width(app.ui.fields.regular);
    let mut controls = column![
        path,
        text("Save current configuration"),
        row![
            button("Capture current")
                .on_press_maybe((!app.busy()).then_some(AppMessage::Archive(Message::Capture))),
            button("Save captured file").on_press_maybe(
                (can_export && has_path).then_some(AppMessage::Archive(Message::Export))
            ),
        ]
        .spacing(app.ui.spacing.s),
        text("Open a backup"),
        button("Open and review").on_press_maybe(
            (!app.busy() && has_path).then_some(AppMessage::Archive(Message::ReviewFile))
        ),
    ]
    .spacing(app.ui.spacing.m);
    if let Some(message) = status(app, state) {
        controls = controls.push(text(message));
    }
    controls.into()
}

fn review<'a>(
    app: &'a Desktop,
    state: &'a ArchiveState,
    style: &panels::UiStyle,
) -> Element<'a, AppMessage> {
    match state {
        ArchiveState::Ready(review) => {
            let mut content = column![review_changes(review, style)].spacing(style.spacing.m);
            if !review.changes.is_empty() {
                content =
                    content.push(button("Apply reviewed changes").on_press_maybe(
                        (!app.busy()).then_some(AppMessage::Archive(Message::Apply)),
                    ));
            }
            content.into()
        }
        ArchiveState::Unverified {
            review: Some(review),
            ..
        } => column![
            text("Previous review retained; capture and review again before applying"),
            review_changes(review, style)
        ]
        .spacing(style.spacing.m)
        .into(),
        ArchiveState::Captured(_) | ArchiveState::Idle => {
            text("Open a backup file to compare it with the keyboard.").into()
        }
        ArchiveState::Unverified { review: None, .. } => {
            text("Archive state needs a fresh capture").into()
        }
    }
}

fn review_changes(review: &Review, style: &panels::UiStyle) -> Element<'static, AppMessage> {
    if review.changes.is_empty() {
        return text("No changes in this configuration").into();
    }
    column(review.changes.iter().map(|change| {
        let count = change
            .count
            .map_or(String::new(), |count| format!(" · {count}"));
        text(format!("{}{}", change.label, count)).into()
    }))
    .spacing(style.spacing.s)
    .into()
}

fn status(app: &Desktop, state: &ArchiveState) -> Option<String> {
    match app.archive_file {
        FileState::Importing { .. } => return Some("Opening backup file…".into()),
        FileState::Exporting { .. } => return Some("Saving backup file…".into()),
        FileState::Idle => {}
    }
    if app.session.busy() {
        return Some(super::view::status(app));
    }
    match state {
        ArchiveState::Idle | ArchiveState::Captured(_) | ArchiveState::Ready(_) => None,
        ArchiveState::Unverified { problem, .. } => match problem {
            ArchiveProblem::ReadRequired => Some("Capture again before reviewing".into()),
            ArchiveProblem::Capture(reason)
            | ArchiveProblem::Review(reason)
            | ArchiveProblem::InvalidResult(reason) => Some(reason.clone()),
            ArchiveProblem::Apply(failure) => Some(super::view::apply_failure_label(failure)),
            ArchiveProblem::ApplyReadbackMismatch => {
                Some("Readback differs from the backup. Capture again before applying.".into())
            }
        },
    }
}
