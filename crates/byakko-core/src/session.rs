//! Correlates feature effects without owning device I/O.
mod macro_assignment;
mod macros;
use crate::{
    Action, Change, Descriptor, State,
    contract::{
        ApplyFailure, Command, CommandPayload, Completion, CompletionPayload, FeatureCommand,
        FeatureResult, Problem, Recovery,
    },
    keymap::{Bindings, Editor},
    validate_state,
};
pub use macro_assignment::AssignmentProblem;
use macro_assignment::{Assignment, Plan};
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
    descriptor: Descriptor,
    keymap: Editor,
    macros: Option<macros::Macros>,
    assignment: Option<Assignment>,
    connection: Connection,
    generation: u64,
    operation: u64,
    pending: Option<Ticket>,
}
impl Session {
    pub fn new(descriptor: Descriptor) -> Result<Self, String> {
        let bindings: Bindings = descriptor
            .layers
            .iter()
            .map(|layer| {
                (
                    layer.id.clone(),
                    descriptor
                        .keys
                        .iter()
                        .map(|key| (key.id.clone(), Action::Disabled))
                        .collect(),
                )
            })
            .collect();
        validate_state(
            &descriptor,
            &State {
                revision: Vec::new(),
                bindings,
            },
        )?;
        Ok(Self {
            descriptor,
            keymap: Editor::new(),
            macros: None,
            assignment: None,
            connection: Connection::Disconnected,
            generation: 0,
            operation: 0,
            pending: None,
        })
    }
    pub fn with_macros(
        mut self,
        capabilities: crate::macros::Capabilities,
    ) -> Result<Self, String> {
        if capabilities.backend_id != self.descriptor.backend_id {
            return Err("Macro capabilities belong to another backend".into());
        }
        if self.macros.is_some() {
            return Err("Macro feature is already configured".into());
        }
        self.macros = Some(macros::Macros::new(capabilities)?);
        Ok(self)
    }
    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
    pub fn keymap(&self) -> &Editor {
        &self.keymap
    }
    pub fn macros(&self) -> Option<&crate::macros::editor::Editor> {
        self.macros.as_ref().map(|feature| &feature.editor)
    }
    pub fn macro_library(&self) -> Option<&crate::macros::library::Library> {
        self.macros.as_ref().map(|feature| &feature.library)
    }
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn catalog_scanning(&self) -> bool {
        self.macros
            .as_ref()
            .is_some_and(|feature| feature.scan.is_some())
    }
    pub fn cancel_catalog(&mut self) {
        if let Some(feature) = &mut self.macros {
            feature.scan = None;
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
        Ok(self.generation)
    }
    pub fn disconnect(&mut self) {
        if let Some(ticket) = &self.pending
            && matches!(ticket.direction, Direction::Save)
        {
            let failure = ApplyFailure {
                message: "Connection lost during save".into(),
                recovery: Recovery::Unverified,
            };
            match &ticket.feature {
                Feature::Keymap => self.keymap.applied(&self.descriptor, Err(failure)),
                Feature::Macro { .. } => {
                    if let Some(feature) = &mut self.macros {
                        feature.editor.accept_apply(Err(failure));
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
        self.keymap.edit(&self.descriptor, change)
    }
    pub fn revert(&mut self) -> Result<(), String> {
        self.idle()?;
        self.keymap.revert()
    }
    pub fn save(&mut self) -> Result<Command, String> {
        let (expected, desired) = self.keymap.save(&self.descriptor)?;
        self.begin(
            Feature::Keymap,
            Direction::Save,
            CommandPayload::Keymap(FeatureCommand::Apply { expected, desired }),
        )
    }
    pub fn select_macro(&mut self, slot: &str) -> Result<(), String> {
        self.idle()?;
        self.macro_feature()?.editor.select(slot)
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
    pub fn edit_macro(&mut self, edit: crate::macros::Edit) -> Result<(), String> {
        self.idle()?;
        self.macro_feature()?.editor.edit(edit)
    }
    pub fn revert_macro(&mut self) -> Result<(), String> {
        self.idle()?;
        self.macro_feature()?.editor.revert()
    }
    pub fn stage_macro_snapshot(&mut self, target: &crate::macros::Snapshot) -> Result<(), String> {
        self.idle()?;
        self.macro_feature()?.editor.import(target)
    }
    /// Prefer a known empty, unbound slot; unknown candidates require a foreground read.
    pub fn macro_candidate(&self) -> Result<String, String> {
        let feature = self.macros.as_ref().ok_or("Macros are not supported")?;
        let bindings = self
            .keymap
            .draft()
            .ok_or("Read the keymap before choosing a macro slot")?;
        feature
            .library
            .candidate(feature.editor.capabilities(), bindings)
    }
    pub fn save_macro(&mut self) -> Result<Command, String> {
        let editor = self.macros().ok_or("Macros are not supported")?;
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
        self.macro_feature()?.begin_scan(generation, operation);
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
            &self.descriptor,
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
            self.keymap.bind_macro(&self.descriptor, change)?;
            let command = self.save()?;
            self.assignment = Some(Assignment::Assigning { macro_saved: false });
            Ok(command)
        }
    }
    fn macro_feature(&mut self) -> Result<&mut macros::Macros, String> {
        self.macros
            .as_mut()
            .ok_or_else(|| "Macros are not supported".into())
    }
    fn idle(&self) -> Result<(), String> {
        if self.busy() {
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
        self.idle()?;
        let generation = self.connected()?;
        let operation = self.next_operation()?;
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
        if let Some(feature) = &mut self.macros
            && let Some(result) = feature.accept_scan(&completion)
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
                self.keymap.read(&self.descriptor, result);
                self.keymap_outcome(Outcome::Loaded)
            }
            (
                Feature::Keymap,
                Direction::Save,
                CompletionPayload::Keymap(FeatureResult::Apply(result)),
            ) => {
                self.keymap.applied(&self.descriptor, result);
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
                let feature = self.macros.as_mut().expect("macro ticket owns feature");
                let success = match direction {
                    Direction::Read => Outcome::MacroLoaded,
                    Direction::Save => Outcome::MacroSaved,
                };
                match feature.accept(result) {
                    Err(problem) => Outcome::Failed(problem),
                    Ok(())
                        if matches!(
                            feature.editor.status(),
                            crate::macros::editor::Status::Conflict { .. }
                        ) =>
                    {
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
        if matches!(self.keymap.status(), crate::keymap::Status::Conflict { .. }) {
            Outcome::Conflict
        } else {
            self.keymap.problem().map_or(success, Outcome::Failed)
        }
    }
    fn assignment_outcome(&mut self, outcome: Outcome) -> Outcome {
        let Some(assignment) = self.assignment.take() else {
            return outcome;
        };
        match assignment.advance(outcome) {
            macro_assignment::Step::Finished(outcome) => outcome,
            macro_assignment::Step::Assign(change) => {
                let request = macro_assignment::assign(&mut self.keymap, &self.descriptor, change);
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
mod tests;
