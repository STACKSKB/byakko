//! One-field settings draft and verified readback.
use super::{
    Capabilities, Content, Edit, Snapshot, Value, validate_capabilities, validate_snapshot,
    validate_value,
};
use crate::{draft::Draft, session::ApplyFailure};
use std::collections::BTreeMap;

pub type Status = crate::draft::Status<Snapshot>;
type Values = BTreeMap<String, Value>;

fn editable(snapshot: &Snapshot) -> Option<&Values> {
    match &snapshot.content {
        Content::Editable(values) => Some(values),
        Content::Opaque { .. } => None,
    }
}

pub struct Editor {
    capabilities: Capabilities,
    state: Draft<Snapshot, Values>,
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
    pub fn draft(&self) -> Option<&Values> {
        self.state.draft()
    }
    pub fn status(&self) -> &Status {
        self.state.status()
    }
    pub fn changes(&self) -> Vec<Edit> {
        let (
            Some(Snapshot {
                content: Content::Editable(original),
                ..
            }),
            Some(draft),
        ) = (self.baseline(), self.draft())
        else {
            return Vec::new();
        };
        draft
            .iter()
            .filter(|(id, value)| original.get(*id) != Some(*value))
            .map(|(id, value)| Edit {
                id: id.clone(),
                value: value.clone(),
            })
            .collect()
    }
    pub fn dirty(&self) -> bool {
        self.state.dirty(editable)
    }
    pub fn invalidate(&mut self) {
        self.state.invalidate();
    }
    pub fn edit(&mut self, edit: Edit) -> Result<(), String> {
        if self.status() != &Status::Ready {
            return Err("Read and verify settings before editing".into());
        }
        validate_value(&self.capabilities, &edit)?;
        let staged = self.changes();
        if staged.first().is_some_and(|change| change.id != edit.id) {
            return Err("Apply or revert the staged setting before editing another field".into());
        }
        let mut next = self.draft().ok_or("Settings are not editable")?.clone();
        *next
            .get_mut(&edit.id)
            .ok_or("Settings draft is missing a field")? = edit.value;
        self.state.stage(next);
        Ok(())
    }
    pub fn revert(&mut self) -> Result<(), String> {
        self.state.revert(editable)
    }
    pub fn request_apply(&self) -> Result<(Snapshot, Edit), String> {
        if self.status() != &Status::Ready {
            return Err("Read and verify settings before applying".into());
        }
        let [edit] = self
            .changes()
            .try_into()
            .map_err(|_| "Exactly one setting change is required")?;
        Ok((self.baseline().unwrap().clone(), edit))
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
