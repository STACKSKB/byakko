//! Device-neutral per-key RGB picture values.
use crate::Descriptor;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub mod editor;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub backend_id: String,
    pub keys: Vec<String>,
    /// An advertised onboard lighting effect that displays stored key colors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lighting_effect: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Content {
    Editable(BTreeMap<String, [u8; 3]>),
    Opaque { reason: String },
}

pub use crate::SnapshotEvidence as Evidence;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub backend_id: String,
    pub revision: Vec<u8>,
    /// Backend-owned state that affects which picture the device reads or writes.
    /// Empty for backends whose picture address is independent of other settings.
    #[serde(default)]
    pub context_revision: Vec<u8>,
    #[serde(default)]
    pub evidence: Evidence,
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
        || caps
            .lighting_effect
            .as_ref()
            .is_some_and(|id| id.trim().is_empty())
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

#[cfg(test)]
mod tests {
    use super::{Capabilities, Content, Evidence, Snapshot};
    use std::collections::BTreeMap;

    #[test]
    fn optional_display_effect_preserves_older_catalog_json() {
        let old = serde_json::json!({"backend_id": "memory", "keys": ["a"]});
        let caps: Capabilities = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(caps.lighting_effect, None);
        assert_eq!(serde_json::to_value(&caps).unwrap(), old);
        let with_effect = Capabilities {
            lighting_effect: Some("per-key".into()),
            ..caps
        };
        assert_eq!(
            serde_json::to_value(with_effect).unwrap()["lighting_effect"],
            "per-key"
        );
    }

    #[test]
    fn picture_evidence_defaults_to_readback_for_legacy_json() {
        let legacy = serde_json::json!({
            "backend_id": "memory",
            "revision": [1],
            "content": {"Editable": {"a": [1, 2, 3]}}
        });
        let read: Snapshot = serde_json::from_value(legacy).unwrap();
        assert_eq!(read.evidence, Evidence::Readback);
        assert_eq!(read.context_revision, Vec::<u8>::new());
        let accepted = Snapshot {
            evidence: Evidence::TransportAccepted,
            ..read
        };
        assert_eq!(
            serde_json::from_value::<Snapshot>(serde_json::to_value(&accepted).unwrap()).unwrap(),
            accepted
        );
        assert_eq!(
            accepted.content,
            Content::Editable(BTreeMap::from([("a".into(), [1, 2, 3])]))
        );
    }
}
