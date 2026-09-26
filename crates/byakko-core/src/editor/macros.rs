//! Selected macro identity, editing and binding policy.
use super::{Editor, Feature, Reception, Status};
use crate::{
    model::macros::*,
    validation::macros::{validate_capabilities, validate_program},
};
pub struct MacroRules {
    capabilities: Capabilities,
    slot: String,
}
impl MacroRules {
    pub fn new(capabilities: Capabilities) -> Result<Self, String> {
        validate_capabilities(&capabilities)?;
        let slot = capabilities.slots[0].id.clone();
        Ok(Self { capabilities, slot })
    }
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }
    pub(crate) fn validate_snapshot_for(
        &self,
        snapshot: &Snapshot,
        slot: &str,
    ) -> Result<(), String> {
        if snapshot.backend_id != self.capabilities.backend_id
            || snapshot.slot != slot
            || !self
                .capabilities
                .slots
                .iter()
                .any(|choice| choice.id == slot)
        {
            return Err("Macro result belongs to a different backend or slot".into());
        }
        if let Content::Editable(program) = &snapshot.content {
            validate_program(&self.capabilities, program)?;
        }
        Ok(())
    }
}
impl Feature for MacroRules {
    type Snapshot = Snapshot;
    type Value = Program;
    type Edit = Edit;
    type Write = Program;
    fn value(snapshot: &Snapshot) -> Option<&Program> {
        match &snapshot.content {
            Content::Editable(program) => Some(program),
            Content::Opaque { .. } => None,
        }
    }
    fn validate(&self, snapshot: &Snapshot, _: Reception) -> Result<(), String> {
        self.validate_snapshot_for(snapshot, &self.slot)
    }
    fn edit(&self, _: &Snapshot, draft: &mut Option<Program>, change: Edit) -> Result<(), String> {
        let next = edit_program(
            &self.capabilities,
            draft.as_ref().ok_or("Macro is not editable")?,
            change,
        )?;
        if !self
            .capabilities
            .editable_repeat_counts
            .contains(&next.repeat_count)
        {
            return Err("Macro repeat count is outside editor limits".into());
        }
        *draft = Some(next);
        Ok(())
    }
    fn plan(&self, _: &Snapshot, draft: &Program) -> Result<Program, String> {
        if !self
            .capabilities
            .editable_repeat_counts
            .contains(&draft.repeat_count)
        {
            return Err("Stage a supported repeat count before saving".into());
        }
        validate_program(&self.capabilities, draft)?;
        Ok(draft.clone())
    }
}
impl Editor<MacroRules> {
    pub fn capabilities(&self) -> &Capabilities {
        &self.rules.capabilities
    }
    pub fn slot(&self) -> &str {
        &self.rules.slot
    }
    pub fn binding_action(&self, id: &str) -> Result<crate::model::keymap::Action, String> {
        if self.dirty() {
            return Err("Apply or revert macro changes before binding".into());
        }
        self.planned_binding_action(id)
    }
    pub(crate) fn planned_binding_action(
        &self,
        id: &str,
    ) -> Result<crate::model::keymap::Action, String> {
        self.ready()?;
        let program = self.draft().ok_or("Macro is not editable")?;
        if !self
            .capabilities()
            .editable_repeat_counts
            .contains(&program.repeat_count)
        {
            return Err("Stored macro count is outside the editable range".into());
        }
        let binding = self
            .capabilities()
            .bindings
            .iter()
            .find(|binding| binding.slot == self.slot() && binding.id == id)
            .ok_or("Unknown macro binding")?;
        if binding
            .required_repeat_count
            .is_some_and(|count| count != program.repeat_count)
        {
            return Err("Macro repeat count does not match this binding".into());
        }
        Ok(binding.action.clone())
    }
    pub fn select(&mut self, slot: &str) -> Result<(), String> {
        if !self
            .capabilities()
            .slots
            .iter()
            .any(|choice| choice.id == slot)
        {
            return Err("Unknown macro slot".into());
        }
        if self.slot() == slot {
            return Ok(());
        }
        if self.dirty() || self.submitted.is_some() {
            return Err("Finish or revert macro changes before changing slots".into());
        }
        self.rules.slot = slot.into();
        self.baseline = None;
        self.draft = None;
        self.status = Status::Unloaded;
        Ok(())
    }
    pub(crate) fn import(&mut self, target: &Snapshot) -> Result<(), String> {
        self.ready()?;
        self.rules.validate(target, Reception::Read)?;
        let baseline = self.baseline().ok_or("No macro baseline")?;
        if baseline.revision != target.revision {
            return Err("Macro file revision differs from the selected slot".into());
        }
        let Content::Editable(program) = &target.content else {
            return Err("Opaque macro files cannot be staged".into());
        };
        if self.draft().is_none() {
            return Err("Opaque macro slots cannot be converted".into());
        }
        if target.content != baseline.content
            && !self
                .capabilities()
                .editable_repeat_counts
                .contains(&program.repeat_count)
        {
            return Err("Macro file count is outside editor limits".into());
        }
        self.draft = Some(program.clone());
        Ok(())
    }
}
/// Rejection leaves the caller's draft untouched. Backend encoding validation
/// still runs before accepting/storing a draft that has a variable byte cost.
pub fn edit_program(
    capabilities: &Capabilities,
    program: &Program,
    edit: Edit,
) -> Result<Program, String> {
    let mut next = program.clone();
    match edit {
        Edit::Insert { at, event } if at <= next.events.len() => next.events.insert(at, event),
        Edit::Replace { at, event } if at < next.events.len() => next.events[at] = event,
        Edit::Remove { at } if at < next.events.len() => {
            next.events.remove(at);
        }
        Edit::Move { from, to } if from < next.events.len() && to < next.events.len() => {
            let event = next.events.remove(from);
            next.events.insert(to, event);
        }
        Edit::Repeat(count) => next.repeat_count = count,
        Edit::Clear => next.events.clear(),
        _ => return Err("Macro edit index is outside the event sequence".into()),
    }
    validate_program(capabilities, &next)?;
    Ok(next)
}

#[cfg(test)]
#[path = "../tests/editor_macros.rs"]
mod tests;
