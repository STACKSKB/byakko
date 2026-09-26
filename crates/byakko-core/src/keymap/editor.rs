//! Owns the keymap baseline and staged bindings, independently of connection.
use super::{Action, Change, Descriptor, State, validate_changes, validate_state};
use crate::{
    contract::{ApplyFailure, Problem},
    draft::Draft,
};
use std::collections::BTreeMap;
pub type Bindings = BTreeMap<String, BTreeMap<String, Action>>;
pub type Status = crate::draft::Status<State>;
pub struct Editor {
    values: Draft<State, Bindings>,
}
fn bindings(state: &State) -> Option<&Bindings> {
    Some(&state.bindings)
}
impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
impl Editor {
    pub fn new() -> Self {
        Self {
            values: Draft::new(),
        }
    }
    pub fn baseline(&self) -> Option<&State> {
        self.values.baseline()
    }
    pub fn draft(&self) -> Option<&Bindings> {
        self.values.draft()
    }
    pub fn status(&self) -> &Status {
        self.values.status()
    }
    pub fn dirty(&self) -> bool {
        self.values.dirty(bindings)
    }
    pub fn changes(&self) -> Vec<Change> {
        let (Some(baseline), Some(draft)) = (self.baseline(), self.draft()) else {
            return Vec::new();
        };
        draft
            .iter()
            .flat_map(|(layer, values)| {
                values
                    .iter()
                    .filter(|(key, action)| {
                        baseline
                            .bindings
                            .get(layer)
                            .and_then(|values| values.get(*key))
                            != Some(*action)
                    })
                    .map(|(key, action)| Change {
                        layer: layer.clone(),
                        key: key.clone(),
                        action: action.clone(),
                    })
            })
            .collect()
    }
    pub(crate) fn edit(&mut self, descriptor: &Descriptor, change: Change) -> Result<(), String> {
        if self.status() != &Status::Ready {
            return Err("Read and verify before editing".into());
        }
        validate_edit(descriptor, &change)?;
        self.stage(change)
    }
    /// Macro capability validation owns the binding action; keymap owns its target.
    pub(crate) fn bind_macro(
        &mut self,
        descriptor: &Descriptor,
        change: Change,
    ) -> Result<(), String> {
        if self.status() != &Status::Ready {
            return Err("Read and verify before assigning a macro".into());
        }
        validate_changes(descriptor, std::slice::from_ref(&change))?;
        self.stage(change)
    }
    fn stage(&mut self, change: Change) -> Result<(), String> {
        self.values.update(|draft| {
            *draft
                .get_mut(&change.layer)
                .and_then(|values| values.get_mut(&change.key))
                .ok_or("Missing binding")? = change.action;
            Ok(())
        })
    }
    pub(crate) fn revert(&mut self) -> Result<(), String> {
        self.values.revert(bindings)
    }
    pub(crate) fn save(&self, descriptor: &Descriptor) -> Result<(State, Vec<Change>), String> {
        if self.status() != &Status::Ready {
            return Err("Read and verify before applying".into());
        }
        let changes = self.changes();
        if changes.is_empty() {
            return Err("No keymap changes are staged".into());
        }
        validate_changes(descriptor, &changes)?;
        Ok((
            self.baseline().ok_or("No keymap baseline")?.clone(),
            changes,
        ))
    }
    pub(crate) fn invalidate(&mut self) {
        if matches!(self.status(), Status::Ready) {
            self.values.invalidate();
        }
    }
    pub(crate) fn read(&mut self, descriptor: &Descriptor, result: Result<State, String>) {
        self.values
            .accept_read(result, |state| validate_state(descriptor, state), bindings);
    }
    pub(crate) fn applied(&mut self, descriptor: &Descriptor, result: Result<State, ApplyFailure>) {
        self.values
            .accept_apply(result, |state| validate_state(descriptor, state), bindings);
    }
    pub(crate) fn problem(&self) -> Option<Problem> {
        match self.status() {
            Status::Unverified { problem } => Some(problem.clone()),
            _ => None,
        }
    }
}

/// Validate a portable key edit against the backend's advertised constraints.
pub fn validate_edit(descriptor: &Descriptor, change: &Change) -> Result<(), String> {
    validate_changes(descriptor, std::slice::from_ref(change))?;
    match &change.action {
        Action::Opaque { .. } => return Err("Opaque bindings cannot be programmed".into()),
        Action::Shortcut { modifiers, key } => {
            descriptor
                .shortcuts
                .as_ref()
                .ok_or("Shortcuts are not supported")?
                .compose(modifiers, *key)?;
        }
        action
            if !descriptor
                .actions
                .iter()
                .any(|choice| &choice.action == action) =>
        {
            return Err("Action is not advertised".into());
        }
        _ => {}
    }
    Ok(())
}
