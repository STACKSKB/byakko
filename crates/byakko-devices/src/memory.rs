//! Deterministic device for exercising the same executor without hardware.

use crate::KeymapDevice;
use byakko_core::{
    Change, Descriptor, State,
    session::{ApplyFailure, Recovery},
    validate_changes, validate_state,
};
use std::path::Path;

pub struct MemoryDevice {
    descriptor: Descriptor,
    state: State,
    initial_revision: Vec<u8>,
    revision_number: u64,
}

impl MemoryDevice {
    pub fn new(descriptor: Descriptor, state: State) -> Result<Self, String> {
        validate_state(&descriptor, &state)?;
        Ok(Self {
            descriptor,
            initial_revision: state.revision.clone(),
            state,
            revision_number: 0,
        })
    }

    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
}

impl KeymapDevice for MemoryDevice {
    fn read(&mut self) -> Result<State, String> {
        Ok(self.state.clone())
    }

    fn apply(
        &mut self,
        expected: &State,
        changes: &[Change],
        _backup_dir: &Path,
    ) -> Result<State, ApplyFailure> {
        let reject = |message| ApplyFailure {
            message,
            recovery: Recovery::NotAttempted,
        };
        if expected != &self.state {
            return Err(reject("Stale expected device state".into()));
        }
        validate_changes(&self.descriptor, changes).map_err(reject)?;
        let next_number = self
            .revision_number
            .checked_add(1)
            .ok_or_else(|| reject("Memory device revision exhausted".into()))?;
        let mut next = self.state.clone();
        for change in changes {
            next.bindings
                .get_mut(&change.layer)
                .expect("validated layer")
                .insert(change.key.clone(), change.action.clone());
        }
        next.revision = self.initial_revision.clone();
        next.revision.extend_from_slice(&next_number.to_be_bytes());
        validate_state(&self.descriptor, &next).map_err(reject)?;
        self.state = next.clone();
        self.revision_number = next_number;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::{Action, Layer, PhysicalKey};
    use std::collections::BTreeMap;

    fn fixture() -> (Descriptor, State) {
        let descriptor = Descriptor {
            backend_id: "memory".into(),
            device_name: "Demo".into(),
            keys: [("editable", true), ("reserved", false)]
                .into_iter()
                .map(|(id, writable)| PhysicalKey {
                    id: id.into(),
                    label: id.into(),
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                    visible: true,
                    writable,
                })
                .collect(),
            layers: vec![Layer {
                id: "base".into(),
                label: "Base".into(),
            }],
            actions: vec![],
        };
        let state = State {
            revision: vec![0xff, 0x01],
            bindings: BTreeMap::from([(
                "base".into(),
                BTreeMap::from([
                    (
                        "editable".into(),
                        Action::Opaque {
                            backend_id: "memory".into(),
                            data: vec![0, 255],
                            label: "Unknown".into(),
                        },
                    ),
                    ("reserved".into(), Action::Key(4)),
                ]),
            )]),
        };
        (descriptor, state)
    }

    #[test]
    fn stale_expected_state_does_not_change_device() {
        let (descriptor, state) = fixture();
        let mut device = MemoryDevice::new(descriptor, state.clone()).unwrap();
        let mut stale = state.clone();
        stale.revision.push(1);
        let result = device.apply(&stale, &[], Path::new("ignored"));
        assert!(matches!(
            result,
            Err(ApplyFailure {
                recovery: Recovery::NotAttempted,
                ..
            })
        ));
        assert_eq!(device.read().unwrap(), state);
    }

    #[test]
    fn reserved_key_is_rejected_without_changing_state() {
        let (descriptor, state) = fixture();
        let mut device = MemoryDevice::new(descriptor, state.clone()).unwrap();
        let result = device.apply(
            &state,
            &[Change {
                layer: "base".into(),
                key: "reserved".into(),
                action: Action::Disabled,
            }],
            Path::new("ignored"),
        );
        assert!(matches!(
            result,
            Err(ApplyFailure {
                recovery: Recovery::NotAttempted,
                ..
            })
        ));
        assert_eq!(device.read().unwrap(), state);
    }

    #[test]
    fn opaque_action_survives_apply_and_revision_advances() {
        let (descriptor, state) = fixture();
        let opaque = state.bindings["base"]["editable"].clone();
        let mut device = MemoryDevice::new(descriptor, state.clone()).unwrap();
        let changed = device
            .apply(
                &state,
                &[Change {
                    layer: "base".into(),
                    key: "editable".into(),
                    action: Action::Key(5),
                }],
                Path::new("ignored"),
            )
            .unwrap();
        let restored = device
            .apply(
                &changed,
                &[Change {
                    layer: "base".into(),
                    key: "editable".into(),
                    action: opaque.clone(),
                }],
                Path::new("ignored"),
            )
            .unwrap();
        assert_eq!(restored.bindings["base"]["editable"], opaque);
        assert_ne!(state.revision, changed.revision);
        assert_ne!(changed.revision, restored.revision);
        assert_eq!(device.read().unwrap(), restored);
    }
}
