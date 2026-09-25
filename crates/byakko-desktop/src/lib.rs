//! Desktop adapter. Domain decisions remain in core; firmware lives outside views.
use byakko_core::session::CompletionPayload;
use byakko_core::session::{DeviceActivity, Feature};
mod action_catalog;
mod archive;
mod audio_stream;
mod clipped_dropdown;
mod color_picker;
mod control_widgets;
pub mod discovery;
mod lighting;
mod macro_assignment;
mod macro_binding_view;
mod macro_editor;
mod macro_files;
mod macro_form;
mod macro_view;
mod panels;
mod physical_board;
mod picture;
mod recording;
mod recording_input;
mod screen_stream;
mod settings;
mod shortcut;
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
    Shortcut(shortcut::Message),
    File(macro_files::Message),
    Record(recording::Message),
    Page(Page),
    Macro(macro_editor::Message),
    SelectLayer(String),
    SelectKey(String),
    Catalog(action_catalog::Message),
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

type Attach = dyn Fn(Option<&str>) -> Result<(String, Executor), String>;

pub mod config;

struct Desktop {
    config: config::Config,
    live_settings: settings::Pending,
    picker_gesture: color_picker::Gesture,
    brush_color: Option<[u8; 3]>,
    ui: panels::UiStyle,
    archive_file: archive::FileState,
    archive_path: String,
    picture_selected: Option<String>,
    macro_files: macro_files::Fields,
    macro_new_slot: Option<String>,
    macro_composer: macro_view::Composer,
    macro_binding_choice: Option<(String, String)>,
    macro_assignment: Option<macro_assignment::Pending>,
    macro_notice: Option<String>,
    clock: std::time::Instant,
    recording_options: recording::Options,
    host: Option<lighting::HostInput>,
    screen_capture: lighting::screen::Controls,
    lighting_panel: lighting::Panel,
    initial_reads: std::collections::VecDeque<Page>,
    picture_activation: Option<(u64, u64)>,
    live_lighting: lighting::live::Pending,
    live_picture: picture::Pending,
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
    action_browser: action_catalog::Browser,
    shortcut: shortcut::Form,
    notice: Option<String>,
    closing: Closing,
}

/// The composition root supplies a session and a factory for one executor per connection.
/// `attach(Some(id))` must match the discovered identity; `attach(None)` explicitly
/// selects the current unique device. Both return the newly pinned identity.
pub fn run(
    session: Session,
    probe: impl Fn() -> Availability + Send + 'static,
    attach: impl Fn(Option<&str>) -> Result<(String, Executor), String> + 'static,
    labels_directory: Option<std::path::PathBuf>,
    config: config::Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let layer = session.descriptor().layers[0].id.clone();
    let mut discovery = Discovery::spawn(probe)?;
    discovery.request();
    // Iced's boot closure is reusable; this native session has exactly one owner.
    let initial = std::cell::RefCell::new(Some(Desktop {
        config,
        live_settings: Default::default(),
        picker_gesture: Default::default(),
        brush_color: None,
        ui: panels::UiStyle::DEFAULT,
        archive_file: archive::FileState::Idle,
        archive_path: String::new(),
        picture_selected: None,
        macro_files: macro_files::Fields::with_labels_directory(labels_directory),
        macro_new_slot: None,
        macro_composer: macro_view::Composer::default(),
        macro_binding_choice: None,
        macro_assignment: None,
        macro_notice: None,
        clock: std::time::Instant::now(),
        recording_options: Default::default(),
        host: None,
        screen_capture: Default::default(),
        lighting_panel: Default::default(),
        initial_reads: Default::default(),
        picture_activation: None,
        live_lighting: Default::default(),
        live_picture: Default::default(),
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
        action_browser: Default::default(),
        shortcut: shortcut::Form::default(),
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
        let mut attached = false;
        if self.executor.is_none() {
            let Some(Availability::Ready { id }) = &self.presence else {
                self.notice = Some("Waiting for one connected keyboard".into());
                return;
            };
            match (self.attach)(Some(id)) {
                Ok((id, executor)) => {
                    self.executor = Some(executor);
                    self.selected_device = Some(id);
                    attached = true;
                }
                Err(reason) => {
                    self.notice = Some(format!("Could not open keyboard: {reason}"));
                    return;
                }
            }
        }
        let request = self
            .connect_session()
            .and_then(|()| self.session.request_read());
        self.submit(request);
        if attached {
            self.load_macro_labels();
        }
    }

    fn load_macro_labels(&mut self) {
        if let Some(editor) = self.session.macros()
            && let Err(reason) = self.macro_files.load_labels(editor)
            && self.notice.is_none()
        {
            self.notice = Some(format!("Local labels could not be loaded: {reason}"));
        }
    }

    fn connect_session(&mut self) -> Result<(), String> {
        let generation = self.session.connect()?;
        self.executor
            .as_ref()
            .expect("attached before connecting")
            .set_generation(generation);
        self.initial_reads = [Page::Lighting, Page::Settings, Page::Picture].into();
        Ok(())
    }

    /// A deliberate reconnect/retry selects the current unique collection afresh.
    /// Individual transactions and recovery keep their immutable executor target.
    fn refresh_connection(&mut self) -> bool {
        if self.busy() {
            return false;
        }
        self.macro_assignment = None;
        self.hold_reconnect_if_cautious();
        if let Some(executor) = self.executor.take() {
            executor.cancel_macro_catalog();
            executor.set_generation(0);
        }
        self.discovery.invalidate();
        self.session.disconnect();
        self.selected_device = None;
        self.initial_reads.clear();
        match (self.attach)(None) {
            Ok((id, executor)) => {
                self.presence = Some(Availability::Ready { id: id.clone() });
                self.selected_device = Some(id);
                self.executor = Some(executor);
                match self.connect_session() {
                    Ok(()) => {
                        self.load_macro_labels();
                        true
                    }
                    Err(reason) => {
                        self.notice = Some(reason);
                        false
                    }
                }
            }
            Err(reason) => {
                self.presence = None;
                self.notice = Some(format!("Could not reconnect keyboard: {reason}"));
                false
            }
        }
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
        let Some(choice) = self.session.descriptor().actions.get(index) else {
            return;
        };
        self.stage_action(choice.action.clone());
    }

    fn stage_action(&mut self, action: byakko_core::Action) {
        let Some(key) = self.selected.as_ref() else {
            return;
        };
        self.notice = self
            .session
            .stage(Change {
                layer: self.layer.clone(),
                key: key.clone(),
                action,
            })
            .err();
        if self.notice.is_none() {
            self.sync_shortcut();
        }
    }

    fn sync_shortcut(&mut self) {
        let action = self
            .selected
            .as_ref()
            .and_then(|key| self.session.draft()?.get(&self.layer)?.get(key));
        self.shortcut
            .load(action, self.session.descriptor().shortcuts.as_ref());
    }

    fn update_shortcut(&mut self, message: shortcut::Message) {
        let Some(caps) = self.session.descriptor().shortcuts.as_ref() else {
            return;
        };
        if self.busy() || *self.session.status() != Status::Ready {
            return;
        }
        match message {
            shortcut::Message::Search(query) => {
                self.shortcut.query = query;
                self.shortcut.error = None;
            }
            shortcut::Message::ToggleModifier(usage) => {
                self.shortcut.error = self.shortcut.toggle_modifier(caps, usage).err();
            }
            shortcut::Message::SelectKey(usage) => {
                self.shortcut.error = self.shortcut.select_key(caps, usage).err();
            }
            shortcut::Message::Stage => match self.shortcut.action(caps) {
                Ok(action) => {
                    self.shortcut.error = None;
                    self.stage_action(action);
                }
                Err(reason) => self.shortcut.error = Some(reason),
            },
        }
    }

    fn read_initial_sections(&mut self) {
        if self.busy() {
            return;
        }
        if !self.session.reconnect_cautions().is_empty() {
            return;
        }
        if matches!(
            self.session.status(),
            Status::Unverified {
                problem: Problem::ReadRequired
            }
        ) {
            let request = self.session.request_read();
            self.submit(request);
            return;
        }
        if *self.session.status() != Status::Ready
            || self.live_lighting.has_pending()
            || self.live_picture.has_pending()
            || self.live_settings.has_pending()
        {
            return;
        }
        if self.initial_reads.is_empty() {
            self.initial_reads = [Page::Lighting, Page::Settings, Page::Picture].into();
        }
        while let Some(page) = self.initial_reads.pop_front() {
            let request = match page {
                Page::Lighting
                    if self.session.lighting().is_some_and(|editor| {
                        matches!(
                            editor.status(),
                            byakko_core::lighting::editor::Status::Unloaded
                                | byakko_core::lighting::editor::Status::Unverified {
                                    problem: Problem::ReadRequired
                                }
                        )
                    }) =>
                {
                    self.session.request_lighting_read()
                }
                Page::Picture
                    if self.session.picture().is_some_and(|editor| {
                        matches!(
                            editor.status(),
                            byakko_core::picture::editor::Status::Unloaded
                                | byakko_core::picture::editor::Status::Unverified {
                                    problem: Problem::ReadRequired
                                }
                        )
                    }) =>
                {
                    self.session.request_picture_read()
                }
                Page::Settings
                    if self.session.settings().is_some_and(|editor| {
                        matches!(
                            editor.status(),
                            byakko_core::settings::editor::Status::Unloaded
                                | byakko_core::settings::editor::Status::Unverified {
                                    problem: Problem::ReadRequired
                                }
                        )
                    }) =>
                {
                    self.session.request_settings_read()
                }
                _ => continue,
            };
            self.submit(request);
            return;
        }
        self.read_macro_catalog_in_background();
    }

    fn read_macro_catalog_in_background(&mut self) {
        if !self.busy()
            && !self.session.macro_catalog_scanning()
            && self.session.status() == &Status::Ready
            && self.session.macros().is_some_and(|editor| {
                editor.catalog().is_none() && editor.catalog_error().is_none()
            })
        {
            let request = self.session.request_macro_catalog_read();
            self.submit(request);
        }
    }

    fn poll(&mut self) -> Task<Message> {
        if let Some(task) = self.poll_host() {
            return task;
        }
        self.poll_host_input();
        if !self.busy()
            && (self.flush_live_lighting()
                || self.flush_live_picture()
                || self.flush_live_settings())
        {
            return Task::none();
        }
        let Some(executor) = &self.executor else {
            return Task::none();
        };
        match executor.try_receive() {
            Ok(completion) => return self.complete(completion),
            Err(TryRecvError::Empty) => return Task::none(),
            Err(TryRecvError::Disconnected) => {
                executor.set_generation(0);
                self.cancel_macro_assignment_after_disconnect();
                let caution = self.hold_reconnect_if_cautious();
                let write_in_flight = matches!(
                    self.session.activity(),
                    byakko_core::session::Activity::Device {
                        request: DeviceActivity::Apply(Feature::Keymap),
                        ..
                    } | byakko_core::session::Activity::Device {
                        request: DeviceActivity::Apply(Feature::Macro { .. }),
                        ..
                    } | byakko_core::session::Activity::Device {
                        request: DeviceActivity::Apply(Feature::Lighting),
                        ..
                    } | byakko_core::session::Activity::Device {
                        request: DeviceActivity::Apply(Feature::Picture),
                        ..
                    } | byakko_core::session::Activity::Device {
                        request: DeviceActivity::Apply(Feature::Settings),
                        ..
                    } | byakko_core::session::Activity::Device {
                        request: DeviceActivity::Apply(Feature::Archive),
                        ..
                    }
                );
                if write_in_flight {
                    self.auto_read = AutoRead::ManualOnly;
                }
                self.executor = None;
                self.session.disconnect();
                if !caution {
                    self.notice = Some(
                        "Device worker stopped; device state is unverified. Read again before editing."
                            .into(),
                    );
                }
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

    fn hold_reconnect_if_cautious(&mut self) -> bool {
        let cautions = self.session.reconnect_cautions();
        if cautions.is_empty() {
            return false;
        }
        self.auto_read = AutoRead::ManualOnly;
        let labels = cautions
            .into_iter()
            .map(view::reconnect_caution_label)
            .collect::<Vec<_>>();
        self.notice = Some(format!(
            "{}\nRead manually after reconnecting.",
            labels.join("\n")
        ));
        true
    }

    fn accept_availability(&mut self, availability: Availability) {
        if self.busy() {
            return;
        }
        let previous = self.selected_device.as_deref();
        let changed = matches!(&availability, Availability::Ready { id } if previous.is_some_and(|old| old != id));
        if changed || !matches!(availability, Availability::Ready { .. }) {
            self.cancel_macro_assignment_after_disconnect();
            self.hold_reconnect_if_cautious();
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
        let macro_assignment_completion = self.macro_assignment_completion(&completion);
        let activated_picture =
            matches!(&completion.payload, CompletionPayload::ApplyLighting { .. })
                && self.picture_activation == Some((completion.generation, completion.operation));
        let macro_draft_before_read = match &completion.payload {
            CompletionPayload::ReadMacro { .. } => Some(
                self.session
                    .macros()
                    .and_then(|editor| editor.draft())
                    .cloned(),
            ),
            _ => None,
        };
        let keymap_result = matches!(
            &completion.payload,
            CompletionPayload::Read { .. } | CompletionPayload::Apply { .. }
        );
        let archive_result = matches!(
            &completion.payload,
            CompletionPayload::CaptureArchive { .. }
                | CompletionPayload::ReviewArchive { .. }
                | CompletionPayload::ApplyArchive { .. }
        );
        let lighting_result = matches!(
            &completion.payload,
            CompletionPayload::ReadLighting { .. } | CompletionPayload::ApplyLighting { .. }
        );
        let picture_result = matches!(
            &completion.payload,
            CompletionPayload::ReadPicture { .. } | CompletionPayload::ApplyPicture { .. }
        );
        let settings_result = matches!(
            &completion.payload,
            CompletionPayload::ReadSettings { .. } | CompletionPayload::ApplySetting { .. }
        );
        let macro_result = matches!(
            &completion.payload,
            CompletionPayload::ReadMacro { .. } | CompletionPayload::ApplyMacro { .. }
        );
        let macro_catalog_result = matches!(
            &completion.payload,
            CompletionPayload::ReadMacroCatalog { .. }
        );
        let was_scanning = self.session.macro_catalog_scanning();
        if self.session.accept(completion) == Acceptance::IgnoredStale {
            return Task::none();
        }
        if was_scanning
            && !macro_catalog_result
            && !self.session.macro_catalog_scanning()
            && let Some(executor) = &self.executor
        {
            executor.cancel_macro_catalog();
        }
        if settings_result && let Some(editor) = self.session.settings() {
            self.live_settings.reconcile(editor.status());
        }
        if keymap_result && *self.session.status() == Status::Ready {
            self.sync_shortcut();
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
        } else if macro_catalog_result {
            self.session
                .macros()
                .is_some_and(|editor| editor.catalog().is_some())
        } else if macro_result {
            let verified = self.session.macros().is_some_and(|editor| {
                *editor.status() == byakko_core::macros::editor::Status::Ready
            });
            if verified {
                self.initialize_new_macro();
            }
            let current_draft = self.session.macros().and_then(|editor| editor.draft());
            if verified
                && macro_draft_before_read
                    .as_ref()
                    .is_none_or(|before| before.as_ref() != current_draft)
            {
                self.reset_macro_inputs();
            }
            if verified
                && self.macro_new_slot.as_ref().is_some_and(|slot| {
                    self.session
                        .macro_library_slots()
                        .is_some_and(|slots| slots.iter().any(|choice| &choice.id == slot))
                })
            {
                self.macro_new_slot = None;
            }
            verified
        } else {
            *self.session.status() == Status::Ready
        };
        if let Some(progress) = macro_assignment_completion {
            self.advance_macro_assignment(progress);
        }
        if self.closing == Closing::Waiting && !self.busy() {
            // Keep failures visible; a close request must not hide an uncertain write.
            if !verified {
                self.closing = Closing::Open;
            } else {
                return self.close();
            }
        }
        if activated_picture {
            self.picture_activation = None;
            if verified && !self.busy() {
                let request = self.session.request_picture_read();
                self.submit(request);
                return Task::none();
            }
        }
        if verified {
            if !self.flush_live_lighting()
                && !self.flush_live_picture()
                && !self.flush_live_settings()
            {
                self.read_initial_sections();
            }
        } else {
            self.initial_reads.clear();
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
        } else if self.session.dirty()
            || self.live_lighting.has_queued()
            || self.live_picture.has_queued()
            || self.live_settings.has_queued()
        {
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
                Message::DiscardAndClose
                    | Message::KeepEditing
                    | Message::Close
                    | Message::Poll
                    | Message::Scan
            )
        {
            return Task::none();
        }
        match message {
            Message::Archive(message) => return self.update_archive(message),
            Message::Lighting(message) => return self.update_lighting(message),
            Message::Picture(message) => self.update_picture(message),
            Message::Settings(message) => self.update_settings(message),
            Message::File(message) => return self.update_macro_files(message),
            Message::Record(message) => self.update_recording(message),
            Message::Page(page) => {
                self.action_browser.input = action_catalog::InputMode::Browse;
                self.page = page;
            }
            Message::Macro(message) => self.update_macro(message),
            Message::Shortcut(message) => self.update_shortcut(message),
            Message::SelectLayer(layer) => {
                self.layer = layer;
                self.sync_shortcut();
            }
            Message::SelectKey(key) => {
                if self
                    .session
                    .picture()
                    .is_some_and(|editor| editor.capabilities().keys.contains(&key))
                {
                    self.picture_selected = Some(key.clone());
                }
                self.selected = Some(key);
                self.sync_shortcut();
            }
            Message::Catalog(message) => return self.update_catalog(message),
            Message::Stage(index) => self.stage(index),
            Message::Read => {
                if self.refresh_connection() {
                    self.auto_read = AutoRead::Enabled;
                    let request = self.session.request_read();
                    self.submit(request);
                }
            }
            Message::Apply => {
                let request = self.session.request_apply();
                self.submit(request);
            }
            Message::Revert => {
                self.notice = self.session.revert().err();
                if self.notice.is_none() {
                    self.sync_shortcut();
                }
            }
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
        let normal = self.base_subscription();
        if self.page == Page::Keys
            && self.closing == Closing::Open
            && !self.session.recording()
            && self.action_browser.input == action_catalog::InputMode::Capture
        {
            Subscription::batch([
                normal,
                iced::event::listen_with(action_catalog::capture_event),
            ])
        } else {
            normal
        }
    }

    fn base_subscription(&self) -> Subscription<Message> {
        let close = window::close_requests().map(|_| Message::Close);
        if self.closing == Closing::ConfirmDiscard {
            let cancel = iced::event::listen_with(|event, _, _| match event {
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }) => Some(Message::KeepEditing),
                _ => None,
            });
            return Subscription::batch([close, cancel]);
        }
        if self.session.recording() {
            Subscription::batch([close, recording::subscription()])
        } else if (self.busy()
            || self.session.macro_catalog_scanning()
            || self.live_lighting.has_pending()
            || self.live_picture.has_pending()
            || self.live_settings.has_pending())
            && !matches!(
                self.session.activity(),
                byakko_core::session::Activity::MacroFile { .. }
            )
        {
            let interval = if self.busy() { 25 } else { 100 };
            let poll = iced::time::every(Duration::from_millis(interval)).map(|_| Message::Poll);
            if self.host.is_some() {
                let focus = iced::event::listen_with(|event, _, _| {
                    matches!(event, iced::Event::Window(window::Event::Unfocused))
                        .then_some(Message::Lighting(lighting::Message::StopHost))
                });
                Subscription::batch([close, poll, focus])
            } else {
                Subscription::batch([close, poll])
            }
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
        view::shell(self)
    }
}
