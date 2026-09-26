//! Selected editor, occupancy library and passive discovery correlation.
use crate::{
    contract::{Completion, CompletionPayload, FeatureResult, Problem, Recovery},
    macros::{
        Capabilities, Snapshot,
        editor::{Editor, Status},
        library::Library,
    },
};
use std::collections::BTreeSet;
pub(super) struct Scan {
    pub generation: u64,
    pub operation: u64,
    refreshed: BTreeSet<String>,
}
pub(super) struct Macros {
    pub editor: Editor,
    pub library: Library,
    pub scan: Option<Scan>,
}
impl Macros {
    pub fn new(capabilities: Capabilities) -> Result<Self, String> {
        let library = Library::new(capabilities.slots.iter().map(|slot| slot.id.clone()));
        Ok(Self {
            editor: Editor::new(capabilities)?,
            library,
            scan: None,
        })
    }
    pub fn invalidate(&mut self) {
        self.editor.invalidate();
        self.library.invalidate();
        self.scan = None;
    }
    pub fn begin_scan(&mut self, generation: u64, operation: u64) {
        self.scan = Some(Scan {
            generation,
            operation,
            refreshed: BTreeSet::new(),
        });
    }
    pub fn accept_scan(&mut self, completion: &Completion) -> Option<Result<(), String>> {
        let scan = self.scan.as_ref()?;
        if scan.generation != completion.generation || scan.operation != completion.operation {
            return None;
        }
        let CompletionPayload::ReadMacroCatalog { result } = &completion.payload else {
            return None;
        };
        let result = result.as_ref().map_err(Clone::clone).and_then(|snapshots| {
            let mut seen = BTreeSet::new();
            for snapshot in snapshots {
                self.editor
                    .validate_snapshot_for(snapshot, &snapshot.slot)?;
                if !seen.insert(snapshot.slot.as_str()) {
                    return Err("Duplicate macro catalog slot".into());
                }
            }
            if seen.len() != self.editor.capabilities().slots.len() {
                return Err("Incomplete macro catalog".into());
            }
            Ok(snapshots)
        });
        match result {
            Ok(snapshots) => {
                for snapshot in snapshots {
                    if !scan.refreshed.contains(&snapshot.slot) {
                        self.library.observe(snapshot);
                    }
                }
                self.library.clear_error();
                self.scan = None;
                Some(Ok(()))
            }
            Err(error) => {
                self.library.failed(error.clone());
                self.scan = None;
                Some(Err(error))
            }
        }
    }
    pub fn accept(&mut self, result: FeatureResult<Snapshot>) -> Result<(), Problem> {
        match result {
            FeatureResult::Read(result) => self.editor.accept_read(result),
            FeatureResult::Apply(result) => self.editor.accept_apply(result),
        }
        let snapshot = match self.editor.status() {
            Status::Ready => self.editor.baseline().expect("ready macro owns a baseline"),
            Status::Conflict { device } => device,
            Status::Unverified { problem } => {
                let uncertain = match problem {
                    Problem::Apply(failure) => {
                        matches!(failure.recovery, Recovery::Failed | Recovery::Unverified)
                    }
                    Problem::InvalidApplyResult(_) | Problem::ApplyReadbackMismatch => true,
                    Problem::ReadRequired | Problem::Read(_) => false,
                };
                if uncertain {
                    self.library.forget(self.editor.slot());
                }
                return Err(problem.clone());
            }
            Status::Unloaded => unreachable!("completion establishes a status"),
        };
        self.library.observe(snapshot);
        if let Some(scan) = &mut self.scan {
            scan.refreshed.insert(snapshot.slot.clone());
        }
        Ok(())
    }
}
