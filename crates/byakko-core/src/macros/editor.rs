//! Deterministic macro draft state. The outer session owns operation identity and I/O.

use super::{
    Capabilities, Content, Edit, Program, Snapshot, edit, validate_capabilities, validate_program,
};
use crate::session::{ApplyFailure, Problem};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    catalog: Option<BTreeMap<String, Snapshot>>,
    catalog_error: Option<String>,
}

impl Editor {
    pub(crate) fn replace(&mut self, program: Program) -> Result<(), String> {
        if self.status != Status::Ready || self.draft.is_none() {
            return Err("Read an editable macro before importing".into());
        }
        validate_program(&self.capabilities, &program)?;
        self.draft = Some(program);
        Ok(())
    }

    pub(crate) fn recorder(
        &self,
        policy: super::recorder::DelayPolicy,
    ) -> Result<super::recorder::Recorder, String> {
        if self.status != Status::Ready {
            return Err("Read and verify the macro before recording".into());
        }
        super::recorder::Recorder::new(
            &self.capabilities,
            self.draft.as_ref().ok_or("Macro is not editable")?,
            policy,
        )
    }

    pub(crate) fn record(
        &mut self,
        recorder: &mut super::recorder::Recorder,
        action: super::Action,
        at: u64,
    ) -> Result<super::recorder::Transition, String> {
        recorder.transition(
            &self.capabilities,
            self.draft.as_mut().ok_or("Macro is not editable")?,
            action,
            at,
        )
    }

    pub(crate) fn stop_recording(
        &mut self,
        recorder: &super::recorder::Recorder,
        at: u64,
    ) -> Result<super::recorder::StopOutcome, String> {
        recorder.stop(
            &self.capabilities,
            self.draft.as_mut().ok_or("Macro is not editable")?,
            at,
        )
    }

    pub fn new(capabilities: Capabilities) -> Result<Self, String> {
        validate_capabilities(&capabilities)?;
        let slot = capabilities.slots[0].id.clone();
        Ok(Self {
            capabilities,
            slot,
            baseline: None,
            draft: None,
            status: Status::Unloaded,
            catalog: None,
            catalog_error: None,
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
    /// Complete, validated snapshots for every advertised slot in the current connection.
    pub fn catalog(&self) -> Option<&BTreeMap<String, Snapshot>> {
        self.catalog.as_ref()
    }
    pub fn catalog_error(&self) -> Option<&str> {
        self.catalog_error.as_deref()
    }
    pub fn configured_slots(&self) -> Option<Vec<&super::Choice>> {
        let catalog = self.catalog.as_ref()?;
        Some(
            self.capabilities
                .slots
                .iter()
                .filter(|choice| {
                    catalog
                        .get(&choice.id)
                        .is_some_and(|snapshot| match &snapshot.content {
                            Content::Editable(program) => !program.events.is_empty(),
                            Content::Opaque { .. } => true,
                        })
                })
                .collect(),
        )
    }
    pub(crate) fn accept_catalog(&mut self, result: Result<Vec<Snapshot>, String>) {
        let validated = result.and_then(|snapshots| {
            let mut catalog = BTreeMap::new();
            for snapshot in snapshots {
                let slot = snapshot.slot.clone();
                self.validate_snapshot_for(&snapshot, &slot)?;
                if catalog.insert(slot, snapshot).is_some() {
                    return Err("Macro catalog contains a duplicate slot".into());
                }
            }
            if catalog.len() != self.capabilities.slots.len() {
                return Err("Macro catalog is missing declared slots".into());
            }
            Ok(catalog)
        });
        match validated {
            Ok(catalog) => {
                self.catalog = Some(catalog);
                self.catalog_error = None;
            }
            Err(error) => {
                self.catalog = None;
                self.catalog_error = Some(error);
            }
        }
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
        if !self
            .capabilities
            .editable_repeat_counts
            .contains(&program.repeat_count)
        {
            return Err("Stored macro count is outside the editable range".into());
        }
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
        self.catalog = None;
        self.catalog_error = None;
        self.status = Status::Unverified {
            problem: Problem::ReadRequired,
        };
    }

    pub fn edit(&mut self, change: Edit) -> Result<(), String> {
        if self.status != Status::Ready {
            return Err("Read and verify the macro before editing".into());
        }
        let current = self.draft.as_ref().ok_or("Macro is not editable")?;
        if matches!(&change, Edit::Repeat(count) if !self.capabilities.editable_repeat_counts.contains(count))
        {
            return Err("Macro repeat count is outside editor limits".into());
        }
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
        let draft = self.draft.as_ref().ok_or("No editable macro draft")?;
        if !self
            .capabilities
            .editable_repeat_counts
            .contains(&draft.repeat_count)
        {
            return Err("Stage a supported repeat count before saving".into());
        }
        Ok((
            self.baseline.as_ref().ok_or("No macro baseline")?.clone(),
            draft.clone(),
        ))
    }

    fn validate_snapshot(&self, snapshot: &Snapshot) -> Result<(), String> {
        self.validate_snapshot_for(snapshot, &self.slot)
    }

    fn validate_snapshot_for(&self, snapshot: &Snapshot, slot: &str) -> Result<(), String> {
        if snapshot.backend_id != self.capabilities.backend_id
            || snapshot.slot != slot
            || !self
                .capabilities
                .slots
                .iter()
                .any(|choice| choice.id == slot)
        {
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
                self.catalog = None;
                self.catalog_error = Some(reason.clone());
                self.status = Status::Unverified {
                    problem: Problem::Read(reason),
                };
                return;
            }
        };
        if let Some(catalog) = self.catalog.as_mut() {
            catalog.insert(snapshot.slot.clone(), snapshot.clone());
        }
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
                self.catalog = None;
                self.catalog_error = Some(failure.message.clone());
                self.status = Status::Unverified {
                    problem: Problem::Apply(failure),
                };
                return;
            }
        };
        if let Err(reason) = self.validate_snapshot(&snapshot) {
            self.catalog = None;
            self.catalog_error = Some(reason.clone());
            self.status = Status::Unverified {
                problem: Problem::InvalidApplyResult(reason),
            };
            return;
        }
        if !matches!((&snapshot.content, &self.draft),
            (Content::Editable(program), Some(draft)) if program == draft)
        {
            self.catalog = None;
            self.catalog_error = Some("Macro readback did not match the staged program".into());
            self.status = Status::Unverified {
                problem: Problem::ApplyReadbackMismatch,
            };
            return;
        }
        if let Some(catalog) = self.catalog.as_mut() {
            catalog.insert(snapshot.slot.clone(), snapshot.clone());
        }
        self.baseline = Some(snapshot);
        self.status = Status::Ready;
    }
}

#[cfg(test)]
mod tests;
