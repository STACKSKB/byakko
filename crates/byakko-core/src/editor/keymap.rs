//! Keymap constraints and sparse write projection.
use super::{Editor, Feature, Reception};
use crate::{
    model::keymap::*,
    validation::keymap::{validate_changes, validate_edit, validate_state},
};
pub struct KeymapRules {
    descriptor: Descriptor,
}
impl KeymapRules {
    pub fn new(descriptor: Descriptor) -> Result<Self, String> {
        let bindings = descriptor
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
                revision: vec![],
                bindings,
            },
        )?;
        Ok(Self { descriptor })
    }
    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
}
fn changes(baseline: &State, draft: &Bindings) -> Vec<Change> {
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
fn stage(draft: &mut Option<Bindings>, change: Change) -> Result<(), String> {
    *draft
        .as_mut()
        .ok_or("Keymap is not editable")?
        .get_mut(&change.layer)
        .and_then(|values| values.get_mut(&change.key))
        .ok_or("Missing binding")? = change.action;
    Ok(())
}
impl Feature for KeymapRules {
    type Snapshot = State;
    type Value = Bindings;
    type Edit = Change;
    type Write = Vec<Change>;
    fn value(snapshot: &State) -> Option<&Bindings> {
        Some(&snapshot.bindings)
    }
    fn validate(&self, snapshot: &State, _: Reception) -> Result<(), String> {
        validate_state(&self.descriptor, snapshot)
    }
    fn edit(
        &self,
        _: &State,
        draft: &mut Option<Bindings>,
        _: Option<&Bindings>,
        change: Change,
    ) -> Result<(), String> {
        validate_edit(&self.descriptor, &change)?;
        stage(draft, change)
    }
    fn plan(&self, baseline: &State, draft: &Bindings) -> Result<Vec<Change>, String> {
        let changes = changes(baseline, draft);
        validate_changes(&self.descriptor, &changes)?;
        Ok(changes)
    }
}
impl Editor<KeymapRules> {
    pub fn changes(&self) -> Vec<Change> {
        match (self.baseline(), self.draft()) {
            (Some(baseline), Some(draft)) => changes(baseline, draft),
            _ => vec![],
        }
    }
    pub(crate) fn bind_macro(&mut self, change: Change) -> Result<(), String> {
        self.ready()?;
        validate_changes(self.rules.descriptor(), std::slice::from_ref(&change))?;
        stage(&mut self.draft, change)
    }
}
