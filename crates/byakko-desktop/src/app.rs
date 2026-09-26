//! Window lifecycle and effect delivery. Feature policy stays in core.
use crate::{
    config::Config,
    controller::autosave::{Autosave, Feature as AutoFeature},
    controller::connection::{Attach, Connection as Link, RefreshStep},
    controller::discovery::Discovery,
    controller::files::{Accepted as FileAccepted, Files},
    controller::host::{Controller as Host, Outcome as HostOutcome},
    controller::recording::Controller as Recording,
    form::{
        application::{Closing, Message, Page},
        catalog, files, host, keymap, lighting, macros, picture, recording, settings,
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
use iced::{Element, Subscription, Task, window};
use std::{
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

struct App {
    session: Session,
    keys: keymap::Form,
    macros: macros::Form,
    recording: Recording,
    lighting: lighting::Form,
    host: Host,
    host_form: host::Form,
    picture: picture::Form,
    settings: settings::Form,
    autosave: Autosave,
    files: Files,
    assignment_binding: Option<(String, String)>,
    config: Config,
    page: Page,
    link: Link,
    closing: Closing,
    notice: String,
    style: UiStyle,
}

pub fn run(
    session: Session,
    config: Config,
    discovery: Discovery,
    attach: impl Fn(Option<&str>) -> Result<(String, byakko_devices::Executor), String> + 'static,
) -> iced::Result {
    let mut app = App::new(session, Box::new(attach));
    app.link.monitor(discovery);
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
                Task::done(Message::Scan),
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
            host: Host::default(),
            host_form: host::Form::default(),
            picture: picture::Form::default(),
            settings: settings::Form::default(),
            autosave: Autosave::default(),
            config: Config::default(),
            page: Page::Keys,
            session,
            link: Link::new(attach),
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
        if (self.host.busy() || !self.session.host().is_idle())
            && !matches!(
                message,
                Message::Host(host::Message::Stop | host::Message::Displays(_))
                    | Message::HostFocusLost
                    | Message::Poll(_)
                    | Message::Close
            )
        {
            return Task::none();
        }
        match message {
            Message::Host(message) => return self.update_host(message),
            Message::HostFocusLost => {
                let outcome = self
                    .host
                    .stop(&mut self.session, self.link.executor(), None);
                self.host_notice(outcome);
            }
            Message::Files(message) => return self.update_files(message),
            Message::FileComplete(completion) => return self.complete_file(completion),
            Message::Record(message) => {
                let was_recording = self.session.recording();
                let result =
                    self.recording
                        .update(message, &mut self.session, self.link.executor());
                self.recording_notice(result, was_recording);
            }
            Message::RecordingInput(event, at) => {
                let was_recording = self.session.recording();
                let result = self.recording.input(&event, at, &mut self.session);
                self.recording_notice(result, was_recording);
            }
            Message::Page(page) => {
                self.keys.catalog.input = catalog::InputMode::Browse;
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
            Message::Keys(message) => match self.keys.update(message, self.session.keymap()) {
                Ok(Some(keymap::Intent::Edit(change))) => match self.session.edit(change) {
                    Ok(()) => {
                        self.keys.sync_shortcut(self.session.keymap());
                        self.notice.clear();
                    }
                    Err(reason) => self.notice = reason,
                },
                Ok(Some(keymap::Intent::Scroll(scroll))) => {
                    let task = match scroll {
                        catalog::Scroll::Top => crate::widget::catalog::top(),
                        catalog::Scroll::Section(category) => {
                            crate::widget::catalog::jump(category)
                        }
                        catalog::Scroll::Measure => crate::widget::catalog::visible(),
                    };
                    return task.map(|message| Message::Keys(keymap::Message::Catalog(message)));
                }
                Ok(None) => {}
                Err(reason) => self.notice = reason,
            },
            Message::Read if !self.session.busy() => {
                self.keys.catalog.input = catalog::InputMode::Browse;
                self.autosave.clear();
                self.assignment_binding = None;
                let request = self.link.read(&mut self.session);
                return self.submit(request);
            }
            Message::Save => {
                let request = self.session.save();
                return self.submit(request);
            }
            Message::Revert => {
                self.notice = self.session.revert().err().unwrap_or_default();
                self.keys.sync_shortcut(self.session.keymap());
            }
            Message::Poll(_) => return self.poll(),
            Message::Scan => return self.scan(),
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
            self.link.invalidate_discovery();
            self.page = Page::Macros;
        } else if was_recording {
            self.macros
                .sync(self.session.macros().and_then(|editor| editor.draft()));
        }
    }

    fn load_page(&mut self) -> Task<Message> {
        if self.session.busy()
            || self.host.busy()
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
            || self.host.busy()
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
                self.keys.catalog.input = catalog::InputMode::Browse;
                self.link.invalidate_discovery();
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

    fn host_notice(&mut self, outcome: HostOutcome) {
        match outcome {
            HostOutcome::None => {}
            HostOutcome::Notice(notice) => self.notice = notice,
            HostOutcome::Finished => {
                self.notice = "Onboard lighting restored and read back.".into()
            }
            HostOutcome::Failed(reason) => {
                self.notice = reason;
                self.closing = Closing::Open;
            }
        }
    }

    fn update_host(&mut self, message: host::Message) -> Task<Message> {
        match message {
            host::Message::Start => {
                if self.autosave.pending() || self.recording.pending() {
                    self.notice = "Finish the current edits before starting host lighting.".into();
                    return Task::none();
                }
                let result = self
                    .link
                    .executor()
                    .ok_or("Read the keyboard before starting host lighting".to_owned())
                    .and_then(|worker| self.host.start(&self.host_form, &mut self.session, worker));
                match result {
                    Ok(()) => {
                        self.keys.catalog.input = catalog::InputMode::Browse;
                        self.link.invalidate_discovery();
                        self.page = Page::Lighting;
                        self.notice = "Preparing host source…".into();
                    }
                    Err(reason) => self.notice = reason,
                }
            }
            host::Message::Stop => {
                let outcome = self
                    .host
                    .stop(&mut self.session, self.link.executor(), None);
                self.host_notice(outcome);
            }
            host::Message::RefreshDisplays => {
                if matches!(self.host_form.displays, host::Displays::Loading) {
                    return Task::none();
                }
                self.host_form.displays = host::Displays::Loading;
                let (sender, receiver) = iced::futures::channel::oneshot::channel();
                if let Err(error) = std::thread::Builder::new()
                    .name("byakko-displays".into())
                    .spawn(move || {
                        let _ = sender.send(byakko_devices::screen_sample::displays());
                    })
                {
                    self.host_form.displays = host::Displays::Failed(error.to_string());
                    return Task::none();
                }
                return Task::perform(
                    async move {
                        receiver
                            .await
                            .unwrap_or_else(|_| Err("Display discovery stopped".into()))
                    },
                    |result| Message::Host(host::Message::Displays(result)),
                );
            }
            message => {
                if let Some(editor) = self.session.lighting() {
                    self.notice = self
                        .host_form
                        .update(message, editor.capabilities())
                        .err()
                        .unwrap_or_default();
                }
            }
        }
        Task::none()
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
            Message::Save => self
                .session
                .macros()
                .ok_or_else(|| "Macros are not available".to_owned())
                .and_then(|editor| self.macros.validate_repeat(editor))
                .and_then(|()| self.session.save_macro()),
            Message::Assign(binding) => match self.keys.target() {
                Some((layer, key)) => {
                    let request = self
                        .session
                        .macros()
                        .ok_or_else(|| "Macros are not available".to_owned())
                        .and_then(|editor| self.macros.validate_repeat(editor))
                        .and_then(|()| self.session.save_and_assign_macro(layer, key, &binding));
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
                match self.session.revert_macro() {
                    Ok(()) => {
                        self.macros
                            .sync(self.session.macros().and_then(|editor| editor.draft()));
                        self.notice.clear();
                    }
                    Err(reason) => self.notice = reason,
                }
                return Task::none();
            }
            message => {
                if self.session.busy() {
                    return Task::none();
                }
                let edit = self
                    .session
                    .macros()
                    .ok_or_else(|| "Macros are not available".to_owned())
                    .and_then(|editor| self.macros.update(message, editor));
                match edit {
                    Ok(Some(edit)) => {
                        let result = self.session.edit_macro(edit.clone());
                        if result.is_ok() {
                            self.macros.accepted(&edit);
                        }
                        self.notice = result.err().unwrap_or_default();
                    }
                    Ok(None) => self.notice.clear(),
                    Err(reason) => self.notice = reason,
                }
                return Task::none();
            }
        };
        self.submit(request)
    }

    fn scan(&mut self) -> Task<Message> {
        if self.host.busy()
            || self.session.busy()
            || self.session.catalog_scanning()
            || self.autosave.pending()
        {
            return Task::none();
        }
        let request = self.link.scan(&mut self.session);
        if !matches!(self.session.connection(), Connection::Connected { .. }) {
            self.keys.catalog.input = catalog::InputMode::Browse;
        }
        match request {
            Ok(Some(command)) => self.submit(Ok(command)),
            Ok(None) => Task::none(),
            Err(reason) => {
                self.notice = reason;
                Task::none()
            }
        }
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
        self.keys.catalog.input = catalog::InputMode::Browse;
        self.link.invalidate_discovery();
        match self.link.executor() {
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
        let outcome = self.host.poll(&mut self.session, self.link.executor());
        let finished = matches!(outcome, HostOutcome::Finished);
        self.host_notice(outcome);
        if finished && self.closing == Closing::Waiting {
            return self.close();
        }
        let Some(worker) = self.link.executor() else {
            return Task::none();
        };
        match worker.try_receive() {
            Ok(completion) => self.complete(completion),
            Err(TryRecvError::Empty) => Task::none(),
            Err(TryRecvError::Disconnected) => {
                self.keys.catalog.input = catalog::InputMode::Browse;
                self.autosave.clear();
                self.assignment_binding = None;
                if self.recording.pending() {
                    let _ = self.recording.finish(&mut self.session, Instant::now());
                }
                let failure = self.link.lost(&mut self.session).err();
                self.closing = Closing::Open;
                self.notice = failure.unwrap_or_else(|| {
                    "Connection lost. Edits are retained; reconnect and read before saving.".into()
                });
                Task::none()
            }
        }
    }

    fn complete(&mut self, completion: Completion) -> Task<Message> {
        // Compare only around an explicit read; the editor remains the program owner.
        let macro_before = match &completion.payload {
            CompletionPayload::Macro {
                result: byakko_core::contract::FeatureResult::Read(_),
                ..
            } => self
                .session
                .macros()
                .and_then(|editor| editor.draft())
                .cloned(),
            _ => None,
        };
        let feature = match &completion.payload {
            CompletionPayload::Lighting(_) => Some(AutoFeature::Lighting),
            CompletionPayload::Picture(_) => Some(AutoFeature::Picture),
            CompletionPayload::Settings(_) => Some(AutoFeature::Settings),
            _ => None,
        };
        let outcome = self.session.accept(completion);
        let refresh = self.link.advance(&mut self.session, &outcome);
        if matches!(outcome, Outcome::Failed(_) | Outcome::Conflict)
            && let Some(feature) = feature
        {
            self.autosave.cancel(feature);
        }
        match outcome {
            Outcome::Ignored => return Task::none(),
            Outcome::Loaded => {
                self.keys.sync_shortcut(self.session.keymap());
                self.notice = "Keymap loaded.".into();
            }
            Outcome::Saved => {
                self.keys.sync_shortcut(self.session.keymap());
                self.notice = "Assignments saved and read back.".into();
            }
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
                let draft = self.session.macros().and_then(|editor| editor.draft());
                if macro_before.as_ref() != draft {
                    self.macros.sync(draft);
                }
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
        if let RefreshStep::Command(request) = refresh {
            self.submit(request.map(|command| *command))
        } else if self.closing == Closing::Waiting {
            self.close()
        } else if matches!(refresh, RefreshStep::Finished) {
            Task::none()
        } else {
            self.load_page()
        }
    }

    fn close(&mut self) -> Task<Message> {
        self.keys.catalog.input = catalog::InputMode::Browse;
        self.link.invalidate_discovery();
        if self.host.busy() || !self.session.host().is_idle() {
            let outcome = self
                .host
                .stop(&mut self.session, self.link.executor(), None);
            self.host_notice(outcome);
            if self.host.busy() || !self.session.host().is_idle() {
                self.closing = Closing::Waiting;
                return Task::none();
            }
        }
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
        if self.host.busy() {
            subscriptions.push(iced::event::listen_with(|event, _, _| {
                matches!(event, iced::Event::Window(window::Event::Unfocused))
                    .then_some(Message::HostFocusLost)
            }));
        }
        if self.page == Page::Keys
            && self.keys.catalog.input == catalog::InputMode::Capture
            && !self.session.busy()
            && !self.files.busy()
            && !self.session.recording()
            && !self.recording.pending()
            && !self.host.busy()
        {
            subscriptions.push(iced::event::listen_with(|event, _, _| {
                input::catalog::capture(event)
                    .map(|message| Message::Keys(keymap::Message::Catalog(message)))
            }));
        }
        if self.session.recording() || self.recording.pending() {
            subscriptions.push(iced::event::listen_with(|event, status, _| {
                input::recording::captures(&event, status)
                    .then(|| Message::RecordingInput(event, Instant::now()))
            }));
        }
        if !self.session.recording()
            && (self.host.busy()
                || self.session.busy()
                || self.session.catalog_scanning()
                || self.autosave.pending())
        {
            subscriptions.push(iced::time::every(Duration::from_millis(25)).map(Message::Poll));
        }
        if self.closing == Closing::Open
            && self.link.monitoring()
            && !self.session.recording()
            && !self.recording.pending()
            && !self.session.busy()
            && !self.session.catalog_scanning()
            && !self.files.busy()
            && !self.autosave.pending()
            && !self.host.busy()
        {
            let cadence = if self.link.awaiting_discovery() {
                Duration::from_millis(100)
            } else {
                Duration::from_secs(2)
            };
            subscriptions.push(iced::time::every(cadence).map(|_| Message::Scan));
        }
        Subscription::batch(subscriptions)
    }

    fn view(&self) -> Element<'_, Message> {
        view::application::view(view::application::View {
            session: &self.session,
            presence: self.link.presence(),
            keys: &self.keys,
            macros: &self.macros,
            lighting: &self.lighting,
            host: &self.host_form,
            host_preparing: self.host.preparing(),
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
