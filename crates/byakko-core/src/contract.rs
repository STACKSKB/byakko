use crate::model::keymap::{Change, State};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
/// Correlation belongs to the transport contract, independently of its payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub generation: u64,
    pub operation: u64,
    pub payload: T,
}

impl<T> Envelope<T> {
    pub fn map<U>(self, transform: impl FnOnce(T) -> U) -> Envelope<U> {
        Envelope {
            generation: self.generation,
            operation: self.operation,
            payload: transform(self.payload),
        }
    }
}

pub type Command = Envelope<CommandPayload>;
pub type Completion = Envelope<CompletionPayload>;

/// Read and apply share one operation shape across all feature types.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FeatureCommand<S, E, R = ()> {
    Read(R),
    Apply { expected: S, desired: E },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FeatureResult<S> {
    Read(Result<S, String>),
    Apply(Result<S, ApplyFailure>),
}

impl<S> FeatureResult<S> {
    pub fn activity(&self, feature: Feature) -> DeviceActivity {
        match self {
            Self::Read(_) => DeviceActivity::Read(feature),
            Self::Apply(_) => DeviceActivity::Apply(feature),
        }
    }

    pub fn failed_write(&self) -> bool {
        matches!(self, Self::Apply(Err(_)))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CommandPayload {
    Keymap(FeatureCommand<State, Vec<Change>>),
    Macro(FeatureCommand<crate::model::macros::Snapshot, crate::model::macros::Program, String>),
    Lighting(FeatureCommand<crate::model::lighting::Snapshot, crate::model::lighting::Setting>),
    Picture(FeatureCommand<crate::model::picture::Snapshot, BTreeMap<String, [u8; 3]>>),
    Settings(FeatureCommand<crate::model::settings::Snapshot, crate::model::settings::Edit>),
    Archive(
        FeatureCommand<crate::model::archive::NativeArchive, crate::model::archive::NativeArchive>,
    ),
    ReadMacroCatalog {
        slots: Vec<String>,
    },
    ReviewArchive {
        target: crate::model::archive::NativeArchive,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CompletionPayload {
    Keymap(FeatureResult<State>),
    Macro {
        slot: String,
        result: FeatureResult<crate::model::macros::Snapshot>,
    },
    Lighting(FeatureResult<crate::model::lighting::Snapshot>),
    Picture(FeatureResult<crate::model::picture::Snapshot>),
    Settings(FeatureResult<crate::model::settings::Snapshot>),
    Archive(FeatureResult<crate::model::archive::NativeArchive>),
    ReadMacroCatalog {
        result: Result<Vec<crate::model::macros::Snapshot>, String>,
    },
    ReviewArchive {
        result: Result<crate::model::archive::Review, String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Recovery {
    Verified,
    Failed,
    NotAttempted,
    /// The executor could not establish whether recovery ran or succeeded.
    Unverified,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplyFailure {
    pub message: String,
    pub recovery: Recovery,
}

impl std::fmt::Display for ApplyFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApplyFailure {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostTicket {
    pub generation: u64,
    pub operation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostStart {
    pub ticket: HostTicket,
    pub mode: crate::model::lighting::HostMode,
    pub setting: Option<crate::model::lighting::Setting>,
    pub expected: crate::model::lighting::Snapshot,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostEvent {
    pub ticket: HostTicket,
    pub kind: HostEventKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HostEventKind {
    Started,
    Finished {
        restored: Result<crate::model::lighting::Snapshot, ApplyFailure>,
        /// A frame or sampler failure remains visible even when restoration succeeds.
        problem: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Problem {
    ReadRequired,
    Read(String),
    Apply(ApplyFailure),
    InvalidApplyResult(String),
    ApplyReadbackMismatch,
}

/// Feature identity used by a pending read or write; macros retain their slot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Feature {
    Keymap,
    Macro { slot: String },
    Lighting,
    Picture,
    Settings,
    Archive,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DeviceActivity {
    Read(Feature),
    Apply(Feature),
    ReviewArchive {
        target: crate::model::archive::NativeArchive,
    },
}
