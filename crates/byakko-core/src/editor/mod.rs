//! One lifecycle for feature baselines, drafts, submitted writes and observations.
use crate::contract::{ApplyFailure, Problem};
use serde::{Deserialize, Serialize};
pub mod keymap;
pub mod lighting;
pub mod macros;
pub mod picture;
pub mod settings;
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Status<S> {
    Unloaded,
    Ready,
    Conflict { device: S },
    Unverified { problem: Problem },
}
#[derive(Clone, Copy)]
pub enum Reception {
    Read,
    Apply,
}
/// A feature supplies its real constraints and values; the editor owns lifecycle.
pub trait Feature {
    type Snapshot: Clone + Eq;
    type Value: Clone + Eq;
    type Edit;
    type Write;
    fn value(snapshot: &Self::Snapshot) -> Option<&Self::Value>;
    fn validate(&self, snapshot: &Self::Snapshot, reception: Reception) -> Result<(), String>;
    /// Implementations validate before mutating, so rejected edits are atomic.
    fn edit(
        &self,
        baseline: &Self::Snapshot,
        draft: &mut Option<Self::Value>,
        edit: Self::Edit,
    ) -> Result<(), String>;
    fn plan(&self, baseline: &Self::Snapshot, value: &Self::Value) -> Result<Self::Write, String>;
}
pub struct Editor<F: Feature> {
    rules: F,
    baseline: Option<F::Snapshot>,
    draft: Option<F::Value>,
    submitted: Option<F::Value>,
    status: Status<F::Snapshot>,
}
impl<F: Feature> Editor<F> {
    pub fn new(rules: F) -> Self {
        Self {
            rules,
            baseline: None,
            draft: None,
            submitted: None,
            status: Status::Unloaded,
        }
    }
    pub fn rules(&self) -> &F {
        &self.rules
    }
    pub fn baseline(&self) -> Option<&F::Snapshot> {
        self.baseline.as_ref()
    }
    pub fn draft(&self) -> Option<&F::Value> {
        self.draft.as_ref()
    }
    pub fn submitted(&self) -> Option<&F::Value> {
        self.submitted.as_ref()
    }
    pub fn status(&self) -> &Status<F::Snapshot> {
        &self.status
    }
    pub fn dirty(&self) -> bool {
        matches!((&self.baseline, &self.draft), (Some(baseline), Some(draft)) if F::value(baseline) != Some(draft))
    }
    pub fn invalidate(&mut self) {
        if matches!(self.status, Status::Ready | Status::Unloaded) {
            self.status = Status::Unverified {
                problem: Problem::ReadRequired,
            };
        }
    }
    pub fn edit(&mut self, edit: F::Edit) -> Result<(), String> {
        self.ready()?;
        self.rules.edit(
            self.baseline.as_ref().ok_or("No baseline")?,
            &mut self.draft,
            edit,
        )
    }
    pub fn revert(&mut self) -> Result<(), String> {
        if self.submitted.is_some() {
            return Err("Wait for the submitted write before reverting".into());
        }
        self.draft = F::value(self.baseline.as_ref().ok_or("No baseline")?).cloned();
        Ok(())
    }
    pub fn request_apply(&mut self) -> Result<(F::Snapshot, F::Write), String> {
        self.ready()?;
        if self.submitted.is_some() {
            return Err("A feature write is already submitted".into());
        }
        if !self.dirty() {
            return Err("No changes are staged".into());
        }
        let baseline = self.baseline.as_ref().ok_or("No baseline")?;
        let draft = self.draft.as_ref().ok_or("Feature is not editable")?;
        let write = self.rules.plan(baseline, draft)?;
        self.submitted = Some(draft.clone());
        Ok((baseline.clone(), write))
    }
    /// Correlation allocation rejected the request before delivery.
    pub fn cancel_apply(&mut self) {
        self.submitted = None;
    }
    pub fn accept_read(&mut self, result: Result<F::Snapshot, String>) {
        let snapshot = match result.and_then(|snapshot| {
            self.rules.validate(&snapshot, Reception::Read)?;
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
            self.draft = F::value(&snapshot).cloned();
        }
        self.baseline = Some(snapshot);
        self.status = Status::Ready;
    }
    pub fn accept_apply(&mut self, result: Result<F::Snapshot, ApplyFailure>) {
        let submitted = self.submitted.take();
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(failure) => {
                self.status = Status::Unverified {
                    problem: Problem::Apply(failure),
                };
                return;
            }
        };
        if let Err(reason) = self.rules.validate(&snapshot, Reception::Apply) {
            self.status = Status::Unverified {
                problem: Problem::InvalidApplyResult(reason),
            };
            return;
        }
        let Some(submitted) = submitted else {
            self.status = Status::Unverified {
                problem: Problem::InvalidApplyResult("No submitted feature write".into()),
            };
            return;
        };
        if F::value(&snapshot) != Some(&submitted) {
            self.status = Status::Unverified {
                problem: Problem::ApplyReadbackMismatch,
            };
            return;
        }
        self.baseline = Some(snapshot);
        self.status = Status::Ready;
    }
    pub fn problem(&self) -> Option<&Problem> {
        match &self.status {
            Status::Unverified { problem } => Some(problem),
            _ => None,
        }
    }
    fn ready(&self) -> Result<(), String> {
        if self.status == Status::Ready {
            Ok(())
        } else {
            Err("Read and verify before editing or applying".into())
        }
    }
}
#[cfg(test)]
#[path = "../tests/editor_lifecycle.rs"]
mod tests;
