//! Deterministic lighting draft and verified readback.
use super::{
    Capabilities, Content, Edit, Setting, Snapshot, default_setting, edit, validate_capabilities,
    validate_setting, validate_snapshot,
};
use crate::draft::Draft;
use crate::session::ApplyFailure;

pub type Status = crate::draft::Status<Snapshot>;

fn editable(snapshot: &Snapshot) -> Option<&Setting> {
    match &snapshot.content {
        Content::Editable(setting) => Some(setting),
        Content::HostActive { .. } | Content::Opaque { .. } => None,
    }
}

pub struct Editor {
    capabilities: Capabilities,
    state: Draft<Snapshot, Setting>,
}

impl Editor {
    fn host_active(&self) -> bool {
        matches!(
            self.baseline().map(|snapshot| &snapshot.content),
            Some(Content::HostActive { .. })
        )
    }

    pub fn new(capabilities: Capabilities) -> Result<Self, String> {
        validate_capabilities(&capabilities)?;
        Ok(Self {
            capabilities,
            state: Draft::new(),
        })
    }
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }
    pub fn baseline(&self) -> Option<&Snapshot> {
        self.state.baseline()
    }
    pub fn draft(&self) -> Option<&Setting> {
        self.state.draft()
    }
    pub fn status(&self) -> &Status {
        self.state.status()
    }
    pub fn dirty(&self) -> bool {
        self.state.dirty(editable)
    }
    pub fn invalidate(&mut self) {
        self.state.invalidate();
    }
    pub fn stage(&mut self, setting: Setting) -> Result<(), String> {
        if self.status() != &Status::Ready || (self.draft().is_none() && !self.host_active()) {
            return Err("Read an editable or active host lighting setting before editing".into());
        }
        validate_setting(&self.capabilities, &setting)?;
        self.state.stage(setting);
        Ok(())
    }
    pub fn edit(&mut self, change: Edit) -> Result<(), String> {
        if self.status() != &Status::Ready {
            return Err("Read and verify lighting before editing".into());
        }
        let next = match (self.draft(), change) {
            (Some(current), change) => edit(&self.capabilities, current, change)?,
            (None, Edit::Effect(id)) if self.host_active() => {
                default_setting(&self.capabilities, &id)?
            }
            _ => return Err("Lighting is not editable".into()),
        };
        self.state.stage(next);
        Ok(())
    }
    pub fn revert(&mut self) -> Result<(), String> {
        self.state.revert(editable)
    }
    pub fn request_apply(&self) -> Result<(Snapshot, Setting), String> {
        self.state.request_apply(editable)
    }
    pub fn accept_read(&mut self, result: Result<Snapshot, String>) {
        self.state.accept_read(
            result,
            |snapshot| validate_snapshot(&self.capabilities, snapshot),
            editable,
        );
    }
    pub fn accept_apply(&mut self, result: Result<Snapshot, ApplyFailure>) {
        self.state.accept_apply(
            result,
            |snapshot| validate_snapshot(&self.capabilities, snapshot),
            editable,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::{Effect, HostMode, HostSource};

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
                parameters: None,
            }],
        };
        let baseline = Snapshot {
            backend_id: "synthetic".into(),
            revision: vec![21],
            content: Content::HostActive {
                mode_id: "screen".into(),
            },
        };
        let mut editor = Editor::new(caps).unwrap();
        editor.accept_read(Ok(baseline.clone()));
        assert_eq!(editor.status(), &Status::Ready);
        assert!(editor.draft().is_none());
        assert!(editor.edit(Edit::Brightness(2)).is_err());
        editor.edit(Edit::Effect("steady".into())).unwrap();
        assert!(editor.dirty());
        assert_eq!(editor.request_apply().unwrap().0, baseline);
        editor.revert().unwrap();
        assert!(!editor.dirty());
        assert!(editor.draft().is_none());
    }
}
