//! Correlation and effect routing for independently owned editors.
use crate::{
    contract::{
        ApplyFailure, Command, CommandPayload, Completion, CompletionPayload, FeatureCommand,
        FeatureResult, HostEvent, HostStart, HostTicket, Problem, Recovery,
    },
    editor::{
        Editor, Status, keymap::KeymapRules, lighting::LightingRules, macros::MacroRules,
        picture::PictureRules, settings::SettingsRules,
    },
    library::archive::{Capture, CaptureProblem},
    library::macros::Library,
    model::keymap::{Change, Descriptor},
    model::{archive::ArchiveCapabilities, lighting, picture, settings},
    recorder::macros::{DelayPolicy, Recorder, StopOutcome, Transition},
    workflow::Problem as WorkflowProblem,
    workflow::host::{self, HostOutcome, State as HostState},
    workflow::macro_assignment::{self, Assignment, Plan},
    workflow::picture_preparation::{self, Action as PictureAction, Preparation},
};
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Connection {
    Disconnected,
    Connected { generation: u64 },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Ignored,
    Loaded,
    Saved,
    Conflict,
    Failed(Problem),
    MacroLoaded,
    MacroSaved,
    LightingLoaded,
    LightingSaved,
    PictureLoaded,
    PictureSaved,
    SettingsLoaded,
    SettingsSaved,
    ArchiveCaptured,
    ArchiveCaptureFailed(CaptureProblem),
    PicturePreparationFailed {
        lighting_applied: bool,
        problem: WorkflowProblem,
    },
    CatalogLoaded,
    CatalogFailed(String),
    Continue(Command),
    AssignmentSucceeded {
        macro_saved: bool,
    },
    AssignmentFailed {
        macro_saved: bool,
        problem: WorkflowProblem,
    },
}
#[derive(Clone, Copy)]
enum Direction {
    Read,
    Save,
}
#[derive(Clone, Eq, PartialEq)]
enum Feature {
    Keymap,
    Macro { slot: String },
    Lighting,
    Picture,
    Settings,
    Archive,
}
struct Ticket {
    generation: u64,
    operation: u64,
    direction: Direction,
    feature: Feature,
}
pub struct Session {
    keymap: Editor<KeymapRules>,
    macros: Option<Editor<MacroRules>>,
    macro_library: Option<Library>,
    lighting: Option<Editor<LightingRules>>,
    picture: Option<Editor<PictureRules>>,
    settings: Option<Editor<SettingsRules>>,
    archive: Option<Capture>,
    assignment: Option<Assignment>,
    picture_preparation: Option<Preparation>,
    connection: Connection,
    generation: u64,
    operation: u64,
    pending: Option<Ticket>,
    recorder: Option<Recorder>,
    host: HostState,
}
impl Session {
    pub fn new(descriptor: Descriptor) -> Result<Self, String> {
        Ok(Self {
            keymap: Editor::new(KeymapRules::new(descriptor)?),
            macros: None,
            macro_library: None,
            lighting: None,
            picture: None,
            settings: None,
            archive: None,
            assignment: None,
            picture_preparation: None,
            connection: Connection::Disconnected,
            generation: 0,
            operation: 0,
            pending: None,
            recorder: None,
            host: HostState::Idle,
        })
    }
    pub fn with_macros(
        mut self,
        capabilities: crate::model::macros::Capabilities,
    ) -> Result<Self, String> {
        if capabilities.backend_id != self.descriptor().backend_id {
            return Err("Macro capabilities belong to another backend".into());
        }
        if self.macros.is_some() {
            return Err("Macro feature is already configured".into());
        }
        let rules = MacroRules::new(capabilities)?;
        self.macro_library = Some(Library::new(
            rules
                .capabilities()
                .slots
                .iter()
                .map(|slot| slot.id.clone()),
        ));
        self.macros = Some(Editor::new(rules));
        Ok(self)
    }
    pub fn descriptor(&self) -> &Descriptor {
        self.keymap.rules().descriptor()
    }
    pub fn with_archive(mut self, capabilities: ArchiveCapabilities) -> Result<Self, String> {
        if capabilities.backend_id != self.descriptor().backend_id {
            return Err("Archive capabilities belong to another backend".into());
        }
        if self.archive.is_some() {
            return Err("Archive capture is already configured".into());
        }
        self.archive = Some(Capture::new(capabilities)?);
        Ok(self)
    }
    pub fn archive(&self) -> Option<&Capture> {
        self.archive.as_ref()
    }
    pub fn capture_archive(&mut self) -> Result<Command, String> {
        self.archive
            .as_ref()
            .ok_or("Archive capture is not supported")?;
        self.begin(
            Feature::Archive,
            Direction::Read,
            CommandPayload::Archive(FeatureCommand::Read(())),
        )
    }
    pub fn stage_macro_document(
        &mut self,
        document: &crate::model::macros::Document,
    ) -> Result<crate::model::macros::DocumentMetadata, String> {
        self.idle()?;
        self.macro_feature()?.import_document(document)
    }
    pub fn export_macro_document(
        &self,
        name: String,
        binding: Option<String>,
    ) -> Result<crate::model::macros::Document, String> {
        self.idle()?;
        self.macros()
            .ok_or("Macros are not supported")?
            .export_document(name, binding)
    }
    pub fn keymap(&self) -> &Editor<KeymapRules> {
        &self.keymap
    }
    pub fn macros(&self) -> Option<&Editor<MacroRules>> {
        self.macros.as_ref()
    }
    pub fn macro_library(&self) -> Option<&Library> {
        self.macro_library.as_ref()
    }
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn requires_manual_read(&self) -> bool {
        requires_manual_read(self.keymap.status())
            || self
                .macros()
                .is_some_and(|editor| requires_manual_read(editor.status()))
            || self
                .lighting()
                .is_some_and(|editor| requires_manual_read(editor.status()))
            || self
                .picture()
                .is_some_and(|editor| requires_manual_read(editor.status()))
            || self
                .settings()
                .is_some_and(|editor| requires_manual_read(editor.status()))
    }
    /// Produce the next read from cache readiness; the client stops on conflict or failure.
    pub fn refresh_next(&mut self) -> Result<Option<Command>, String> {
        self.idle()?;
        self.connected()?;
        if self.keymap.status() != &Status::Ready {
            return self.read().map(Some);
        }
        if self
            .lighting()
            .is_some_and(|editor| editor.status() != &Status::Ready)
        {
            return self.read_lighting().map(Some);
        }
        if self
            .settings()
            .is_some_and(|editor| editor.status() != &Status::Ready)
        {
            return self.read_settings().map(Some);
        }
        if self
            .picture()
            .is_some_and(|editor| editor.status() != &Status::Ready)
        {
            return self.read_picture().map(Some);
        }
        if self
            .macros()
            .is_some_and(|editor| editor.baseline().is_some() && editor.status() != &Status::Ready)
        {
            return self.read_macro().map(Some);
        }
        Ok(None)
    }
    pub fn recording(&self) -> bool {
        self.recorder.is_some()
    }
    pub fn host(&self) -> &HostState {
        &self.host
    }
    pub fn start_host(
        &mut self,
        mode_id: &str,
        setting: Option<lighting::Setting>,
    ) -> Result<HostStart, String> {
        self.idle()?;
        let generation = self.connected()?;
        if self.catalog_scanning() {
            return Err("Finish macro discovery before host lighting".into());
        }
        let plan = host::plan(
            self.lighting().ok_or("Lighting is not supported")?,
            self.settings(),
            mode_id,
            setting,
        )?;
        let start = HostStart {
            ticket: HostTicket {
                generation,
                operation: self.next_operation()?,
            },
            mode: plan.mode,
            setting: plan.setting,
            expected: plan.expected,
        };
        self.host.begin(&start);
        Ok(start)
    }
    pub fn stop_host(&mut self) -> Option<HostTicket> {
        self.host.stop()
    }
    pub fn accept_host(&mut self, event: HostEvent) -> HostOutcome {
        match &mut self.lighting {
            Some(editor) => self.host.accept(event, editor),
            None => HostOutcome::Ignored,
        }
    }
    pub fn start_recording(&mut self, policy: DelayPolicy) -> Result<(), String> {
        self.idle()?;
        if self.catalog_scanning() {
            return Err("Finish macro discovery before recording".into());
        }
        self.recorder = Some(
            self.macros()
                .ok_or("Macros are not supported")?
                .begin_recording(policy)?,
        );
        Ok(())
    }
    pub fn record_input(
        &mut self,
        action: crate::model::macros::Action,
        now_ms: u64,
    ) -> Result<Transition, String> {
        let recorder = self
            .recorder
            .as_mut()
            .ok_or("Macro recording is not active")?;
        self.macros
            .as_mut()
            .ok_or("Macros are not supported")?
            .record_input(recorder, action, now_ms)
    }
    pub fn stop_recording(&mut self, now_ms: u64) -> Result<StopOutcome, String> {
        let recorder = self
            .recorder
            .as_ref()
            .ok_or("Macro recording is not active")?;
        let outcome = self
            .macros
            .as_mut()
            .ok_or("Macros are not supported")?
            .finish_recording(recorder, now_ms)?;
        self.recorder = None;
        Ok(outcome)
    }
    pub fn catalog_scanning(&self) -> bool {
        self.macro_library.as_ref().is_some_and(Library::scanning)
    }
    pub fn cancel_catalog(&mut self) {
        if let Some(library) = &mut self.macro_library {
            library.cancel();
        }
    }
    pub fn connect(&mut self) -> Result<u64, String> {
        self.idle()?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("Connection generation exhausted")?;
        self.connection = Connection::Connected {
            generation: self.generation,
        };
        self.keymap.invalidate();
        if let Some(feature) = &mut self.macros {
            feature.invalidate();
        }
        if let Some(editor) = &mut self.lighting {
            editor.invalidate();
        }
        if let Some(editor) = &mut self.picture {
            editor.invalidate();
        }
        if let Some(editor) = &mut self.settings {
            editor.invalidate();
        }
        if let Some(library) = &mut self.macro_library {
            library.invalidate();
        }
        Ok(self.generation)
    }
    pub fn disconnect(&mut self) -> Result<Option<StopOutcome>, String> {
        if let Some(editor) = &mut self.lighting {
            self.host.disconnect(editor);
        }
        let recording = match self.recorder.as_ref() {
            Some(recorder) => self.stop_recording(recorder.last_timestamp()).map(Some),
            None => Ok(None),
        };
        if let Some(ticket) = &self.pending
            && matches!(ticket.direction, Direction::Save)
        {
            let failure = ApplyFailure {
                message: "Connection lost during save".into(),
                recovery: Recovery::Unverified,
            };
            match &ticket.feature {
                Feature::Archive => {}
                Feature::Keymap => self.keymap.accept_apply(Err(failure)),
                Feature::Lighting => {
                    if let Some(editor) = &mut self.lighting {
                        editor.accept_apply(Err(failure));
                    }
                }
                Feature::Picture => {
                    if let Some(editor) = &mut self.picture {
                        editor.accept_apply(Err(failure));
                    }
                }
                Feature::Settings => {
                    if let Some(editor) = &mut self.settings {
                        editor.accept_apply(Err(failure));
                    }
                }
                Feature::Macro { .. } => {
                    if let Some(feature) = &mut self.macros {
                        feature.accept_apply(Err(failure));
                    }
                }
            }
        }
        self.connection = Connection::Disconnected;
        if self
            .pending
            .as_ref()
            .is_some_and(|ticket| ticket.feature == Feature::Archive)
            && let Some(capture) = &mut self.archive
        {
            let _ = capture.accept(Err("Connection lost during archive capture".into()));
        }
        self.pending = None;
        self.assignment = None;
        self.picture_preparation = None;
        self.keymap.invalidate();
        if let Some(feature) = &mut self.macros {
            feature.invalidate();
        }
        if let Some(editor) = &mut self.lighting {
            editor.invalidate();
        }
        if let Some(editor) = &mut self.picture {
            editor.invalidate();
        }
        if let Some(editor) = &mut self.settings {
            editor.invalidate();
        }
        if let Some(library) = &mut self.macro_library {
            library.invalidate();
        }
        recording
    }
    pub fn read(&mut self) -> Result<Command, String> {
        self.begin(
            Feature::Keymap,
            Direction::Read,
            CommandPayload::Keymap(FeatureCommand::Read(())),
        )
    }
    pub fn edit(&mut self, change: Change) -> Result<(), String> {
        self.idle()?;
        self.keymap.edit(change)
    }
    pub fn revert(&mut self) -> Result<(), String> {
        self.idle()?;
        self.keymap.revert()
    }
    pub fn save(&mut self) -> Result<Command, String> {
        let (expected, desired) = self.keymap.request_apply()?;
        self.begin(
            Feature::Keymap,
            Direction::Save,
            CommandPayload::Keymap(FeatureCommand::Apply { expected, desired }),
        )
    }
    pub fn select_macro(&mut self, slot: &str) -> Result<(), String> {
        self.idle()?;
        self.macro_feature()?.select(slot)
    }
    pub fn read_macro(&mut self) -> Result<Command, String> {
        let slot = self
            .macros()
            .ok_or("Macros are not supported")?
            .slot()
            .to_owned();
        self.begin(
            Feature::Macro { slot: slot.clone() },
            Direction::Read,
            CommandPayload::Macro(FeatureCommand::Read(slot)),
        )
    }
    pub fn edit_macro(&mut self, edit: crate::model::macros::Edit) -> Result<(), String> {
        self.idle()?;
        self.macro_feature()?.edit(edit)
    }
    pub fn revert_macro(&mut self) -> Result<(), String> {
        self.idle()?;
        self.macro_feature()?.revert()
    }
    pub fn stage_macro_snapshot(
        &mut self,
        target: &crate::model::macros::Snapshot,
    ) -> Result<(), String> {
        self.idle()?;
        self.macro_feature()?.import(target)
    }
    /// Prefer a known empty, unbound slot; unknown candidates require a foreground read.
    pub fn macro_candidate(&self) -> Result<String, String> {
        let feature = self.macros.as_ref().ok_or("Macros are not supported")?;
        let bindings = self
            .keymap
            .draft()
            .ok_or("Read the keymap before choosing a macro slot")?;
        self.macro_library
            .as_ref()
            .ok_or("Macros are not supported")?
            .candidate(feature.capabilities(), bindings)
    }
    pub fn save_macro(&mut self) -> Result<Command, String> {
        let editor = self.macro_feature()?;
        let slot = editor.slot().to_owned();
        let (expected, desired) = editor.request_apply()?;
        let command = self.begin(
            Feature::Macro { slot },
            Direction::Save,
            CommandPayload::Macro(FeatureCommand::Apply { expected, desired }),
        )?;
        self.cancel_catalog();
        Ok(command)
    }
    pub fn request_macro_catalog(&mut self) -> Result<Command, String> {
        if !self.host.is_idle() {
            return Err("Stop host lighting before macro discovery".into());
        }
        if self.recording() {
            return Err("Stop recording before macro discovery".into());
        }
        let generation = self.connected()?;
        if self.catalog_scanning() {
            return Err("Macro discovery is already running".into());
        }
        let slots = self
            .macros()
            .ok_or("Macros are not supported")?
            .capabilities()
            .slots
            .iter()
            .map(|slot| slot.id.clone())
            .collect();
        let operation = self.next_operation()?;
        self.macro_library
            .as_mut()
            .ok_or("Macros are not supported")?
            .begin_scan(generation, operation);
        Ok(Command {
            generation,
            operation,
            payload: CommandPayload::ReadMacroCatalog { slots },
        })
    }
    pub fn save_and_assign_macro(
        &mut self,
        layer: &str,
        key: &str,
        binding: &str,
    ) -> Result<Command, String> {
        self.idle()?;
        self.connected()?;
        let Plan {
            change,
            save_macro,
            already_assigned,
        } = macro_assignment::plan(
            self.descriptor(),
            &self.keymap,
            self.macros().ok_or("Macros are not supported")?,
            layer,
            key,
            binding,
        )?;
        if save_macro {
            let command = self.save_macro()?;
            self.assignment = Some(Assignment::SavingMacro { change });
            Ok(command)
        } else {
            if already_assigned {
                return Err("Macro is already assigned to this key".into());
            }
            self.keymap.bind_macro(change)?;
            let command = self.save()?;
            self.assignment = Some(Assignment::Assigning { macro_saved: false });
            Ok(command)
        }
    }

    pub fn with_lighting(mut self, capabilities: lighting::Capabilities) -> Result<Self, String> {
        if capabilities.backend_id != self.descriptor().backend_id {
            return Err("Lighting capabilities belong to another backend".into());
        }
        if self.lighting.is_some() {
            return Err("Lighting feature is already configured".into());
        }

        self.lighting = Some(Editor::new(LightingRules::new(capabilities)?));
        Ok(self)
    }
    pub fn lighting(&self) -> Option<&Editor<LightingRules>> {
        self.lighting.as_ref()
    }
    pub fn read_lighting(&mut self) -> Result<Command, String> {
        self.lighting.as_ref().ok_or("Lighting is not supported")?;
        self.begin(
            Feature::Lighting,
            Direction::Read,
            CommandPayload::Lighting(FeatureCommand::Read(())),
        )
    }
    pub fn edit_lighting(&mut self, edit: lighting::Edit) -> Result<(), String> {
        self.editable_activity(&Feature::Lighting)?;
        self.lighting
            .as_mut()
            .ok_or("Lighting is not supported")?
            .edit(edit)
    }
    pub fn revert_lighting(&mut self) -> Result<(), String> {
        self.idle()?;
        self.lighting
            .as_mut()
            .ok_or("Lighting is not supported")?
            .revert()
    }
    pub fn save_lighting(&mut self) -> Result<Command, String> {
        let (expected, desired) = self
            .lighting
            .as_mut()
            .ok_or("Lighting is not supported")?
            .request_apply()?;
        self.begin(
            Feature::Lighting,
            Direction::Save,
            CommandPayload::Lighting(FeatureCommand::Apply { expected, desired }),
        )
    }

    pub fn with_picture(mut self, capabilities: picture::Capabilities) -> Result<Self, String> {
        if capabilities.backend_id != self.descriptor().backend_id {
            return Err("Picture capabilities belong to another backend".into());
        }
        if self.picture.is_some() {
            return Err("Picture feature is already configured".into());
        }
        crate::validation::picture::validate_capabilities(&capabilities, self.descriptor())?;
        self.picture = Some(Editor::new(PictureRules::new(capabilities)));
        Ok(self)
    }
    pub fn picture(&self) -> Option<&Editor<PictureRules>> {
        self.picture.as_ref()
    }
    pub fn read_picture(&mut self) -> Result<Command, String> {
        self.picture.as_ref().ok_or("Picture is not supported")?;
        self.begin(
            Feature::Picture,
            Direction::Read,
            CommandPayload::Picture(FeatureCommand::Read(())),
        )
    }
    pub fn edit_picture(&mut self, edit: picture::Edit) -> Result<(), String> {
        self.editable_activity(&Feature::Picture)?;
        self.picture
            .as_mut()
            .ok_or("Picture is not supported")?
            .edit(edit)
    }
    pub fn revert_picture(&mut self) -> Result<(), String> {
        self.idle()?;
        self.picture
            .as_mut()
            .ok_or("Picture is not supported")?
            .revert()
    }
    pub fn save_picture(&mut self) -> Result<Command, String> {
        let (expected, desired) = self
            .picture
            .as_mut()
            .ok_or("Picture is not supported")?
            .request_apply()?;
        self.begin(
            Feature::Picture,
            Direction::Save,
            CommandPayload::Picture(FeatureCommand::Apply { expected, desired }),
        )
    }

    pub fn with_settings(mut self, capabilities: settings::Capabilities) -> Result<Self, String> {
        if capabilities.backend_id != self.descriptor().backend_id {
            return Err("Settings capabilities belong to another backend".into());
        }
        if self.settings.is_some() {
            return Err("Settings feature is already configured".into());
        }

        self.settings = Some(Editor::new(SettingsRules::new(capabilities)?));
        Ok(self)
    }
    pub fn settings(&self) -> Option<&Editor<SettingsRules>> {
        self.settings.as_ref()
    }
    pub fn read_settings(&mut self) -> Result<Command, String> {
        self.settings.as_ref().ok_or("Settings is not supported")?;
        self.begin(
            Feature::Settings,
            Direction::Read,
            CommandPayload::Settings(FeatureCommand::Read(())),
        )
    }
    pub fn edit_settings(&mut self, edit: settings::Edit) -> Result<(), String> {
        self.editable_activity(&Feature::Settings)?;
        self.settings
            .as_mut()
            .ok_or("Settings is not supported")?
            .edit(edit)
    }
    pub fn revert_settings(&mut self) -> Result<(), String> {
        self.idle()?;
        self.settings
            .as_mut()
            .ok_or("Settings is not supported")?
            .revert()
    }
    pub fn save_settings(&mut self) -> Result<Command, String> {
        let (expected, desired) = self
            .settings
            .as_mut()
            .ok_or("Settings is not supported")?
            .request_apply()?;
        self.begin(
            Feature::Settings,
            Direction::Save,
            CommandPayload::Settings(FeatureCommand::Apply { expected, desired }),
        )
    }

    pub fn stage_lighting(&mut self, setting: lighting::Setting) -> Result<(), String> {
        self.editable_activity(&Feature::Lighting)?;
        self.lighting
            .as_mut()
            .ok_or("Lighting is not supported")?
            .stage(setting)
    }
    pub fn stage_picture_snapshot(&mut self, target: &picture::Snapshot) -> Result<(), String> {
        self.editable_activity(&Feature::Picture)?;
        self.picture
            .as_mut()
            .ok_or("Picture is not supported")?
            .import(target)
    }
    pub fn prepare_picture(&mut self) -> Result<Option<Command>, String> {
        self.idle()?;
        self.connected()?;
        let picture = self.picture().ok_or("Picture is not supported")?;
        if picture.capabilities().lighting_effect.is_none() {
            return match picture.status() {
                Status::Ready => Ok(None),
                Status::Unloaded
                | Status::Unverified {
                    problem: Problem::ReadRequired,
                } if !picture.dirty() => self.read_picture().map(Some),
                _ => Err("Read and resolve the picture problem before preparing it".into()),
            };
        }
        let action = picture_preparation::plan(
            self.lighting().ok_or("Lighting is not supported")?,
            self.picture().ok_or("Picture is not supported")?,
        )?;
        self.begin_picture_preparation(action, false)
    }
    fn begin_picture_preparation(
        &mut self,
        action: PictureAction,
        lighting_applied: bool,
    ) -> Result<Option<Command>, String> {
        let (state, command) = match action {
            PictureAction::Ready => return Ok(None),
            PictureAction::ReadLighting => (Preparation::ReadingLighting, self.read_lighting()?),
            PictureAction::SaveLighting(setting) => {
                self.stage_lighting(setting)?;
                (Preparation::SavingLighting, self.save_lighting()?)
            }
            PictureAction::ReadPicture => (
                Preparation::ReadingPicture { lighting_applied },
                self.read_picture()?,
            ),
        };
        self.picture_preparation = Some(state);
        Ok(Some(command))
    }
    fn picture_preparation_outcome(&mut self, outcome: Outcome) -> Outcome {
        let Some(state) = self.picture_preparation.take() else {
            return outcome;
        };
        let step = state.advance(
            outcome,
            self.lighting().expect("preparation owns lighting"),
            self.picture().expect("preparation owns picture"),
        );
        match step {
            picture_preparation::Step::Finished(outcome) => outcome,
            picture_preparation::Step::Next {
                action,
                lighting_applied,
            } => match self.begin_picture_preparation(action, lighting_applied) {
                Ok(Some(command)) => Outcome::Continue(command),
                Ok(None) => Outcome::PictureLoaded,
                Err(reason) => picture_preparation::failed(
                    lighting_applied,
                    WorkflowProblem::Validation(reason),
                ),
            },
        }
    }
    fn editable_activity(&self, feature: &Feature) -> Result<(), String> {
        if self.picture_preparation.is_some() {
            return Err("Wait for picture preparation before editing".into());
        }
        if !self.recording()
            && self.pending.as_ref().is_some_and(|ticket| {
                ticket.feature == *feature && matches!(ticket.direction, Direction::Save)
            })
        {
            Ok(())
        } else {
            self.idle()
        }
    }
    fn macro_feature(&mut self) -> Result<&mut Editor<MacroRules>, String> {
        self.macros
            .as_mut()
            .ok_or_else(|| "Macros are not supported".into())
    }
    fn idle(&self) -> Result<(), String> {
        if !self.host.is_idle() {
            Err("Stop host lighting before another session activity".into())
        } else if self.recording() {
            Err("Stop recording before another session activity".into())
        } else if self.busy() {
            Err("Wait for the current operation".into())
        } else {
            Ok(())
        }
    }
    fn connected(&self) -> Result<u64, String> {
        match self.connection {
            Connection::Connected { generation } => Ok(generation),
            Connection::Disconnected => Err("Connect before requesting device operations".into()),
        }
    }
    fn next_operation(&mut self) -> Result<u64, String> {
        self.operation = self
            .operation
            .checked_add(1)
            .ok_or("Operation ID exhausted")?;
        Ok(self.operation)
    }
    fn begin(
        &mut self,
        feature: Feature,
        direction: Direction,
        payload: CommandPayload,
    ) -> Result<Command, String> {
        let correlation = self
            .idle()
            .and_then(|()| self.connected())
            .and_then(|generation| {
                self.next_operation()
                    .map(|operation| (generation, operation))
            });
        let (generation, operation) = match correlation {
            Ok(correlation) => correlation,
            Err(reason) => {
                if matches!(direction, Direction::Save) {
                    match &feature {
                        Feature::Archive => {}
                        Feature::Keymap => self.keymap.cancel_apply(),
                        Feature::Lighting => {
                            if let Some(editor) = &mut self.lighting {
                                editor.cancel_apply();
                            }
                        }
                        Feature::Picture => {
                            if let Some(editor) = &mut self.picture {
                                editor.cancel_apply();
                            }
                        }
                        Feature::Settings => {
                            if let Some(editor) = &mut self.settings {
                                editor.cancel_apply();
                            }
                        }
                        Feature::Macro { .. } => {
                            if let Some(editor) = &mut self.macros {
                                editor.cancel_apply();
                            }
                        }
                    }
                }
                return Err(reason);
            }
        };
        self.pending = Some(Ticket {
            generation,
            operation,
            direction,
            feature,
        });
        Ok(Command {
            generation,
            operation,
            payload,
        })
    }
    pub fn accept(&mut self, completion: Completion) -> Outcome {
        if let (Some(library), Some(editor)) = (&mut self.macro_library, &self.macros)
            && let Some(result) = library.accept_scan(&completion, editor.rules())
        {
            return result.map_or_else(Outcome::CatalogFailed, |()| Outcome::CatalogLoaded);
        }
        let Some(ticket) = &self.pending else {
            return Outcome::Ignored;
        };
        if completion.generation != ticket.generation || completion.operation != ticket.operation {
            return Outcome::Ignored;
        }
        let outcome = match (&ticket.feature, ticket.direction, completion.payload) {
            (
                Feature::Archive,
                Direction::Read,
                CompletionPayload::Archive(FeatureResult::Read(result)),
            ) => self
                .archive
                .as_mut()
                .expect("archive ticket owns capture")
                .accept(result)
                .map_or_else(Outcome::ArchiveCaptureFailed, |()| Outcome::ArchiveCaptured),
            (
                Feature::Keymap,
                Direction::Read,
                CompletionPayload::Keymap(FeatureResult::Read(result)),
            ) => {
                self.keymap.accept_read(result);
                self.keymap_outcome(Outcome::Loaded)
            }
            (
                Feature::Keymap,
                Direction::Save,
                CompletionPayload::Keymap(FeatureResult::Apply(result)),
            ) => {
                self.keymap.accept_apply(result);
                self.keymap_outcome(Outcome::Saved)
            }

            (Feature::Lighting, direction, CompletionPayload::Lighting(result))
                if matches!(
                    (direction, &result),
                    (Direction::Read, FeatureResult::Read(_))
                        | (Direction::Save, FeatureResult::Apply(_))
                ) =>
            {
                let editor = self.lighting.as_mut().expect("lighting ticket owns editor");
                let outcome = accept_feature(
                    editor,
                    result,
                    match direction {
                        Direction::Read => Outcome::LightingLoaded,
                        Direction::Save => Outcome::LightingSaved,
                    },
                );

                if matches!(
                    outcome,
                    Outcome::LightingLoaded | Outcome::LightingSaved | Outcome::Conflict
                ) {
                    let observation = match editor.status() {
                        Status::Conflict { device } => Some(device),
                        _ => editor.baseline(),
                    };
                    if let Some(observation) = observation
                        && !observation.picture_context.is_empty()
                        && let Some(picture) = &mut self.picture
                        && picture.baseline().is_some_and(|snapshot| {
                            !snapshot.context_revision.is_empty()
                                && snapshot.context_revision != observation.picture_context
                        })
                    {
                        picture.invalidate();
                    }
                }
                outcome
            }

            (Feature::Picture, direction, CompletionPayload::Picture(result))
                if matches!(
                    (direction, &result),
                    (Direction::Read, FeatureResult::Read(_))
                        | (Direction::Save, FeatureResult::Apply(_))
                ) =>
            {
                let editor = self.picture.as_mut().expect("picture ticket owns editor");
                accept_feature(
                    editor,
                    result,
                    match direction {
                        Direction::Read => Outcome::PictureLoaded,
                        Direction::Save => Outcome::PictureSaved,
                    },
                )
            }

            (Feature::Settings, direction, CompletionPayload::Settings(result))
                if matches!(
                    (direction, &result),
                    (Direction::Read, FeatureResult::Read(_))
                        | (Direction::Save, FeatureResult::Apply(_))
                ) =>
            {
                let editor = self.settings.as_mut().expect("settings ticket owns editor");
                accept_feature(
                    editor,
                    result,
                    match direction {
                        Direction::Read => Outcome::SettingsLoaded,
                        Direction::Save => Outcome::SettingsSaved,
                    },
                )
            }
            (
                Feature::Macro { slot: expected },
                direction,
                CompletionPayload::Macro { slot, result },
            ) if *expected == slot
                && matches!(
                    (direction, &result),
                    (Direction::Read, FeatureResult::Read(_))
                        | (Direction::Save, FeatureResult::Apply(_))
                ) =>
            {
                let editor = self.macros.as_mut().expect("macro ticket owns editor");
                match result {
                    FeatureResult::Read(result) => editor.accept_read(result),
                    FeatureResult::Apply(result) => editor.accept_apply(result),
                }
                let success = match direction {
                    Direction::Read => Outcome::MacroLoaded,
                    Direction::Save => Outcome::MacroSaved,
                };
                match self
                    .macro_library
                    .as_mut()
                    .expect("macro editor owns library")
                    .observe_editor(editor)
                {
                    Err(problem) => Outcome::Failed(problem),
                    Ok(()) if matches!(editor.status(), Status::Conflict { .. }) => {
                        Outcome::Conflict
                    }
                    Ok(()) => success,
                }
            }
            _ => return Outcome::Ignored,
        };
        if matches!(ticket.direction, Direction::Save) && matches!(outcome, Outcome::Failed(_)) {
            self.cancel_catalog();
        }
        self.pending = None;
        let outcome = self.assignment_outcome(outcome);
        self.picture_preparation_outcome(outcome)
    }
    fn keymap_outcome(&self, success: Outcome) -> Outcome {
        if matches!(self.keymap.status(), Status::Conflict { .. }) {
            Outcome::Conflict
        } else {
            self.keymap
                .problem()
                .cloned()
                .map_or(success, Outcome::Failed)
        }
    }
    fn assignment_outcome(&mut self, outcome: Outcome) -> Outcome {
        let Some(assignment) = self.assignment.take() else {
            return outcome;
        };
        match assignment.advance(outcome) {
            macro_assignment::Step::Finished(outcome) => outcome,
            macro_assignment::Step::Assign(change) => {
                let request = macro_assignment::assign(&mut self.keymap, change);
                let command = match request {
                    Ok(None) => return Outcome::AssignmentSucceeded { macro_saved: true },
                    Ok(Some(request)) => self.begin(
                        Feature::Keymap,
                        Direction::Save,
                        CommandPayload::Keymap(request),
                    ),
                    Err(reason) => Err(reason),
                };
                match command {
                    Ok(command) => {
                        self.assignment = Some(Assignment::Assigning { macro_saved: true });
                        Outcome::Continue(command)
                    }
                    Err(reason) => Outcome::AssignmentFailed {
                        macro_saved: true,
                        problem: WorkflowProblem::Validation(reason),
                    },
                }
            }
        }
    }
}
#[cfg(test)]
#[path = "tests/session.rs"]
mod tests;

fn accept_feature<F: crate::editor::Feature>(
    editor: &mut Editor<F>,
    result: FeatureResult<F::Snapshot>,
    success: Outcome,
) -> Outcome {
    match result {
        FeatureResult::Read(result) => editor.accept_read(result),
        FeatureResult::Apply(result) => editor.accept_apply(result),
    }
    if matches!(editor.status(), Status::Conflict { .. }) {
        Outcome::Conflict
    } else {
        editor.problem().cloned().map_or(success, Outcome::Failed)
    }
}

fn requires_manual_read<S>(status: &Status<S>) -> bool {
    match status {
        Status::Conflict { .. } => true,
        Status::Unverified { problem } => match problem {
            Problem::Apply(_) | Problem::InvalidApplyResult(_) | Problem::ApplyReadbackMismatch => {
                true
            }
            Problem::ReadRequired | Problem::Read(_) => false,
        },
        Status::Unloaded | Status::Ready => false,
    }
}
