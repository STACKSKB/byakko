//! Deterministic lighting draft and verified readback.
use super::{
    Capabilities, Content, Edit, Setting, Snapshot, edit, validate_capabilities, validate_setting,
    validate_snapshot,
};
use crate::session::{ApplyFailure, Problem};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Status {
    Unloaded,
    Ready,
    Conflict { device: Snapshot },
    Unverified { problem: Problem },
}

pub struct Editor {
    capabilities: Capabilities,
    baseline: Option<Snapshot>,
    draft: Option<Setting>,
    status: Status,
}

impl Editor {
    pub fn new(capabilities: Capabilities) -> Result<Self, String> {
        validate_capabilities(&capabilities)?;
        Ok(Self {
            capabilities,
            baseline: None,
            draft: None,
            status: Status::Unloaded,
        })
    }
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }
    pub fn baseline(&self) -> Option<&Snapshot> {
        self.baseline.as_ref()
    }
    pub fn draft(&self) -> Option<&Setting> {
        self.draft.as_ref()
    }
    pub fn status(&self) -> &Status {
        &self.status
    }
    pub fn dirty(&self) -> bool {
        matches!((&self.baseline, &self.draft), (Some(Snapshot { content: Content::Editable(original), .. }), Some(draft)) if original != draft)
    }
    pub fn invalidate(&mut self) {
        self.status = Status::Unverified {
            problem: Problem::ReadRequired,
        };
    }
    pub fn stage(&mut self, setting: Setting) -> Result<(), String> {
        if self.status != Status::Ready || self.draft.is_none() {
            return Err("Read an editable lighting setting before editing".into());
        }
        validate_setting(&self.capabilities, &setting)?;
        self.draft = Some(setting);
        Ok(())
    }
    pub fn edit(&mut self, change: Edit) -> Result<(), String> {
        if self.status != Status::Ready {
            return Err("Read and verify lighting before editing".into());
        }
        let current = self.draft.as_ref().ok_or("Lighting is not editable")?;
        self.draft = Some(edit(&self.capabilities, current, change)?);
        Ok(())
    }
    pub fn revert(&mut self) -> Result<(), String> {
        let baseline = self.baseline.as_ref().ok_or("No lighting baseline")?;
        self.draft = match &baseline.content {
            Content::Editable(setting) => Some(setting.clone()),
            Content::Opaque { .. } => None,
        };
        Ok(())
    }
    pub fn request_apply(&self) -> Result<(Snapshot, Setting), String> {
        if self.status != Status::Ready {
            return Err("Read and verify lighting before applying".into());
        }
        if !self.dirty() {
            return Err("No lighting changes are staged".into());
        }
        Ok((
            self.baseline.as_ref().unwrap().clone(),
            self.draft.as_ref().unwrap().clone(),
        ))
    }
    pub fn accept_read(&mut self, result: Result<Snapshot, String>) {
        let snapshot = match result.and_then(|snapshot| {
            validate_snapshot(&self.capabilities, &snapshot)?;
            Ok(snapshot)
        }) {
            Ok(snapshot) => snapshot,
            Err(reason) => {
                self.status = Status::Unverified {
                    problem: Problem::Read(reason),
                };
                return;
            }
        };
        if self.dirty() && self.baseline.as_ref() != Some(&snapshot) {
            self.status = Status::Conflict { device: snapshot };
            return;
        }
        if !self.dirty() {
            self.draft = match &snapshot.content {
                Content::Editable(setting) => Some(setting.clone()),
                Content::Opaque { .. } => None,
            };
        }
        self.baseline = Some(snapshot);
        self.status = Status::Ready;
    }
    pub fn accept_apply(&mut self, result: Result<Snapshot, ApplyFailure>) {
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(failure) => {
                self.status = Status::Unverified {
                    problem: Problem::Apply(failure),
                };
                return;
            }
        };
        if let Err(reason) = validate_snapshot(&self.capabilities, &snapshot) {
            self.status = Status::Unverified {
                problem: Problem::InvalidApplyResult(reason),
            };
            return;
        }
        if !matches!((&snapshot.content, &self.draft), (Content::Editable(setting), Some(draft)) if setting == draft)
        {
            self.status = Status::Unverified {
                problem: Problem::ApplyReadbackMismatch,
            };
            return;
        }
        self.baseline = Some(snapshot);
        self.status = Status::Ready;
    }
}
