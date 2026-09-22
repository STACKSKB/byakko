//! Desktop adapter. Domain decisions remain in core; firmware lives outside views.
mod control_widgets;
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
#[cfg(test)]
mod tests;
mod view;

use byakko_core::{
    Change,
    session::{Acceptance, Command, Completion, Session, Status},
};
use byakko_devices::Executor;
use iced::{Element, Subscription, Task, window};
use std::{sync::mpsc::TryRecvError, time::Duration};

#[derive(Clone, Debug)]
enum Message {
    Lighting(lighting::Message),
    Picture(picture::Message),
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
    Close,
    DiscardAndClose,
    KeepEditing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Page {
    Keys,
    Macros,
    Lighting,
    Picture,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Closing {
    Open,
    Waiting,
    ConfirmDiscard,
}

struct Desktop {
    ui: panels::UiStyle,
    picture_selected: Option<String>,
    macro_files: macro_files::Fields,
    clock: std::time::Instant,
    recording_options: recording::Options,
    page: Page,
    macro_form: macro_form::Form,
    repeat_input: String,
    session: Session,
    executor: Executor,
    layer: String,
    selected: Option<String>,
    search: String,
    notice: Option<String>,
    closing: Closing,
}

/// The composition root supplies a configured session and its executor together.
pub fn run(session: Session, executor: Executor) -> Result<(), Box<dyn std::error::Error>> {
    let layer = session.descriptor().layers[0].id.clone();
    // Iced's boot closure is reusable; this native session has exactly one owner.
    let initial = std::cell::RefCell::new(Some(Desktop {
        ui: panels::UiStyle::DEFAULT,
        picture_selected: None,
        macro_files: Default::default(),
        clock: std::time::Instant::now(),
        recording_options: Default::default(),
        page: Page::Keys,
        macro_form: Default::default(),
        repeat_input: String::new(),
        session,
        executor,
        layer,
        selected: None,
        search: String::new(),
        notice: None,
        closing: Closing::Open,
    }));
    iced::application(
        move || {
            let mut desktop = initial.borrow_mut().take().expect("single desktop boot");
            desktop.read();
            desktop
        },
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
        self.session.busy()
    }

    fn read(&mut self) {
        if self.busy() {
            return;
        }
        let request = self.session.connect().and_then(|generation| {
            self.executor.set_generation(generation);
            self.session.request_read()
        });
        self.submit(request);
    }

    fn submit(&mut self, request: Result<Command, String>) {
        self.notice = None;
        match request {
            Ok(command) => {
                if let Err(completion) = self.executor.try_submit(command) {
                    self.session.accept(*completion);
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
        use byakko_core::session::Activity;
        if !matches!(
            self.session.activity(),
            Activity::Read { .. }
                | Activity::Apply { .. }
                | Activity::ReadMacro { .. }
                | Activity::ApplyMacro { .. }
                | Activity::ReadLighting { .. }
                | Activity::ApplyLighting { .. }
                | Activity::ReadPicture { .. }
                | Activity::ApplyPicture { .. }
        ) {
            return Task::none();
        }
        match self.executor.try_receive() {
            Ok(completion) => return self.complete(completion),
            Err(TryRecvError::Empty) => return Task::none(),
            Err(TryRecvError::Disconnected) => {
                self.session.disconnect();
                self.notice = Some(
                    "Device worker stopped; device state is unverified. Restart to reconnect."
                        .into(),
                );
                self.closing = Closing::Open;
            }
        }
        Task::none()
    }

    fn complete(&mut self, completion: Completion) -> Task<Message> {
        let lighting_result = matches!(
            completion,
            Completion::ReadLighting { .. } | Completion::ApplyLighting { .. }
        );
        let picture_result = matches!(
            completion,
            Completion::ReadPicture { .. } | Completion::ApplyPicture { .. }
        );
        let macro_result = matches!(
            completion,
            Completion::ReadMacro { .. } | Completion::ApplyMacro { .. }
        );
        if self.session.accept(completion) == Acceptance::IgnoredStale {
            return Task::none();
        }
        let verified = if picture_result {
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
                Message::DiscardAndClose | Message::Close | Message::Poll
            )
        {
            self.closing = Closing::Open;
        }
        match message {
            Message::Lighting(message) => self.update_lighting(message),
            Message::Picture(message) => self.update_picture(message),
            Message::File(message) => return self.update_macro_files(message),
            Message::Record(message) => self.update_recording(message),
            Message::Page(page) => self.page = page,
            Message::Macro(message) => self.update_macro(message),
            Message::SelectLayer(layer) => self.layer = layer,
            Message::SelectKey(key) => self.selected = Some(key),
            Message::Search(search) => self.search = search,
            Message::Stage(index) => self.stage(index),
            Message::Read => self.read(),
            Message::Apply => {
                let request = self.session.request_apply();
                self.submit(request);
            }
            Message::Revert => self.notice = self.session.revert().err(),
            Message::Poll => return self.poll(),
            Message::Close => return self.close(),
            Message::DiscardAndClose if !self.busy() => return iced::exit(),
            Message::DiscardAndClose => {}
            Message::KeepEditing => self.closing = Closing::Open,
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let close = window::close_requests().map(|_| Message::Close);
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
            close
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
