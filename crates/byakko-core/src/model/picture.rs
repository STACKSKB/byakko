//! Device-neutral per-key RGB picture values.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

pub use crate::model::SnapshotEvidence as Evidence;

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
