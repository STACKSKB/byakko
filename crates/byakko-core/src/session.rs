//! Correlation and effect routing for independently owned editors.
use crate::{
    contract::{
        ApplyFailure, Command, CommandPayload, Completion, CompletionPayload, FeatureCommand,
        FeatureResult, Problem, Recovery,
    },
    editor::{Editor, Status, keymap::KeymapRules, macros::MacroRules},
    library::macros::Library,
    model::keymap::{Change, Descriptor},
    recorder::macros::{DelayPolicy, Recorder, StopOutcome, Transition},
    workflow::macro_assignment::{self, Assignment, AssignmentProblem, Plan},
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
    CatalogLoaded,
    CatalogFailed(String),
    Continue(Command),
    AssignmentSucceeded {
        macro_saved: bool,
    },
    AssignmentFailed {
        macro_saved: bool,
        problem: AssignmentProblem,
    },
}
#[derive(Clone, Copy)]
enum Direction {
    Read,
    Save,
}
#[derive(Clone)]
enum Feature {
    Keymap,
    Macro { slot: String },
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
    assignment: Option<Assignment>,
    connection: Connection,
    generation: u64,
    operation: u64,
    pending: Option<Ticket>,
    recorder: Option<Recorder>,
}
impl Session {
    pub fn new(descriptor: Descriptor) -> Result<Self, String> {
        Ok(Self {
            keymap: Editor::new(KeymapRules::new(descriptor)?),
            macros: None,
            macro_library: None,
            assignment: None,
            connection: Connection::Disconnected,
            generation: 0,
            operation: 0,
            pending: None,
            recorder: None,
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
    pub fn recording(&self) -> bool {
        self.recorder.is_some()
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
        if let Some(library) = &mut self.macro_library {
            library.invalidate();
        }
        Ok(self.generation)
    }
    pub fn disconnect(&mut self) -> Result<Option<StopOutcome>, String> {
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
                Feature::Keymap => self.keymap.accept_apply(Err(failure)),
                Feature::Macro { .. } => {
                    if let Some(feature) = &mut self.macros {
                        feature.accept_apply(Err(failure));
                    }
                }
            }
        }
        self.connection = Connection::Disconnected;
        self.pending = None;
        self.assignment = None;
        self.keymap.invalidate();
        if let Some(feature) = &mut self.macros {
            feature.invalidate();
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
    fn macro_feature(&mut self) -> Result<&mut Editor<MacroRules>, String> {
        self.macros
            .as_mut()
            .ok_or_else(|| "Macros are not supported".into())
    }
    fn idle(&self) -> Result<(), String> {
        if self.recording() {
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
                        Feature::Keymap => self.keymap.cancel_apply(),
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
        self.assignment_outcome(outcome)
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
                        problem: AssignmentProblem::Validation(reason),
                    },
                }
            }
        }
    }
}
#[cfg(test)]
#[path = "tests/session.rs"]
mod tests;
