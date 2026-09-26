use crate::{Change, State};
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
    Macro(FeatureCommand<crate::macros::Snapshot, crate::macros::Program, String>),
    Lighting(FeatureCommand<crate::lighting::Snapshot, crate::lighting::Setting>),
    Picture(FeatureCommand<crate::picture::Snapshot, BTreeMap<String, [u8; 3]>>),
    Settings(FeatureCommand<crate::settings::Snapshot, crate::settings::Edit>),
    Archive(FeatureCommand<crate::archive::NativeArchive, crate::archive::NativeArchive>),
    ReadMacroCatalog {
        slots: Vec<String>,
    },
    ReviewArchive {
        target: crate::archive::NativeArchive,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CompletionPayload {
    Keymap(FeatureResult<State>),
    Macro {
        slot: String,
        result: FeatureResult<crate::macros::Snapshot>,
    },
    Lighting(FeatureResult<crate::lighting::Snapshot>),
    Picture(FeatureResult<crate::picture::Snapshot>),
    Settings(FeatureResult<crate::settings::Snapshot>),
    Archive(FeatureResult<crate::archive::NativeArchive>),
    ReadMacroCatalog {
        result: Result<Vec<crate::macros::Snapshot>, String>,
    },
    ReviewArchive {
        result: Result<crate::archive::Review, String>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostTicket {
    pub generation: u64,
    pub operation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostStart {
    pub ticket: HostTicket,
    pub mode: crate::lighting::HostMode,
    pub setting: Option<crate::lighting::Setting>,
    pub expected: crate::lighting::Snapshot,
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
        target: crate::archive::NativeArchive,
    },
}
