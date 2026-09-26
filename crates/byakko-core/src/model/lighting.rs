//! Device-neutral lighting catalog and snapshot values.
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;

pub use crate::model::SnapshotEvidence as Evidence;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub id: String,
    pub label: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Color {
    Rgb([u8; 3]),
    Rainbow,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ColorCapability {
    Fixed,
    Rainbow,
    FixedOrRainbow,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub id: String,
    pub label: String,
    pub brightness: Option<RangeInclusive<u16>>,
    pub speed: Option<RangeInclusive<u16>>,
    pub options: Vec<Choice>,
    pub color: Option<ColorCapability>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HostSource {
    ScreenAverage,
    PlaybackAudio { bands: u8 },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostMode {
    pub id: String,
    pub label: String,
    pub source: HostSource,
    pub parameters: Option<HostParameters>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostParameters {
    pub schema: Effect,
    pub default: Setting,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub backend_id: String,
    pub effects: Vec<Effect>,
    pub host_modes: Vec<HostMode>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Setting {
    pub effect: String,
    pub brightness: Option<u16>,
    pub speed: Option<u16>,
    pub option: Option<String>,
    pub color: Option<Color>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Channel {
    Red,
    Green,
    Blue,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Edit {
    Effect(String),
    Brightness(u16),
    Speed(u16),
    Option(String),
    Color(Color),
    Channel(Channel, u8),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Content {
    Editable(Setting),
    /// A known host-driven mode remains stored. This session does not own its
    /// stream; choosing an onboard effect is an explicit, guarded replacement.
    HostActive {
        mode_id: String,
    },
    Opaque {
        reason: String,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub backend_id: String,
    pub revision: Vec<u8>,
    #[serde(default)]
    pub evidence: Evidence,
    pub content: Content,
}

#[cfg(test)]
mod tests {
    use super::{Evidence, Snapshot};

    #[test]
    fn legacy_snapshot_defaults_to_readback_evidence() {
        let legacy = serde_json::json!({
            "backend_id": "memory",
            "revision": [1],
            "content": {"Opaque": {"reason": "unknown"}}
        });
        let snapshot: Snapshot = serde_json::from_value(legacy).unwrap();
        assert_eq!(snapshot.evidence, Evidence::Readback);
        let accepted = Snapshot {
            evidence: Evidence::TransportAccepted,
            ..snapshot
        };
        assert_eq!(
            serde_json::from_value::<Snapshot>(serde_json::to_value(&accepted).unwrap()).unwrap(),
            accepted
        );
    }
}
