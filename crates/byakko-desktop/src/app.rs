//! Window lifecycle and effect delivery. Feature policy stays in core.
use crate::{
    config::Config,
    controller::autosave::{Autosave, Feature as AutoFeature},
    controller::files::{Accepted as FileAccepted, Files},
    controller::recording::Controller as Recording,
    form::{
        application::{Closing, Message, Page},
        files, keymap, lighting, macros, picture, recording, settings,
    },
    input, view,
    widget::panels::UiStyle,
};
use byakko_core::{
    contract::{Command, Completion, CompletionPayload, Problem},
    editor::Status,
    session::{Connection, Outcome, Session},
    workflow::Problem as WorkflowProblem,
};
use byakko_devices::Executor;
use iced::{Element, Subscription, Task, window};
use std::{
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

type Attach = dyn Fn(Option<&str>) -> Result<(String, Executor), String>;

struct App {
    session: Session,
    keys: keymap::Form,
    macros: macros::Form,
    recording: Recording,
    lighting: lighting::Form,
    picture: picture::Form,
    settings: settings::Form,
    autosave: Autosave,
    files: Files,
    assignment_binding: Option<(String, String)>,
    config: Config,
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
    config: Config,
    attach: impl Fn(Option<&str>) -> Result<(String, Executor), String> + 'static,
) -> iced::Result {
    let mut app = App::new(session, Box::new(attach));
    app.files = Files::new(&app.session, config.data_directory.clone());
    app.config = config;
    let initial = std::cell::RefCell::new(Some(app));
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
            files: Files::new(&session, None),
            assignment_binding: None,
            keys: keymap::Form::new(session.descriptor()),
            macros: macros::Form::default(),
            recording: Recording::default(),
            lighting: lighting::Form::default(),
            picture: picture::Form::default(),
            settings: settings::Form::default(),
            autosave: Autosave::default(),
            config: Config::default(),
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
        let now = match &message {
            Message::Poll(at) => *at,
            _ => Instant::now(),
        };
        let task = self.reduce(message);
        Task::batch([task, self.flush_saves(now, false)])
    }

    fn reduce(&mut self, message: Message) -> Task<Message> {
        if self.closing != Closing::Open
            && !matches!(
                message,
                Message::Poll(_)
                    | Message::Close
                    | Message::Discard
                    | Message::KeepEditing
                    | Message::FileComplete(_)
            )
        {
            return Task::none();
        }
        if self.files.busy()
            && !matches!(
                message,
                Message::FileComplete(_) | Message::Poll(_) | Message::Close
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
                    | Message::Poll(_)
            )
        {
            return Task::none();
        }
        match message {
            Message::Files(message) => return self.update_files(message),
            Message::FileComplete(completion) => return self.complete_file(completion),
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
            Message::Page(page) => {
                self.page = page;
                if page == Page::Picture
                    && self.session.lighting().is_some_and(|editor| editor.dirty())
                {
                    self.autosave.cancel(AutoFeature::Lighting);
                    let request = self.session.save_lighting();
                    return self.submit(request);
                }
                return self.load_page();
            }
            Message::Lighting(message) => return self.update_lighting(message),
            Message::Picture(message) => return self.update_picture(message),
            Message::Settings(message) => return self.update_settings(message),
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
            Message::Poll(_) => return self.poll(),
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

    fn load_page(&mut self) -> Task<Message> {
        if self.session.busy()
            || self.files.busy()
            || !matches!(self.session.connection(), Connection::Connected { .. })
        {
            return Task::none();
        }
        let request = match self.page {
            Page::Macros if self.files.needs_labels() => {
                return self.begin_file(files::Operation::LoadLabels);
            }
            Page::Lighting if self.session.lighting().is_some_and(needs_read) => {
                Some(self.session.read_lighting())
            }
            Page::Picture => match self.session.prepare_picture() {
                Ok(Some(command)) => Some(Ok(command)),
                Ok(None) => None,
                Err(reason) => Some(Err(reason)),
            },
            Page::Settings if self.session.settings().is_some_and(needs_read) => {
                Some(self.session.read_settings())
            }
            _ => None,
        };
        request.map_or_else(Task::none, |request| self.submit(request))
    }

    fn flush_saves(&mut self, now: Instant, flush: bool) -> Task<Message> {
        if self.session.busy()
            || self.files.busy()
            || self.session.recording()
            || self.recording.pending()
            || self.closing == Closing::ConfirmDiscard
        {
            return Task::none();
        }
        if !flush && (self.lighting.dragging() || self.picture.dragging()) {
            return Task::none();
        }
        while let Some(feature) = self.autosave.take_due(now, flush) {
            let dirty = match feature {
                AutoFeature::Lighting => {
                    self.session.lighting().is_some_and(|editor| editor.dirty())
                }
                AutoFeature::Picture => self.session.picture().is_some_and(|editor| editor.dirty()),
                AutoFeature::Settings => {
                    self.session.settings().is_some_and(|editor| editor.dirty())
                }
            };
            if !dirty {
                continue;
            }
            let request = match feature {
                AutoFeature::Lighting => self.session.save_lighting(),
                AutoFeature::Picture => self.session.save_picture(),
                AutoFeature::Settings => self.session.save_settings(),
            };
            if request.is_err() {
                self.closing = Closing::Open;
            }
            return self.submit(request);
        }
        if self.closing == Closing::Waiting && !self.autosave.pending() {
            self.close()
        } else {
            Task::none()
        }
    }

    fn edited(&mut self, feature: AutoFeature, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.autosave.edited(feature, Instant::now(), &self.config);
                self.notice = "Changes queued for automatic apply.".into();
            }
            Err(reason) => self.notice = reason,
        }
    }

    fn update_files(&mut self, message: files::Message) -> Task<Message> {
        if self.session.busy() {
            return Task::none();
        }
        match message {
            files::Message::MacroPath(path) => self.files.form.macro_path = path,
            files::Message::ArchivePath(path) => self.files.form.archive_path = path,
            files::Message::Name(name) => {
                if let Some(editor) = self.session.macros() {
                    self.files.form.rename(editor.slot(), name);
                }
            }
            files::Message::Begin(operation) => return self.begin_file(operation),
            files::Message::Capture => {
                let request = self.session.capture_archive();
                return self.submit(request);
            }
        }
        Task::none()
    }

    fn begin_file(&mut self, operation: files::Operation) -> Task<Message> {
        match self.files.begin(operation, &self.session) {
            Ok(job) => {
                self.notice.clear();
                job.task().map(Message::FileComplete)
            }
            Err(reason) => {
                self.notice = reason;
                Task::none()
            }
        }
    }

    fn complete_file(&mut self, completion: crate::controller::files::Completion) -> Task<Message> {
        match self.files.accept(completion, &mut self.session) {
            None => return Task::none(),
            Some(Err(reason)) => {
                self.closing = Closing::Open;
                self.notice = reason;
                return Task::none();
            }
            Some(Ok(FileAccepted::Imported(notice))) => {
                self.notice = notice;
                self.macros
                    .sync(self.session.macros().and_then(|editor| editor.draft()));
            }
            Some(Ok(FileAccepted::Finished(notice))) => self.notice = notice,
        }
        if self.closing == Closing::Waiting {
            self.close()
        } else {
            Task::none()
        }
    }

    fn update_lighting(&mut self, message: lighting::Message) -> Task<Message> {
        let request = match message {
            lighting::Message::Read => {
                self.autosave.cancel(AutoFeature::Lighting);
                self.session.read_lighting()
            }
            lighting::Message::Save => {
                self.autosave.cancel(AutoFeature::Lighting);
                self.session.save_lighting()
            }
            lighting::Message::Revert => {
                if let Err(reason) = self.session.revert_lighting() {
                    self.notice = reason;
                } else {
                    self.autosave.cancel(AutoFeature::Lighting);
                }
                return Task::none();
            }
            message => {
                if let Some(edit) = self.lighting.update(message) {
                    let result = self.session.edit_lighting(edit);
                    self.edited(AutoFeature::Lighting, result);
                }
                return Task::none();
            }
        };
        self.submit(request)
    }

    fn update_picture(&mut self, message: picture::Message) -> Task<Message> {
        let request = match message {
            picture::Message::Read => {
                self.autosave.cancel(AutoFeature::Picture);
                self.session.read_picture()
            }
            picture::Message::Save => {
                self.autosave.cancel(AutoFeature::Picture);
                self.session.save_picture()
            }
            picture::Message::Revert => {
                if let Err(reason) = self.session.revert_picture() {
                    self.notice = reason;
                } else {
                    self.autosave.cancel(AutoFeature::Picture);
                }
                return Task::none();
            }
            message => {
                if let Some(edit) = self
                    .session
                    .picture()
                    .and_then(|editor| self.picture.update(message, editor))
                {
                    let key = match &edit {
                        byakko_core::model::picture::Edit::Color { key, .. }
                        | byakko_core::model::picture::Edit::Channel { key, .. } => key.clone(),
                    };
                    let result = self.session.edit_picture(edit);
                    if result.is_ok() {
                        self.picture
                            .accepted(&key, self.session.picture().expect("accepted picture edit"));
                    }
                    self.edited(AutoFeature::Picture, result);
                }
                return Task::none();
            }
        };
        self.submit(request)
    }

    fn update_settings(&mut self, message: settings::Message) -> Task<Message> {
        let request = match message {
            settings::Message::Read => {
                self.autosave.cancel(AutoFeature::Settings);
                self.session.read_settings()
            }
            settings::Message::Save => {
                self.autosave.cancel(AutoFeature::Settings);
                self.session.save_settings()
            }
            settings::Message::Revert => {
                if let Err(reason) = self.session.revert_settings() {
                    self.notice = reason;
                } else {
                    self.autosave.cancel(AutoFeature::Settings);
                    self.settings.clear();
                }
                return Task::none();
            }
            message => {
                let edit = self
                    .session
                    .settings()
                    .ok_or_else(|| "Settings are unavailable".to_owned())
                    .and_then(|editor| self.settings.update(message, editor));
                match edit {
                    Ok(Some(edit)) => {
                        let id = edit.id.clone();
                        let result = self.session.edit_settings(edit);
                        if result.is_ok() {
                            self.settings.accepted(&id);
                        }
                        self.edited(AutoFeature::Settings, result);
                    }
                    Err(reason) => self.notice = reason,
                    Ok(None) => {}
                }
                return Task::none();
            }
        };
        self.submit(request)
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
                Some((layer, key)) => {
                    let request = self.session.save_and_assign_macro(layer, key, &binding);
                    if request.is_ok()
                        && let Some(editor) = self.session.macros()
                    {
                        self.assignment_binding = Some((editor.slot().to_owned(), binding));
                    }
                    request
                }
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
                self.autosave.clear();
                self.assignment_binding = None;
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
        let feature = match &completion.payload {
            CompletionPayload::Lighting(_) => Some(AutoFeature::Lighting),
            CompletionPayload::Picture(_) => Some(AutoFeature::Picture),
            CompletionPayload::Settings(_) => Some(AutoFeature::Settings),
            _ => None,
        };
        let outcome = self.session.accept(completion);
        if matches!(outcome, Outcome::Failed(_) | Outcome::Conflict)
            && let Some(feature) = feature
        {
            self.autosave.cancel(feature);
        }
        match outcome {
            Outcome::Ignored => return Task::none(),
            Outcome::Loaded => {
                self.notice = "Keymap loaded.".into();
            }
            Outcome::Saved => self.notice = "Assignments saved and read back.".into(),
            Outcome::LightingLoaded => self.notice = "Lighting loaded.".into(),
            Outcome::LightingSaved => self.notice = "Lighting applied.".into(),
            Outcome::PictureLoaded => self.notice = "Key colors loaded.".into(),
            Outcome::PictureSaved => self.notice = "Key colors applied.".into(),
            Outcome::SettingsLoaded => self.notice = "Settings loaded.".into(),
            Outcome::SettingsSaved => self.notice = "Setting saved and read back.".into(),
            Outcome::ArchiveCaptured => self.notice = "Diagnostic archive captured.".into(),
            Outcome::ArchiveCaptureFailed(problem) => {
                self.closing = Closing::Open;
                self.notice = match problem {
                    byakko_core::library::archive::CaptureProblem::Capture(reason)
                    | byakko_core::library::archive::CaptureProblem::InvalidResult(reason) => {
                        reason
                    }
                };
                return Task::none();
            }
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
                if let Some((slot, binding)) = self.assignment_binding.take() {
                    self.files.form.bindings.insert(slot, binding);
                }
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
                self.assignment_binding = None;
                self.closing = Closing::Open;
                let reason = match problem {
                    WorkflowProblem::Device(problem) => problem_text(&problem),
                    WorkflowProblem::Validation(reason) => reason,
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
            Outcome::PicturePreparationFailed {
                lighting_applied,
                problem,
            } => {
                self.closing = Closing::Open;
                let reason = match problem {
                    WorkflowProblem::Device(problem) => problem_text(&problem),
                    WorkflowProblem::Validation(reason) => reason,
                };
                self.notice = format!(
                    "{} {reason}",
                    if lighting_applied {
                        "Per-key lighting applied; loading its colors failed."
                    } else {
                        "Could not prepare per-key lighting."
                    }
                );
                return Task::none();
            }
            Outcome::Conflict => {
                self.assignment_binding = None;
                self.closing = Closing::Open;
                self.notice =
                    "The observed feature differs from your edit baseline. Edits are retained."
                        .into();
                return Task::none();
            }
            Outcome::Failed(problem) => {
                self.assignment_binding = None;
                self.closing = Closing::Open;
                self.notice = problem_text(&problem);
                return Task::none();
            }
        }
        if self.closing == Closing::Waiting {
            self.close()
        } else {
            self.load_page()
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
        if self.session.busy() || self.files.busy() {
            self.closing = Closing::Waiting;
        } else if self.autosave.pending() {
            self.closing = Closing::Waiting;
            return self.flush_saves(Instant::now(), true);
        } else if self.session.keymap().dirty()
            || self.session.macros().is_some_and(|editor| editor.dirty())
            || self.session.lighting().is_some_and(|editor| editor.dirty())
            || self.session.picture().is_some_and(|editor| editor.dirty())
            || self.session.settings().is_some_and(|editor| editor.dirty())
            || self.settings.has_input()
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
        if !self.session.recording()
            && (self.session.busy() || self.session.catalog_scanning() || self.autosave.pending())
        {
            subscriptions.push(iced::time::every(Duration::from_millis(25)).map(Message::Poll));
        }
        Subscription::batch(subscriptions)
    }

    fn view(&self) -> Element<'_, Message> {
        view::application::view(view::application::View {
            session: &self.session,
            keys: &self.keys,
            macros: &self.macros,
            lighting: &self.lighting,
            picture: &self.picture,
            settings: &self.settings,
            files: &self.files.form,
            files_busy: self.files.busy(),
            names_available: self.files.can_save_labels(),
            recording_options: self.recording.options(),
            recording_pending: self.recording.pending(),
            page: self.page,
            closing: self.closing,
            notice: &self.notice,
            style: &self.style,
        })
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
            "The save result did not match the submitted changes.".into()
        }
    }
}

fn needs_read<F: byakko_core::editor::Feature>(editor: &byakko_core::editor::Editor<F>) -> bool {
    editor.status() == &Status::Unloaded
        || (!editor.dirty()
            && matches!(
                editor.status(),
                Status::Unverified {
                    problem: Problem::ReadRequired
                }
            ))
}

#[cfg(test)]
mod tests;
