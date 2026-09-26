//! Lighting rules preserve readback versus transport-acceptance evidence.
use super::{Editor, Feature, Reception};
use crate::{
    model::lighting::*,
    validation::lighting::{
        validate_capabilities, validate_parameters, validate_setting, validate_snapshot,
    },
};
pub struct LightingRules {
    capabilities: Capabilities,
}
impl LightingRules {
    pub fn new(capabilities: Capabilities) -> Result<Self, String> {
        validate_capabilities(&capabilities)?;
        Ok(Self { capabilities })
    }
}
impl Feature for LightingRules {
    type Snapshot = Snapshot;
    type Value = Setting;
    type Edit = Edit;
    type Write = Setting;
    fn same_baseline(left: &Snapshot, right: &Snapshot) -> bool {
        left.backend_id == right.backend_id
            && left.revision == right.revision
            && left.picture_context == right.picture_context
            && left.content == right.content
    }
    fn value(snapshot: &Snapshot) -> Option<&Setting> {
        match &snapshot.content {
            Content::Editable(setting) => Some(setting),
            _ => None,
        }
    }
    fn validate(&self, snapshot: &Snapshot, reception: Reception) -> Result<(), String> {
        validate_snapshot(&self.capabilities, snapshot)?;
        if matches!(reception, Reception::Read) && snapshot.evidence != Evidence::Readback {
            return Err("Lighting read did not contain device readback".into());
        }
        Ok(())
    }
    fn edit(
        &self,
        baseline: &Snapshot,
        draft: &mut Option<Setting>,
        _: Option<&Setting>,
        change: Edit,
    ) -> Result<(), String> {
        let next = match (draft.as_ref(), change) {
            (Some(current), change) => edit_setting(&self.capabilities, current, change)?,
            (None, Edit::Effect(id)) if matches!(baseline.content, Content::HostActive { .. }) => {
                default_setting(&self.capabilities, &id)?
            }
            _ => return Err("Lighting is not editable".into()),
        };
        *draft = Some(next);
        Ok(())
    }
    fn plan(&self, _: &Snapshot, draft: &Setting) -> Result<Setting, String> {
        validate_setting(&self.capabilities, draft)?;
        Ok(draft.clone())
    }
}
impl Editor<LightingRules> {
    pub(crate) fn accept_host_restoration(
        &mut self,
        snapshot: Snapshot,
    ) -> Result<(), crate::contract::ApplyFailure> {
        use crate::contract::{ApplyFailure, Recovery};
        self.rules
            .validate(&snapshot, Reception::Read)
            .map_err(|message| ApplyFailure {
                message,
                recovery: Recovery::Unverified,
            })?;
        if self
            .baseline()
            .is_none_or(|baseline| !LightingRules::same_baseline(baseline, &snapshot))
        {
            return Err(ApplyFailure {
                message: "Host lighting restoration differs from its original baseline".into(),
                recovery: Recovery::Failed,
            });
        }
        self.accept_read(Ok(snapshot));
        Ok(())
    }
    pub fn capabilities(&self) -> &Capabilities {
        &self.rules.capabilities
    }
    pub fn stage(&mut self, setting: Setting) -> Result<(), String> {
        self.ready()?;
        if self.draft().is_none()
            && !matches!(
                self.baseline().map(|snapshot| &snapshot.content),
                Some(Content::HostActive { .. })
            )
        {
            return Err("Lighting is not editable".into());
        }
        validate_setting(self.capabilities(), &setting)?;
        self.draft = Some(setting);
        Ok(())
    }
}
pub fn edit_setting(
    caps: &Capabilities,
    current: &Setting,
    change: Edit,
) -> Result<Setting, String> {
    validate_setting(caps, current)?;
    match change {
        Edit::Effect(id) if id != current.effect => default_setting(caps, &id),
        change => edit_parameters(
            caps.effects
                .iter()
                .find(|effect| effect.id == current.effect)
                .expect("validated effect"),
            current,
            change,
        ),
    }
}

pub fn edit_parameters(
    effect: &Effect,
    current: &Setting,
    change: Edit,
) -> Result<Setting, String> {
    validate_parameters(effect, current)?;
    let mut next = current.clone();
    match change {
        Edit::Effect(id) if id != current.effect => {
            return Err("Select a mode before editing its parameters".into());
        }
        Edit::Effect(_) => {}
        Edit::Brightness(value) => next.brightness = Some(value),
        Edit::Speed(value) => next.speed = Some(value),
        Edit::Option(id) => next.option = Some(id),
        Edit::Color(color) => next.color = Some(color),
        Edit::Channel(channel, value) => {
            let Some(Color::Rgb(rgb)) = next.color.as_mut() else {
                return Err("Lighting color is not fixed RGB".into());
            };
            rgb[match channel {
                Channel::Red => 0,
                Channel::Green => 1,
                Channel::Blue => 2,
            }] = value;
        }
    }
    validate_parameters(effect, &next)?;
    Ok(next)
}
pub fn default_setting(caps: &Capabilities, effect_id: &str) -> Result<Setting, String> {
    validate_capabilities(caps)?;
    let effect = caps
        .effects
        .iter()
        .find(|effect| effect.id == effect_id)
        .ok_or("Unknown lighting effect")?;
    Ok(default_parameters(effect))
}

pub fn default_parameters(effect: &Effect) -> Setting {
    Setting {
        effect: effect.id.clone(),
        brightness: effect.brightness.as_ref().map(|range| *range.end()),
        speed: effect.speed.as_ref().map(|range| *range.start()),
        option: effect.options.first().map(|choice| choice.id.clone()),
        color: effect.color.as_ref().map(|capability| match capability {
            ColorCapability::Rainbow => Color::Rainbow,
            _ => Color::Rgb([255; 3]),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Status;
    use crate::model::lighting::{Effect, HostMode, HostSource};

    #[test]
    fn known_host_mode_can_be_explicitly_replaced_with_an_onboard_effect() {
        let caps = Capabilities {
            backend_id: "synthetic".into(),
            effects: vec![Effect {
                id: "steady".into(),
                label: "Steady".into(),
                brightness: None,
                speed: None,
                options: vec![],
                color: None,
            }],
            host_modes: vec![HostMode {
                id: "screen".into(),
                label: "Screen".into(),
                source: HostSource::ScreenAverage,
                requires_enabled_setting: None,
                parameters: None,
            }],
        };
        let baseline = Snapshot {
            backend_id: "synthetic".into(),
            revision: vec![21],
            picture_context: vec![],
            evidence: Evidence::Readback,
            content: Content::HostActive {
                mode_id: "screen".into(),
            },
        };
        let mut editor = Editor::new(LightingRules::new(caps).unwrap());
        editor.accept_read(Ok(baseline.clone()));
        assert_eq!(editor.status(), &Status::Ready);
        assert!(editor.draft().is_none());
        assert!(editor.edit(Edit::Brightness(2)).is_err());
        editor.edit(Edit::Effect("steady".into())).unwrap();
        assert!(editor.dirty());
        assert_eq!(editor.request_apply().unwrap().0, baseline);
        editor.cancel_apply();
        editor.revert().unwrap();
        assert!(!editor.dirty());
        assert!(editor.draft().is_none());
    }
}
