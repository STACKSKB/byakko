//! Desktop adapter. Domain decisions remain in core; firmware lives outside views.
mod archive;
mod audio_stream;
mod control_widgets;
pub mod discovery;
mod lighting;
mod macro_binding_view;
mod macro_editor;
mod macro_files;
mod macro_form;
mod macro_view;
mod panels;
mod picture;
mod recording;
mod recording_input;
mod screen_stream;
mod settings;
#[cfg(test)]
mod tests;
mod view;

use byakko_core::{
    Change,
    session::{Acceptance, Command, Completion, Problem, Session, Status},
};
use byakko_devices::Executor;
use discovery::{Availability, Discovery};
use iced::{Element, Subscription, Task, window};
use std::{sync::mpsc::TryRecvError, time::Duration};

#[derive(Clone, Debug)]
enum Message {
    Archive(archive::Message),
    Lighting(lighting::Message),
    Picture(picture::Message),
    Settings(settings::Message),
    File(macro_files::Message),
    Record(recording::Message),
    Page(Page),
    Macro(macro_editor::Message),
    SelectLayer(String),
    SelectKey(String),
    Search(String),
    Stage(usize),
    Read,
    Apply,
    Revert,
    Poll,
    Scan,
    Close,
    DiscardAndClose,
    KeepEditing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Page {
    Archive,
    Keys,
    Macros,
    Lighting,
    Picture,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Closing {
    Open,
    Waiting,
    ConfirmDiscard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AutoRead {
    Enabled,
    ManualOnly,
}

type Attach = dyn Fn(&str) -> Result<Executor, String>;

struct Desktop {
    ui: panels::UiStyle,
    archive_file: archive::FileState,
    archive_path: String,
    picture_selected: Option<String>,
    settings_selected: Option<String>,
    macro_files: macro_files::Fields,
    clock: std::time::Instant,
    recording_options: recording::Options,
    host: Option<lighting::HostInput>,
    page: Page,
    macro_form: macro_form::Form,
    repeat_input: String,
    session: Session,
    executor: Option<Executor>,
    attach: Box<Attach>,
    discovery: Discovery,
    presence: Option<Availability>,
    selected_device: Option<String>,
    auto_read: AutoRead,
    layer: String,
    selected: Option<String>,
    search: String,
    notice: Option<String>,
    closing: Closing,
}

/// The composition root supplies a session and a factory for one executor per connection.
pub fn run(
    session: Session,
    probe: impl Fn() -> Availability + Send + 'static,
    attach: impl Fn(&str) -> Result<Executor, String> + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
    let layer = session.descriptor().layers[0].id.clone();
    let mut discovery = Discovery::spawn(probe)?;
    discovery.request();
    // Iced's boot closure is reusable; this native session has exactly one owner.
    let initial = std::cell::RefCell::new(Some(Desktop {
        ui: panels::UiStyle::DEFAULT,
        archive_file: archive::FileState::Idle,
        archive_path: String::new(),
        picture_selected: None,
        settings_selected: None,
        macro_files: Default::default(),
        clock: std::time::Instant::now(),
        recording_options: Default::default(),
        host: None,
        page: Page::Keys,
        macro_form: Default::default(),
        repeat_input: String::new(),
        session,
        executor: None,
        attach: Box::new(attach),
        discovery,
        presence: None,
        selected_device: None,
        auto_read: AutoRead::Enabled,
        layer,
        selected: None,
        search: String::new(),
        notice: None,
        closing: Closing::Open,
    }));
    iced::application(
        move || initial.borrow_mut().take().expect("single desktop boot"),
        Desktop::update,
        Desktop::view,
    )
    .title("Byakko")
    .theme(panels::UiStyle::DEFAULT.theme)
    .subscription(Desktop::subscription)
    .exit_on_close_request(false)
    .window_size(panels::UiStyle::DEFAULT.initial_window)
    .run()?;
    Ok(())
}

impl Desktop {
    fn busy(&self) -> bool {
        self.session.busy() || self.host.is_some() || self.archive_file != archive::FileState::Idle
    }

    fn read(&mut self) {
        if self.busy() {
            return;
        }
        if self.executor.is_none() {
            let Some(Availability::Ready { id }) = &self.presence else {
                self.notice = Some("Waiting for one connected keyboard".into());
                return;
            };
            match (self.attach)(id) {
                Ok(executor) => {
                    self.executor = Some(executor);
                    self.selected_device = Some(id.clone());
                }
                Err(reason) => {
                    self.notice = Some(format!("Could not open keyboard: {reason}"));
                    return;
                }
            }
        }
        let request = self.session.connect().and_then(|generation| {
            self.executor
                .as_ref()
                .expect("attached above")
                .set_generation(generation);
            self.session.request_read()
        });
        self.submit(request);
    }

    fn submit(&mut self, request: Result<Command, String>) {
        self.notice = None;
        match request {
            Ok(command) => {
                self.discovery.invalidate();
                if let Some(executor) = &self.executor {
                    if let Err(completion) = executor.try_submit(command) {
                        self.session.accept(*completion);
                    }
                } else {
                    self.session.disconnect();
                    self.notice = Some("Device executor is unavailable".into());
                }
            }
            Err(message) => self.notice = Some(message),
        }
    }

    fn stage(&mut self, index: usize) {
        let Some(key) = self.selected.as_ref() else {
            return;
        };
        let Some(choice) = self.session.descriptor().actions.get(index) else {
            return;
        };
        self.notice = self
            .session
            .stage(Change {
                layer: self.layer.clone(),
                key: key.clone(),
                action: choice.action.clone(),
            })
            .err();
    }

    fn poll(&mut self) -> Task<Message> {
        if let Some(task) = self.poll_host() {
            return task;
        }
        self.poll_host_input();
        let Some(executor) = &self.executor else {
            return Task::none();
        };
        match executor.try_receive() {
            Ok(completion) => return self.complete(completion),
            Err(TryRecvError::Empty) => return Task::none(),
            Err(TryRecvError::Disconnected) => {
                executor.set_generation(0);
                let write_in_flight = matches!(
                    self.session.activity(),
                    byakko_core::session::Activity::Apply { .. }
                        | byakko_core::session::Activity::ApplyMacro { .. }
                        | byakko_core::session::Activity::ApplyLighting { .. }
                        | byakko_core::session::Activity::ApplyPicture { .. }
                        | byakko_core::session::Activity::ApplySetting { .. }
                        | byakko_core::session::Activity::ApplyArchive { .. }
                );
                if write_in_flight {
                    self.auto_read = AutoRead::ManualOnly;
                }
                self.executor = None;
                self.session.disconnect();
                self.notice = Some(
                    "Device worker stopped; device state is unverified. Read again before editing."
                        .into(),
                );
                self.closing = Closing::Open;
            }
        }
        Task::none()
    }

    fn scan(&mut self) {
        // Even after a disconnect, a completion may arrive for the old generation.
        // Consume it before submitting any command to the bounded worker queue.
        if !self.busy() {
            let _ = self.poll();
        }
        if let Some(availability) = self.discovery.receive() {
            self.accept_availability(availability);
        }
        if !self.busy() {
            self.discovery.request();
        }
    }

    fn accept_availability(&mut self, availability: Availability) {
        if self.busy() {
            return;
        }
        let previous = self.selected_device.as_deref();
        let changed = matches!(&availability, Availability::Ready { id } if previous.is_some_and(|old| old != id));
        if changed || !matches!(availability, Availability::Ready { .. }) {
            if matches!(
                self.session.status(),
                Status::Conflict { .. }
                    | Status::Unverified {
                        problem: Problem::Apply(_)
                            | Problem::InvalidApplyResult(_)
                            | Problem::ApplyReadbackMismatch
                    }
            ) {
                self.auto_read = AutoRead::ManualOnly;
                self.notice = Some(format!(
                    "{} · Read manually after reconnecting.",
                    view::status(self)
                ));
            }
            if let Some(executor) = &self.executor {
                executor.set_generation(0);
            }
            self.executor = None;
            self.session.disconnect();
        }
        self.presence = Some(availability.clone());
        match &availability {
            Availability::Ready { .. } => {
                if self.executor.is_none() {
                    self.selected_device = None;
                }
                if self.auto_read == AutoRead::Enabled
                    && matches!(
                        self.session.status(),
                        Status::Disconnected
                            | Status::Unverified {
                                problem: Problem::Read(_)
                            }
                    )
                {
                    self.read();
                }
            }
            _ => self.selected_device = None,
        }
    }

    fn complete(&mut self, completion: Completion) -> Task<Message> {
        let archive_result = matches!(
            completion,
            Completion::CaptureArchive { .. }
                | Completion::ReviewArchive { .. }
                | Completion::ApplyArchive { .. }
        );
        let lighting_result = matches!(
            completion,
            Completion::ReadLighting { .. } | Completion::ApplyLighting { .. }
        );
        let picture_result = matches!(
            completion,
            Completion::ReadPicture { .. } | Completion::ApplyPicture { .. }
        );
        let settings_result = matches!(
            completion,
            Completion::ReadSettings { .. } | Completion::ApplySetting { .. }
        );
        let macro_result = matches!(
            completion,
            Completion::ReadMacro { .. } | Completion::ApplyMacro { .. }
        );
        if self.session.accept(completion) == Acceptance::IgnoredStale {
            return Task::none();
        }
        let verified = if archive_result {
            self.session.archive().is_some_and(|state| {
                matches!(
                    state,
                    byakko_core::archive::ArchiveState::Captured(_)
                        | byakko_core::archive::ArchiveState::Ready(_)
                )
            })
        } else if settings_result {
            self.session.settings().is_some_and(|editor| {
                *editor.status() == byakko_core::settings::editor::Status::Ready
            })
        } else if picture_result {
            self.session.picture().is_some_and(|editor| {
                *editor.status() == byakko_core::picture::editor::Status::Ready
            })
        } else if lighting_result {
            self.session.lighting().is_some_and(|editor| {
                *editor.status() == byakko_core::lighting::editor::Status::Ready
            })
        } else if macro_result {
            let verified = self.session.macros().is_some_and(|editor| {
                *editor.status() == byakko_core::macros::editor::Status::Ready
            });
            if verified {
                self.reset_macro_inputs();
            }
            verified
        } else {
            *self.session.status() == Status::Ready
        };
        if self.closing == Closing::Waiting && !self.busy() {
            // Keep failures visible; a close request must not hide an uncertain write.
            if !verified {
                self.closing = Closing::Open;
            } else {
                return self.close();
            }
        }
        Task::none()
    }

    fn close(&mut self) -> Task<Message> {
        if self.session.recording() && !self.finish_recording(std::time::Instant::now()) {
            return Task::none();
        }
        if self.host.is_some() {
            self.stop_host();
            if self.session.busy() {
                self.closing = Closing::Waiting;
                return Task::none();
            }
        }
        if self.busy() {
            self.closing = Closing::Waiting;
        } else if self.session.dirty() {
            self.closing = Closing::ConfirmDiscard;
        } else {
            return iced::exit();
        }
        Task::none()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        if self.session.recording() && !matches!(message, Message::Record(_) | Message::Close) {
            return Task::none();
        }
        if self.closing == Closing::ConfirmDiscard
            && !matches!(
                message,
                Message::DiscardAndClose | Message::Close | Message::Poll | Message::Scan
            )
        {
            self.closing = Closing::Open;
        }
        match message {
            Message::Archive(message) => return self.update_archive(message),
            Message::Lighting(message) => self.update_lighting(message),
            Message::Picture(message) => self.update_picture(message),
            Message::Settings(message) => self.update_settings(message),
            Message::File(message) => return self.update_macro_files(message),
            Message::Record(message) => self.update_recording(message),
            Message::Page(page) => self.page = page,
            Message::Macro(message) => self.update_macro(message),
            Message::SelectLayer(layer) => self.layer = layer,
            Message::SelectKey(key) => self.selected = Some(key),
            Message::Search(search) => self.search = search,
            Message::Stage(index) => self.stage(index),
            Message::Read => {
                self.auto_read = AutoRead::Enabled;
                self.read();
            }
            Message::Apply => {
                let request = self.session.request_apply();
                self.submit(request);
            }
            Message::Revert => self.notice = self.session.revert().err(),
            Message::Poll => return self.poll(),
            Message::Scan if self.closing == Closing::ConfirmDiscard => {}
            Message::Scan => self.scan(),
            Message::Close => return self.close(),
            Message::DiscardAndClose if !self.busy() => return iced::exit(),
            Message::DiscardAndClose => {}
            Message::KeepEditing => self.closing = Closing::Open,
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let close = window::close_requests().map(|_| Message::Close);
        if self.closing == Closing::ConfirmDiscard {
            return close;
        }
        if self.session.recording() {
            Subscription::batch([close, recording::subscription()])
        } else if self.busy()
            && !matches!(
                self.session.activity(),
                byakko_core::session::Activity::MacroFile { .. }
            )
        {
            Subscription::batch([
                close,
                iced::time::every(Duration::from_millis(25)).map(|_| Message::Poll),
            ])
        } else {
            Subscription::batch([
                close,
                iced::time::every(if self.presence.is_none() {
                    Duration::from_millis(100)
                } else {
                    Duration::from_secs(2)
                })
                .map(|_| Message::Scan),
            ])
        }
    }

    fn view(&self) -> Element<'_, Message> {
        if self.session.recording() {
            recording::capture_view(self)
        } else {
            view::shell(self)
        }
    }
}
