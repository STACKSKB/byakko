//! Occupancy knowledge is separate from the selected slot's editable snapshot.
use super::{Content, Snapshot};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Occupancy {
    Unknown,
    Empty,
    Configured,
    Opaque,
}
pub struct Library {
    slots: BTreeMap<String, Occupancy>,
    error: Option<String>,
}
impl Library {
    pub(crate) fn new(slots: impl Iterator<Item = String>) -> Self {
        Self {
            slots: slots.map(|slot| (slot, Occupancy::Unknown)).collect(),
            error: None,
        }
    }
    pub fn occupancy(&self, slot: &str) -> Option<&Occupancy> {
        self.slots.get(slot)
    }
    pub fn slots(&self) -> &BTreeMap<String, Occupancy> {
        &self.slots
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub(crate) fn candidate(
        &self,
        capabilities: &super::Capabilities,
        bindings: &crate::keymap::Bindings,
    ) -> Result<String, String> {
        for occupancy in [Occupancy::Empty, Occupancy::Unknown] {
            for slot in &capabilities.slots {
                let bound = capabilities
                    .bindings
                    .iter()
                    .filter(|binding| binding.slot == slot.id)
                    .any(|binding| {
                        bindings
                            .values()
                            .any(|keys| keys.values().any(|action| action == &binding.action))
                    });
                if !bound && self.occupancy(&slot.id) == Some(&occupancy) {
                    return Ok(slot.id.clone());
                }
            }
        }
        Err("No unbound empty or unknown macro slot is available".into())
    }
    pub(crate) fn observe(&mut self, snapshot: &Snapshot) {
        if let Some(occupancy) = self.slots.get_mut(&snapshot.slot) {
            *occupancy = match &snapshot.content {
                Content::Editable(program) if program.events.is_empty() => Occupancy::Empty,
                Content::Editable(_) => Occupancy::Configured,
                Content::Opaque { .. } => Occupancy::Opaque,
            };
        }
    }
    pub(crate) fn invalidate(&mut self) {
        self.slots
            .values_mut()
            .for_each(|slot| *slot = Occupancy::Unknown);
        self.error = None;
    }
    pub(crate) fn forget(&mut self, slot: &str) {
        if let Some(occupancy) = self.slots.get_mut(slot) {
            *occupancy = Occupancy::Unknown;
        }
    }
    pub(crate) fn failed(&mut self, error: String) {
        self.error = Some(error);
    }
    pub(crate) fn clear_error(&mut self) {
        self.error = None;
    }
}
