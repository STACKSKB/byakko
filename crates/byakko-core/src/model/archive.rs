//! Opaque, backend-owned native configuration archives.
use crate::contract::ApplyFailure;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArchiveCapabilities {
    pub backend_id: String,
    pub format_id: String,
    pub max_bytes: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NativeArchive {
    pub backend_id: String,
    pub format_id: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SectionChange {
    pub id: String,
    pub label: String,
    pub count: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Review {
    pub before: NativeArchive,
    pub target: NativeArchive,
    pub changes: Vec<SectionChange>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ArchiveProblem {
    ReadRequired,
    Capture(String),
    Review(String),
    InvalidResult(String),
    Apply(ApplyFailure),
    ApplyReadbackMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ArchiveState {
    Idle,
    Captured(NativeArchive),
    Ready(Review),
    Unverified {
        problem: ArchiveProblem,
        review: Option<Review>,
    },
}
