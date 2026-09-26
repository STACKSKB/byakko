//! Occupancy summaries and passive discovery; no editable snapshots are retained.
use crate::{
    contract::{Completion, CompletionPayload, Problem, Recovery},
    editor::{Editor, Status, macros::MacroRules},
    model::keymap::Bindings,
    model::macros::{Capabilities, Content, Snapshot},
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Occupancy {
    Unknown,
    Empty,
    Configured,
    Opaque,
}
struct Scan {
    generation: u64,
    operation: u64,
    refreshed: BTreeSet<String>,
}
pub struct Library {
    slots: BTreeMap<String, Occupancy>,
    error: Option<String>,
    scan: Option<Scan>,
}
impl Library {
    pub(crate) fn new(slots: impl Iterator<Item = String>) -> Self {
        Self {
            slots: slots.map(|slot| (slot, Occupancy::Unknown)).collect(),
            error: None,
            scan: None,
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
    pub fn scanning(&self) -> bool {
        self.scan.is_some()
    }
    pub(crate) fn cancel(&mut self) {
        self.scan = None;
    }
    pub(crate) fn begin_scan(&mut self, generation: u64, operation: u64) {
        self.scan = Some(Scan {
            generation,
            operation,
            refreshed: BTreeSet::new(),
        });
    }
    pub(crate) fn candidate(
        &self,
        capabilities: &Capabilities,
        bindings: &Bindings,
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
    fn observe(&mut self, snapshot: &Snapshot) {
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
        self.cancel();
    }
    pub(crate) fn accept_scan(
        &mut self,
        completion: &Completion,
        rules: &MacroRules,
    ) -> Option<Result<(), String>> {
        let scan = self.scan.as_ref()?;
        if scan.generation != completion.generation || scan.operation != completion.operation {
            return None;
        }
        let CompletionPayload::ReadMacroCatalog { result } = &completion.payload else {
            return None;
        };
        let scan = self.scan.take().expect("matched passive discovery ticket");
        let result = result.as_ref().map_err(Clone::clone).and_then(|snapshots| {
            let mut seen = BTreeSet::new();
            for snapshot in snapshots {
                rules.validate_snapshot_for(snapshot, &snapshot.slot)?;
                if !seen.insert(snapshot.slot.as_str()) {
                    return Err("Duplicate macro catalog slot".into());
                }
            }
            if seen.len() != self.slots.len() {
                return Err("Incomplete macro catalog".into());
            }
            Ok(snapshots)
        });
        match result {
            Ok(snapshots) => {
                for snapshot in snapshots {
                    if !scan.refreshed.contains(&snapshot.slot) {
                        self.observe(snapshot);
                    }
                }
                self.error = None;
                Some(Ok(()))
            }
            Err(error) => {
                self.error = Some(error.clone());
                Some(Err(error))
            }
        }
    }
    pub(crate) fn observe_editor(&mut self, editor: &Editor<MacroRules>) -> Result<(), Problem> {
        let snapshot = match editor.status() {
            Status::Ready => editor.baseline().expect("ready editor owns baseline"),
            Status::Conflict { device } => device,
            Status::Unverified { problem } => {
                let uncertain = match problem {
                    Problem::Apply(failure) => {
                        matches!(failure.recovery, Recovery::Failed | Recovery::Unverified)
                    }
                    Problem::InvalidApplyResult(_) | Problem::ApplyReadbackMismatch => true,
                    _ => false,
                };
                if uncertain && let Some(occupancy) = self.slots.get_mut(editor.slot()) {
                    *occupancy = Occupancy::Unknown;
                }
                return Err(problem.clone());
            }
            Status::Unloaded => unreachable!("completion establishes a status"),
        };
        self.observe(snapshot);
        if let Some(scan) = &mut self.scan {
            scan.refreshed.insert(snapshot.slot.clone());
        }
        Ok(())
    }
}
