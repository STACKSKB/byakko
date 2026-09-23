//! Device-neutral per-key RGB picture values.
use crate::Descriptor;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub mod editor;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub backend_id: String,
    pub keys: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Content {
    Editable(BTreeMap<String, [u8; 3]>),
    Opaque { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub backend_id: String,
    pub revision: Vec<u8>,
    /// Backend-owned state that affects which picture the device reads or writes.
    /// Empty for backends whose picture address is independent of other settings.
    #[serde(default)]
    pub context_revision: Vec<u8>,
    pub content: Content,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Channel {
    Red,
    Green,
    Blue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelControl {
    pub label: &'static str,
    pub channel: Channel,
    pub value: u8,
}

pub fn channels(rgb: [u8; 3]) -> [ChannelControl; 3] {
    [
        ChannelControl {
            label: "Red",
            channel: Channel::Red,
            value: rgb[0],
        },
        ChannelControl {
            label: "Green",
            channel: Channel::Green,
            value: rgb[1],
        },
        ChannelControl {
            label: "Blue",
            channel: Channel::Blue,
            value: rgb[2],
        },
    ]
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Edit {
    Color {
        key: String,
        color: [u8; 3],
    },
    Channel {
        key: String,
        channel: Channel,
        value: u8,
    },
}

pub fn validate_capabilities(caps: &Capabilities, descriptor: &Descriptor) -> Result<(), String> {
    if caps.backend_id.is_empty()
        || caps.backend_id != descriptor.backend_id
        || caps.keys.is_empty()
    {
        return Err("Invalid picture backend or empty key catalog".into());
    }
    let mut seen = BTreeSet::new();
    for id in &caps.keys {
        if id.is_empty() || !seen.insert(id) {
            return Err("Duplicate or empty picture key ID".into());
        }
        if !descriptor.keys.iter().any(|key| key.id == *id) {
            return Err(format!("Unknown picture key: {id}"));
        }
    }
    Ok(())
}

pub fn validate_snapshot(caps: &Capabilities, snapshot: &Snapshot) -> Result<(), String> {
    if snapshot.backend_id != caps.backend_id {
        return Err("Picture result belongs to a different backend".into());
    }
    if let Content::Editable(colors) = &snapshot.content {
        let keys: BTreeSet<_> = caps.keys.iter().collect();
        if colors.len() != keys.len() || colors.keys().any(|id| !keys.contains(id)) {
            return Err("Picture has missing or unknown keys".into());
        }
    }
    Ok(())
}
