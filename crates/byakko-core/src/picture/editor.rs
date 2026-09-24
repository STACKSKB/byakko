//! Deterministic picture draft and verified readback.
use super::{Capabilities, Channel, Content, Edit, Evidence, Snapshot, validate_snapshot};
use crate::draft::Draft;
use crate::session::ApplyFailure;
use std::collections::BTreeMap;

pub type Status = crate::draft::Status<Snapshot>;
type Colors = BTreeMap<String, [u8; 3]>;

fn editable(snapshot: &Snapshot) -> Option<&Colors> {
    match &snapshot.content {
        Content::Editable(colors) => Some(colors),
        Content::Opaque { .. } => None,
    }
}

pub struct Editor {
    capabilities: Capabilities,
    state: Draft<Snapshot, Colors>,
}

impl Editor {
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            capabilities,
            state: Draft::new(),
        }
    }
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }
    pub fn baseline(&self) -> Option<&Snapshot> {
        self.state.baseline()
    }
    pub fn draft(&self) -> Option<&Colors> {
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
            .filter(|(key, color)| original.get(*key) != Some(*color))
            .map(|(key, color)| Edit::Color {
                key: key.clone(),
                color: *color,
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
            return Err("Read and verify picture before editing".into());
        }
        let mut next = self.draft().ok_or("Picture is not editable")?.clone();
        match edit {
            Edit::Color { key, color } => *next.get_mut(&key).ok_or("Unknown picture key")? = color,
            Edit::Channel {
                key,
                channel,
                value,
            } => {
                let color = next.get_mut(&key).ok_or("Unknown picture key")?;
                color[match channel {
                    Channel::Red => 0,
                    Channel::Green => 1,
                    Channel::Blue => 2,
                }] = value;
            }
        }
        self.state.stage(next);
        Ok(())
    }
    pub fn revert(&mut self) -> Result<(), String> {
        self.state.revert(editable)
    }
    pub fn request_apply(&self) -> Result<(Snapshot, Colors), String> {
        self.state.request_apply(editable)
    }
    pub fn accept_read(&mut self, result: Result<Snapshot, String>) {
        self.state.accept_read(
            result,
            |snapshot| {
                validate_snapshot(&self.capabilities, snapshot)?;
                if snapshot.evidence != Evidence::Readback {
                    return Err("Picture read did not contain device readback".into());
                }
                Ok(())
            },
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
