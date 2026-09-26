//! Picture rules retain complete advertised maps and selector context.
use super::{Editor, Feature, Reception};
use crate::{model::picture::*, validation::picture::validate_snapshot};
use std::collections::BTreeMap;
pub type Colors = BTreeMap<String, [u8; 3]>;
pub struct PictureRules {
    capabilities: Capabilities,
}
impl PictureRules {
    pub fn new(capabilities: Capabilities) -> Self {
        Self { capabilities }
    }
}
impl Feature for PictureRules {
    type Snapshot = Snapshot;
    type Value = Colors;
    type Edit = Edit;
    type Write = Colors;
    fn same_baseline(left: &Snapshot, right: &Snapshot) -> bool {
        left.backend_id == right.backend_id
            && left.revision == right.revision
            && left.context_revision == right.context_revision
            && left.content == right.content
    }
    fn value(snapshot: &Snapshot) -> Option<&Colors> {
        match &snapshot.content {
            Content::Editable(colors) => Some(colors),
            Content::Opaque { .. } => None,
        }
    }
    fn validate(&self, snapshot: &Snapshot, reception: Reception) -> Result<(), String> {
        validate_snapshot(&self.capabilities, snapshot)?;
        if matches!(reception, Reception::Read) && snapshot.evidence != Evidence::Readback {
            return Err("Picture read did not contain device readback".into());
        }
        Ok(())
    }
    fn edit(
        &self,
        _: &Snapshot,
        draft: &mut Option<Colors>,
        _: Option<&Colors>,
        edit: Edit,
    ) -> Result<(), String> {
        let colors = draft.as_mut().ok_or("Picture is not editable")?;
        match edit {
            Edit::Color { key, color } => {
                *colors.get_mut(&key).ok_or("Unknown picture key")? = color
            }
            Edit::Channel {
                key,
                channel,
                value,
            } => {
                let color = colors.get_mut(&key).ok_or("Unknown picture key")?;
                color[match channel {
                    Channel::Red => 0,
                    Channel::Green => 1,
                    Channel::Blue => 2,
                }] = value;
            }
        }
        Ok(())
    }
    fn plan(&self, _: &Snapshot, draft: &Colors) -> Result<Colors, String> {
        Ok(draft.clone())
    }
}
impl Editor<PictureRules> {
    pub fn import(&mut self, target: &Snapshot) -> Result<(), String> {
        self.ready()?;
        validate_snapshot(self.capabilities(), target)?;
        let baseline = self.baseline().ok_or("Read the picture before importing")?;
        if target.revision != baseline.revision
            || target.context_revision != baseline.context_revision
        {
            return Err("Picture import does not match the selected baseline and context".into());
        }
        if self.draft().is_none() {
            return Err("Picture is not editable".into());
        }
        let Content::Editable(colors) = &target.content else {
            return Err("Picture import is not editable".into());
        };
        self.draft = Some(colors.clone());
        Ok(())
    }
    pub fn capabilities(&self) -> &Capabilities {
        &self.rules.capabilities
    }
    pub fn changes(&self) -> Vec<Edit> {
        match (self.baseline().and_then(PictureRules::value), self.draft()) {
            (Some(original), Some(draft)) => draft
                .iter()
                .filter(|(key, color)| original.get(*key) != Some(*color))
                .map(|(key, color)| Edit::Color {
                    key: key.clone(),
                    color: *color,
                })
                .collect(),
            _ => vec![],
        }
    }
}
