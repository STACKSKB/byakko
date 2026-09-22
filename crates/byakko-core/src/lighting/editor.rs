//! Deterministic lighting draft and verified readback.
use super::{
    Capabilities, Content, Edit, Setting, Snapshot, edit, validate_capabilities, validate_setting,
    validate_snapshot,
};
use crate::draft::Draft;
use crate::session::ApplyFailure;

pub type Status = crate::draft::Status<Snapshot>;

fn editable(snapshot: &Snapshot) -> Option<&Setting> {
    match &snapshot.content {
        Content::Editable(setting) => Some(setting),
        Content::Opaque { .. } => None,
    }
}

pub struct Editor {
    capabilities: Capabilities,
    state: Draft<Snapshot, Setting>,
}

impl Editor {
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
        if self.status() != &Status::Ready || self.draft().is_none() {
            return Err("Read an editable lighting setting before editing".into());
        }
        validate_setting(&self.capabilities, &setting)?;
        self.state.stage(setting);
        Ok(())
    }
    pub fn edit(&mut self, change: Edit) -> Result<(), String> {
        if self.status() != &Status::Ready {
            return Err("Read and verify lighting before editing".into());
        }
        let current = self.draft().ok_or("Lighting is not editable")?;
        let next = edit(&self.capabilities, current, change)?;
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
