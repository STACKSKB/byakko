//! Window lifecycle and effect delivery. Feature policy stays in core.
use crate::{
    controller::recording::Controller as Recording,
    form::{keymap, macros, recording},
    input, view,
    widget::panels::UiStyle,
};
use byakko_core::{
    contract::{Command, Completion, Problem},
    editor::Status,
    session::{Connection, Outcome, Session},
    workflow::macro_assignment::AssignmentProblem,
};
use byakko_devices::Executor;
use iced::{
    Element, Event, Fill, Subscription, Task,
    widget::{button, column, container, row, text},
    window,
};
use std::{
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

type Attach = dyn Fn(Option<&str>) -> Result<(String, Executor), String>;

#[derive(Clone, Debug)]
enum Message {
    Keys(keymap::Message),
    Macros(macros::Message),
    Record(recording::Message),
    RecordingInput(Event, Instant),
    Page(Page),
    Read,
    Save,
    Revert,
    Poll,
    Close,
    Discard,
    KeepEditing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Page {
    Keys,
    Macros,
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
    macros: macros::Form,
    recording: Recording,
    page: Page,
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
            macros: macros::Form::default(),
            recording: Recording::default(),
            page: Page::Keys,
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
        if (self.session.recording() || self.recording.pending())
            && !matches!(
                message,
                Message::Record(recording::Message::Stop)
                    | Message::RecordingInput(..)
                    | Message::Close
                    | Message::Poll
            )
        {
            return Task::none();
        }
        match message {
            Message::Record(message) => {
                let was_recording = self.session.recording();
                let result =
                    self.recording
                        .update(message, &mut self.session, self.worker.as_ref());
                self.recording_notice(result, was_recording);
            }
            Message::RecordingInput(event, at) => {
                let was_recording = self.session.recording();
                let result = self.recording.input(&event, at, &mut self.session);
                self.recording_notice(result, was_recording);
            }
            Message::Page(page) => self.page = page,
            Message::Macros(message) => return self.update_macro(message),
            Message::Keys(message) => {
                if let Some(change) = self.keys.update(message, self.session.descriptor()) {
                    self.notice = self.session.edit(change).err().unwrap_or_default();
                }
            }
            Message::Read if !self.session.busy() => {
                let connected = match self.connect() {
                    Ok(connected) => connected,
                    Err(reason) => {
                        self.notice = reason;
                        return Task::none();
                    }
                };
                let request = self.session.read();
                let read = self.submit(request);
                if connected && self.session.macros().is_some() {
                    let catalog = self.session.request_macro_catalog();
                    return Task::batch([read, self.submit(catalog)]);
                }
                return read;
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

    fn recording_notice(&mut self, result: Result<Option<String>, String>, was_recording: bool) {
        match result {
            Ok(Some(notice)) | Err(notice) => self.notice = notice,
            Ok(None) => {}
        }
        if self.session.recording() || self.recording.pending() {
            self.page = Page::Macros;
        } else if was_recording {
            self.macros
                .sync(self.session.macros().and_then(|editor| editor.draft()));
        }
    }

    fn update_macro(&mut self, message: macros::Message) -> Task<Message> {
        use macros::Message;
        let request = match message {
            Message::Select(slot)
                if self.session.macros().is_some_and(|editor| {
                    editor.slot() == slot && editor.status() == &Status::Ready
                }) =>
            {
                return Task::none();
            }
            Message::Select(slot) => self
                .session
                .select_macro(&slot)
                .and_then(|()| self.session.read_macro()),
            Message::Add => self
                .session
                .macro_candidate()
                .and_then(|slot| self.session.select_macro(&slot))
                .and_then(|()| self.session.read_macro()),
            Message::Read => self.session.read_macro(),
            Message::Save => self.session.save_macro(),
            Message::Assign(binding) => match self.keys.target() {
                Some((layer, key)) => self.session.save_and_assign_macro(layer, key, &binding),
                None => Err("Select a key before assigning a macro".into()),
            },
            Message::Revert => {
                self.notice = self.session.revert_macro().err().unwrap_or_default();
                self.macros
                    .sync(self.session.macros().and_then(|editor| editor.draft()));
                return Task::none();
            }
            message => {
                let edit = self
                    .session
                    .macros()
                    .ok_or_else(|| "Macros are not available".to_owned())
                    .and_then(|editor| self.macros.update(message, editor));
                self.notice = edit
                    .and_then(|edit| edit.map_or(Ok(()), |edit| self.session.edit_macro(edit)))
                    .err()
                    .unwrap_or_default();
                return Task::none();
            }
        };
        self.submit(request)
    }

    fn connect(&mut self) -> Result<bool, String> {
        if self.worker.is_some() {
            return Ok(false);
        }
        let (id, worker) = (self.attach)(self.selected_device.as_deref())?;
        let generation = self.session.connect()?;
        worker.set_generation(generation);
        self.selected_device = Some(id);
        self.worker = Some(worker);
        Ok(true)
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
                self.notice = self
                    .session
                    .disconnect()
                    .err()
                    .unwrap_or_else(|| "Read the keyboard before editing.".into());
            }
        }
        Task::none()
    }

    fn poll(&mut self) -> Task<Message> {
        if self.session.recording() {
            return Task::none();
        }
        let Some(worker) = &self.worker else {
            return Task::none();
        };
        match worker.try_receive() {
            Ok(completion) => self.complete(completion),
            Err(TryRecvError::Empty) => Task::none(),
            Err(TryRecvError::Disconnected) => {
                worker.set_generation(0);
                self.worker = None;
                if self.recording.pending() {
                    let _ = self.recording.finish(&mut self.session, Instant::now());
                }
                let failure = self.session.disconnect().err();
                self.closing = Closing::Open;
                self.notice = failure.unwrap_or_else(|| {
                    "Connection lost. Edits are retained; reconnect and read before saving.".into()
                });
                Task::none()
            }
        }
    }

    fn complete(&mut self, completion: Completion) -> Task<Message> {
        match self.session.accept(completion) {
            Outcome::Ignored => return Task::none(),
            Outcome::Loaded => {
                self.notice = "Keymap loaded.".into();
            }
            Outcome::Saved => self.notice = "Assignments saved and read back.".into(),
            Outcome::Continue(command) => return self.submit(Ok(command)),
            Outcome::MacroLoaded => {
                self.macros
                    .sync(self.session.macros().and_then(|editor| editor.draft()));
                self.notice = "Macro loaded.".into();
            }
            Outcome::MacroSaved => self.notice = "Macro saved and read back.".into(),
            Outcome::CatalogLoaded => {
                let result = self.recording.catalog_finished(&mut self.session);
                self.recording_notice(result, false);
                return Task::none();
            }
            Outcome::CatalogFailed(reason) => {
                if !self.recording.pending() {
                    self.notice = format!("Macro discovery stopped: {reason}");
                }
                let result = self.recording.catalog_finished(&mut self.session);
                self.recording_notice(result, false);
                return Task::none();
            }
            Outcome::AssignmentSucceeded { macro_saved } => {
                self.notice = if macro_saved {
                    "Macro saved and assigned."
                } else {
                    "Macro assigned."
                }
                .into()
            }
            Outcome::AssignmentFailed {
                macro_saved,
                problem,
            } => {
                self.closing = Closing::Open;
                let reason = match problem {
                    AssignmentProblem::Device(problem) => problem_text(&problem),
                    AssignmentProblem::Validation(reason) => reason,
                };
                self.notice = format!(
                    "{} {reason}",
                    if macro_saved {
                        "Macro saved; assignment failed."
                    } else {
                        "Macro assignment failed."
                    }
                );
                return Task::none();
            }
            Outcome::Conflict => {
                self.closing = Closing::Open;
                self.notice =
                    "The observed feature differs from your edit baseline. Edits are retained."
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
        if self.session.recording() || self.recording.pending() {
            match self.recording.finish(&mut self.session, Instant::now()) {
                Ok(notice) => {
                    self.notice = notice;
                    self.macros
                        .sync(self.session.macros().and_then(|editor| editor.draft()));
                }
                Err(reason) => {
                    self.notice = reason;
                    return Task::none();
                }
            }
        }
        if self.session.busy() {
            self.closing = Closing::Waiting;
        } else if self.session.keymap().dirty()
            || self.session.macros().is_some_and(|editor| editor.dirty())
        {
            self.closing = Closing::ConfirmDiscard;
        } else {
            return iced::exit();
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let close = window::close_requests().map(|_| Message::Close);
        let mut subscriptions = vec![close];
        if self.session.recording() || self.recording.pending() {
            subscriptions.push(iced::event::listen_with(|event, status, _| {
                input::recording::captures(&event, status)
                    .then(|| Message::RecordingInput(event, Instant::now()))
            }));
        }
        if !self.session.recording() && (self.session.busy() || self.session.catalog_scanning()) {
            subscriptions.push(iced::time::every(Duration::from_millis(25)).map(|_| Message::Poll));
        }
        Subscription::batch(subscriptions)
    }

    fn view(&self) -> Element<'_, Message> {
        let idle = !self.session.busy() && !self.session.recording() && !self.recording.pending();
        let editable = idle
            && matches!(self.session.connection(), Connection::Connected { .. })
            && self.session.keymap().status() == &Status::Ready;
        let toolbar = row![
            button("Assignments").on_press_maybe(idle.then_some(Message::Page(Page::Keys))),
            button("Macros").on_press_maybe(
                (idle && self.session.macros().is_some()).then_some(Message::Page(Page::Macros))
            ),
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
        } else if self.session.busy() {
            "Working…"
        } else {
            &self.notice
        };
        let base: Element<'_, Message> = container(
            column![
                text(&self.session.descriptor().device_name).size(self.style.type_scale.page_title),
                toolbar,
                text(status),
                self.feature_view(editable),
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

    fn feature_view(&self, editable: bool) -> Element<'_, Message> {
        if (self.session.recording() || self.recording.pending())
            && let Some(editor) = self.session.macros()
        {
            let program = editor.draft();
            let phase = if self.recording.pending() {
                view::recording::Phase::Waiting
            } else {
                view::recording::Phase::Recording {
                    events: program.map_or(0, |program| program.events.len()),
                }
            };
            let mut content = column![
                view::keymap::workspace(
                    &self.keys,
                    self.session.descriptor(),
                    self.session.keymap(),
                    false,
                    &self.style
                )
                .map(Message::Keys),
                view::recording::controls(self.recording.options(), phase, &self.style)
                    .map(Message::Record),
            ]
            .spacing(self.style.spacing.m);
            if let Some(program) = program {
                content = content
                    .push(view::recording::preview(program, &self.style).map(Message::Record));
            }
            return content.height(Fill).into();
        }
        match self.page {
            Page::Keys => view::keymap::view(
                &self.keys,
                self.session.descriptor(),
                self.session.keymap(),
                editable,
                &self.style,
            )
            .map(Message::Keys),
            Page::Macros => match (self.session.macros(), self.session.macro_library()) {
                (Some(editor), Some(library)) => column![
                    view::keymap::workspace(
                        &self.keys,
                        self.session.descriptor(),
                        self.session.keymap(),
                        true,
                        &self.style
                    )
                    .map(Message::Keys),
                    view::recording::controls(
                        self.recording.options(),
                        view::recording::Phase::Idle {
                            editable: !self.session.busy()
                                && editor.status() == &Status::Ready
                                && editor.draft().is_some_and(|program| editor
                                    .capabilities()
                                    .editable_repeat_counts
                                    .contains(&program.repeat_count)),
                        },
                        &self.style
                    )
                    .map(Message::Record),
                    view::macros::view(
                        &self.macros,
                        editor,
                        library,
                        !self.session.busy(),
                        self.keys.target(),
                        self.session.catalog_scanning(),
                        &self.style
                    )
                    .map(Message::Macros),
                ]
                .spacing(self.style.spacing.m)
                .height(Fill)
                .into(),
                _ => text("Macros are unavailable for this keyboard.").into(),
            },
        }
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
