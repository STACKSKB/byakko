//! Scalar settings use a whole baseline and one-field writes.
use super::{Editor, Feature, Reception};
use crate::{
    model::settings::*,
    validation::settings::{validate_capabilities, validate_snapshot, validate_value},
};
use std::collections::BTreeMap;
pub type Values = BTreeMap<String, Value>;
pub struct SettingsRules {
    capabilities: Capabilities,
}
impl SettingsRules {
    pub fn new(capabilities: Capabilities) -> Result<Self, String> {
        validate_capabilities(&capabilities)?;
        Ok(Self { capabilities })
    }
}
fn changes(original: &Values, draft: &Values) -> Vec<Edit> {
    draft
        .iter()
        .filter(|(id, value)| original.get(*id) != Some(*value))
        .map(|(id, value)| Edit {
            id: id.clone(),
            value: value.clone(),
        })
        .collect()
}
impl Feature for SettingsRules {
    type Snapshot = Snapshot;
    type Value = Values;
    type Edit = Edit;
    type Write = Edit;
    fn value(snapshot: &Snapshot) -> Option<&Values> {
        match &snapshot.content {
            Content::Editable(values) => Some(values),
            Content::Opaque { .. } => None,
        }
    }
    fn validate(&self, snapshot: &Snapshot, _: Reception) -> Result<(), String> {
        validate_snapshot(&self.capabilities, snapshot)
    }
    fn edit(
        &self,
        baseline: &Snapshot,
        draft: &mut Option<Values>,
        submitted: Option<&Values>,
        edit: Edit,
    ) -> Result<(), String> {
        validate_value(&self.capabilities, &edit)?;
        let original = Self::value(baseline).ok_or("Settings are not editable")?;
        if submitted.is_some_and(|values| {
            changes(original, values)
                .first()
                .is_some_and(|change| change.id != edit.id)
        }) {
            return Err("Wait for the submitted setting before editing another field".into());
        }
        let draft = draft.as_mut().ok_or("Settings are not editable")?;
        if changes(
            Self::value(baseline).ok_or("Settings are not editable")?,
            draft,
        )
        .first()
        .is_some_and(|change| change.id != edit.id)
        {
            return Err("Apply or revert the staged setting before editing another field".into());
        }
        *draft
            .get_mut(&edit.id)
            .ok_or("Settings draft is missing a field")? = edit.value;
        Ok(())
    }
    fn plan(&self, baseline: &Snapshot, draft: &Values) -> Result<Edit, String> {
        let [edit] = changes(
            Self::value(baseline).ok_or("Settings are not editable")?,
            draft,
        )
        .try_into()
        .map_err(|_| "Exactly one setting change is required")?;
        Ok(edit)
    }
}
impl Editor<SettingsRules> {
    pub fn capabilities(&self) -> &Capabilities {
        &self.rules.capabilities
    }
    pub fn changes(&self) -> Vec<Edit> {
        match (self.baseline().and_then(SettingsRules::value), self.draft()) {
            (Some(original), Some(draft)) => changes(original, draft),
            _ => vec![],
        }
    }
}
