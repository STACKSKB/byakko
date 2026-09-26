//! Window lifecycle and effect delivery. Feature policy stays in core.
use crate::{keymap, panels::UiStyle};
use byakko_core::{
    contract::{Command, Completion, Problem},
    keymap::Status,
    session::{Connection, Outcome, Session},
};
use byakko_devices::Executor;
use iced::{
    Element, Fill, Subscription, Task,
    widget::{button, column, container, row, text},
    window,
};
use std::{sync::mpsc::TryRecvError, time::Duration};

type Attach = dyn Fn(Option<&str>) -> Result<(String, Executor), String>;

#[derive(Clone, Debug)]
enum Message {
    Keys(keymap::Message),
    Read,
    Save,
    Revert,
    Poll,
    Close,
    Discard,
    KeepEditing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Closing {
    Open,
    Waiting,
    ConfirmDiscard,
}

struct App {
    session: Session,
    keys: keymap::Form,
    worker: Option<Executor>,
    selected_device: Option<String>,
    attach: Box<Attach>,
    closing: Closing,
    notice: String,
    style: UiStyle,
}

pub fn run(
    session: Session,
    attach: impl Fn(Option<&str>) -> Result<(String, Executor), String> + 'static,
) -> iced::Result {
    let initial = std::cell::RefCell::new(Some(App::new(session, Box::new(attach))));
    iced::application(
        move || {
            (
                initial
                    .borrow_mut()
                    .take()
                    .expect("one window owns the session"),
                Task::done(Message::Read),
            )
        },
        App::update,
        App::view,
    )
    .title("Byakko")
    .theme(UiStyle::DEFAULT.theme)
    .window_size(UiStyle::DEFAULT.initial_window)
    .subscription(App::subscription)
    .exit_on_close_request(false)
    .run()
}

impl App {
    fn new(session: Session, attach: Box<Attach>) -> Self {
        Self {
            keys: keymap::Form::new(session.descriptor()),
            session,
            worker: None,
            selected_device: None,
            attach,
            closing: Closing::Open,
            notice: String::new(),
            style: UiStyle::DEFAULT,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        if self.closing != Closing::Open
            && !matches!(
                message,
                Message::Poll | Message::Close | Message::Discard | Message::KeepEditing
            )
        {
            return Task::none();
        }
        match message {
            Message::Keys(message) => {
                if let Some(change) = self.keys.update(message, self.session.descriptor()) {
                    self.notice = self.session.edit(change).err().unwrap_or_default();
                }
            }
            Message::Read if !self.session.busy() => {
                let request = self.connect().and_then(|()| self.session.read());
                return self.submit(request);
            }
            Message::Save => {
                let request = self.session.save();
                return self.submit(request);
            }
            Message::Revert => self.notice = self.session.revert().err().unwrap_or_default(),
            Message::Poll => return self.poll(),
            Message::Close => return self.close(),
            Message::Discard if self.closing == Closing::ConfirmDiscard => return iced::exit(),
            Message::KeepEditing => self.closing = Closing::Open,
            Message::Read | Message::Discard => {}
        }
        Task::none()
    }

    fn connect(&mut self) -> Result<(), String> {
        if self.worker.is_some() {
            return Ok(());
        }
        let (id, worker) = (self.attach)(self.selected_device.as_deref())?;
        let generation = self.session.connect()?;
        worker.set_generation(generation);
        self.selected_device = Some(id);
        self.worker = Some(worker);
        Ok(())
    }

    fn submit(&mut self, request: Result<Command, String>) -> Task<Message> {
        let command = match request {
            Ok(command) => command,
            Err(reason) => {
                self.notice = reason;
                return Task::none();
            }
        };
        self.notice.clear();
        match &self.worker {
            Some(worker) => {
                if let Err(completion) = worker.try_submit(command) {
                    return self.complete(*completion);
                }
            }
            None => {
                self.session.disconnect();
                self.notice = "Read the keyboard before editing.".into();
            }
        }
        Task::none()
    }

    fn poll(&mut self) -> Task<Message> {
        let Some(worker) = &self.worker else {
            return Task::none();
        };
        match worker.try_receive() {
            Ok(completion) => self.complete(completion),
            Err(TryRecvError::Empty) => Task::none(),
            Err(TryRecvError::Disconnected) => {
                worker.set_generation(0);
                self.worker = None;
                self.session.disconnect();
                self.closing = Closing::Open;
                self.notice =
                    "Connection lost. Edits are retained; reconnect and read before saving.".into();
                Task::none()
            }
        }
    }

    fn complete(&mut self, completion: Completion) -> Task<Message> {
        match self.session.accept(completion) {
            Outcome::Ignored => return Task::none(),
            Outcome::Loaded => self.notice = "Keymap loaded.".into(),
            Outcome::Saved => self.notice = "Assignments saved and read back.".into(),
            Outcome::Conflict => {
                self.closing = Closing::Open;
                self.notice =
                    "The observed keymap differs from your edit baseline. Edits are retained."
                        .into();
                return Task::none();
            }
            Outcome::Failed(problem) => {
                self.closing = Closing::Open;
                self.notice = problem_text(&problem);
                return Task::none();
            }
        }
        if self.closing == Closing::Waiting {
            self.close()
        } else {
            Task::none()
        }
    }

    fn close(&mut self) -> Task<Message> {
        if self.session.busy() {
            self.closing = Closing::Waiting;
        } else if self.session.keymap().dirty() {
            self.closing = Closing::ConfirmDiscard;
        } else {
            return iced::exit();
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let close = window::close_requests().map(|_| Message::Close);
        if self.session.busy() {
            Subscription::batch([
                close,
                iced::time::every(Duration::from_millis(25)).map(|_| Message::Poll),
            ])
        } else {
            close
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let idle = !self.session.busy();
        let editable = idle
            && matches!(self.session.connection(), Connection::Connected { .. })
            && self.session.keymap().status() == &Status::Ready;
        let toolbar = row![
            button("Read / reconnect").on_press_maybe(idle.then_some(Message::Read)),
            button("Save assignments").on_press_maybe(
                (editable && self.session.keymap().dirty()).then_some(Message::Save)
            ),
            button("Revert")
                .on_press_maybe((idle && self.session.keymap().dirty()).then_some(Message::Revert)),
        ]
        .spacing(self.style.spacing.s);
        let status = if self.closing == Closing::Waiting {
            "Waiting for the device operation before closing…"
        } else if !idle {
            "Working…"
        } else {
            &self.notice
        };
        let base: Element<'_, Message> = container(
            column![
                text(&self.session.descriptor().device_name).size(self.style.type_scale.page_title),
                toolbar,
                text(status),
                self.keys
                    .view(
                        self.session.descriptor(),
                        self.session.keymap(),
                        editable,
                        &self.style
                    )
                    .map(Message::Keys),
            ]
            .spacing(self.style.spacing.m),
        )
        .padding(self.style.spacing.page_padding)
        .max_width(self.style.workspace_width)
        .center_x(Fill)
        .height(Fill)
        .into();
        if self.closing != Closing::ConfirmDiscard {
            return base;
        }
        let dialog = container(
            column![
                text("Discard unsaved changes?").size(self.style.type_scale.section_title),
                row![
                    button("Keep editing").on_press(Message::KeepEditing),
                    button("Discard & close").on_press(Message::Discard)
                ]
                .spacing(self.style.spacing.m),
            ]
            .spacing(self.style.spacing.l),
        )
        .padding(self.style.spacing.panel_padding)
        .style(container::bordered_box);
        iced::widget::stack![
            base,
            container(iced::widget::opaque(dialog))
                .center_x(Fill)
                .center_y(Fill)
        ]
        .into()
    }
}

fn problem_text(problem: &Problem) -> String {
    match problem {
        Problem::ReadRequired => "Read the keyboard before editing.".into(),
        Problem::Read(reason) => format!("Read failed: {reason}"),
        Problem::Apply(failure) => format!(
            "Save failed: {}. Recovery: {:?}",
            failure.message, failure.recovery
        ),
        Problem::InvalidApplyResult(reason) => format!("Save result was invalid: {reason}"),
        Problem::ApplyReadbackMismatch => {
            "Readback did not match the submitted assignments.".into()
        }
    }
}

#[cfg(test)]
mod tests;
