//! Shared deterministic draft lifecycle for complete device snapshots.
use crate::session::{ApplyFailure, Problem};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Status<S> {
    Unloaded,
    Ready,
    Conflict { device: S },
    Unverified { problem: Problem },
}

pub struct Draft<S, V> {
    baseline: Option<S>,
    draft: Option<V>,
    status: Status<S>,
}

impl<S: Clone + Eq, V: Clone + Eq> Draft<S, V> {
    pub fn new() -> Self {
        Self {
            baseline: None,
            draft: None,
            status: Status::Unloaded,
        }
    }
    pub fn baseline(&self) -> Option<&S> {
        self.baseline.as_ref()
    }
    pub fn draft(&self) -> Option<&V> {
        self.draft.as_ref()
    }
    pub fn status(&self) -> &Status<S> {
        &self.status
    }
    pub fn dirty(&self, editable: fn(&S) -> Option<&V>) -> bool {
        matches!((&self.baseline, &self.draft), (Some(original), Some(draft)) if editable(original) != Some(draft))
    }
    pub fn invalidate(&mut self) {
        self.status = Status::Unverified {
            problem: Problem::ReadRequired,
        };
    }
    pub fn stage(&mut self, value: V) {
        self.draft = Some(value);
    }
    pub fn revert(&mut self, editable: fn(&S) -> Option<&V>) -> Result<(), String> {
        let baseline = self.baseline.as_ref().ok_or("No baseline")?;
        self.draft = editable(baseline).cloned();
        Ok(())
    }
    pub fn request_apply(&self, editable: fn(&S) -> Option<&V>) -> Result<(S, V), String> {
        if self.status != Status::Ready {
            return Err("Read and verify before applying".into());
        }
        if !self.dirty(editable) {
            return Err("No changes are staged".into());
        }
        Ok((
            self.baseline.as_ref().unwrap().clone(),
            self.draft.as_ref().unwrap().clone(),
        ))
    }
    pub fn accept_read(
        &mut self,
        result: Result<S, String>,
        validate: impl Fn(&S) -> Result<(), String>,
        editable: fn(&S) -> Option<&V>,
    ) {
        let snapshot = match result.and_then(|snapshot| {
            validate(&snapshot)?;
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
        if self.dirty(editable) && self.baseline.as_ref() != Some(&snapshot) {
            self.status = Status::Conflict { device: snapshot };
            return;
        }
        if !self.dirty(editable) {
            self.draft = editable(&snapshot).cloned();
        }
        self.baseline = Some(snapshot);
        self.status = Status::Ready;
    }
    pub fn accept_apply(
        &mut self,
        result: Result<S, ApplyFailure>,
        validate: impl Fn(&S) -> Result<(), String>,
        editable: fn(&S) -> Option<&V>,
    ) {
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(failure) => {
                self.status = Status::Unverified {
                    problem: Problem::Apply(failure),
                };
                return;
            }
        };
        if let Err(reason) = validate(&snapshot) {
            self.status = Status::Unverified {
                problem: Problem::InvalidApplyResult(reason),
            };
            return;
        }
        if editable(&snapshot) != self.draft.as_ref() {
            self.status = Status::Unverified {
                problem: Problem::ApplyReadbackMismatch,
            };
            return;
        }
        self.baseline = Some(snapshot);
        self.status = Status::Ready;
    }
}
