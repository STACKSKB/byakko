//! Deterministic macro draft state. The outer session owns operation identity and I/O.

use super::{
    Capabilities, Content, Edit, Program, Snapshot, edit, validate_capabilities, validate_program,
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
    slot: String,
    baseline: Option<Snapshot>,
    draft: Option<Program>,
    status: Status,
}

impl Editor {
    pub fn new(capabilities: Capabilities) -> Result<Self, String> {
        validate_capabilities(&capabilities)?;
        let slot = capabilities.slots[0].id.clone();
        Ok(Self {
            capabilities,
            slot,
            baseline: None,
            draft: None,
            status: Status::Unloaded,
        })
    }

    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }
    pub fn slot(&self) -> &str {
        &self.slot
    }
    pub fn baseline(&self) -> Option<&Snapshot> {
        self.baseline.as_ref()
    }
    pub fn draft(&self) -> Option<&Program> {
        self.draft.as_ref()
    }
    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn dirty(&self) -> bool {
        match (&self.baseline, &self.draft) {
            (Some(snapshot), Some(draft)) => match &snapshot.content {
                Content::Editable(program) => program != draft,
                Content::Opaque { .. } => false,
            },
            _ => false,
        }
    }

    pub fn binding_action(&self, id: &str) -> Result<crate::Action, String> {
        if self.status != Status::Ready {
            return Err("Read and verify the macro before binding".into());
        }
        if self.dirty() {
            return Err("Apply or revert macro changes before binding".into());
        }
        let program = self.draft.as_ref().ok_or("Macro is not editable")?;
        let binding = self
            .capabilities
            .bindings
            .iter()
            .find(|binding| binding.slot == self.slot && binding.id == id)
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
            .capabilities
            .slots
            .iter()
            .any(|choice| choice.id == slot)
        {
            return Err("Unknown macro slot".into());
        }
        if self.slot == slot {
            return Ok(());
        }
        if self.dirty() {
            return Err("Revert macro changes before changing slots".into());
        }
        self.slot = slot.into();
        self.baseline = None;
        self.draft = None;
        self.status = Status::Unloaded;
        Ok(())
    }

    pub fn invalidate(&mut self) {
        self.status = Status::Unverified {
            problem: Problem::ReadRequired,
        };
    }

    pub fn edit(&mut self, change: Edit) -> Result<(), String> {
        if self.status != Status::Ready {
            return Err("Read and verify the macro before editing".into());
        }
        let current = self.draft.as_ref().ok_or("Macro is not editable")?;
        self.draft = Some(edit(&self.capabilities, current, change)?);
        Ok(())
    }

    pub fn revert(&mut self) -> Result<(), String> {
        let baseline = self.baseline.as_ref().ok_or("No macro baseline")?;
        self.draft = match &baseline.content {
            Content::Editable(program) => Some(program.clone()),
            Content::Opaque { .. } => None,
        };
        Ok(())
    }

    pub fn request_apply(&self) -> Result<(Snapshot, Program), String> {
        if self.status != Status::Ready {
            return Err("Read and verify the macro before applying".into());
        }
        if !self.dirty() {
            return Err("No macro changes are staged".into());
        }
        Ok((
            self.baseline.as_ref().ok_or("No macro baseline")?.clone(),
            self.draft
                .as_ref()
                .ok_or("No editable macro draft")?
                .clone(),
        ))
    }

    fn validate_snapshot(&self, snapshot: &Snapshot) -> Result<(), String> {
        if snapshot.backend_id != self.capabilities.backend_id || snapshot.slot != self.slot {
            return Err("Macro result belongs to a different backend or slot".into());
        }
        if let Content::Editable(program) = &snapshot.content {
            validate_program(&self.capabilities, program)?;
        }
        Ok(())
    }

    pub fn accept_read(&mut self, result: Result<Snapshot, String>) {
        let snapshot = match result.and_then(|snapshot| {
            self.validate_snapshot(&snapshot)?;
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
        let dirty = self.dirty();
        if dirty && self.baseline.as_ref() != Some(&snapshot) {
            self.status = Status::Conflict { device: snapshot };
            return;
        }
        if !dirty {
            self.draft = match &snapshot.content {
                Content::Editable(program) => Some(program.clone()),
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
        if let Err(reason) = self.validate_snapshot(&snapshot) {
            self.status = Status::Unverified {
                problem: Problem::InvalidApplyResult(reason),
            };
            return;
        }
        if !matches!((&snapshot.content, &self.draft),
            (Content::Editable(program), Some(draft)) if program == draft)
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

#[cfg(test)]
mod tests;
