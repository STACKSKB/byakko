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
    fn edit(&self, _: &Snapshot, draft: &mut Option<Colors>, edit: Edit) -> Result<(), String> {
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
