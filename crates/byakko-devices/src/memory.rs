//! Deterministic device for exercising the same executor without hardware.

use crate::Device;
use byakko_core::{
    Change, Descriptor, State, lighting, macros,
    session::{ApplyFailure, Recovery},
    validate_changes, validate_state,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

struct MacroStorage {
    capabilities: macros::Capabilities,
    slots: BTreeMap<String, StoredMacro>,
}

struct StoredMacro {
    initial_revision: Vec<u8>,
    revision_number: u64,
    snapshot: macros::Snapshot,
}

pub struct MemoryDevice {
    descriptor: Descriptor,
    state: State,
    initial_revision: Vec<u8>,
    revision_number: u64,
    macros: Option<MacroStorage>,
    lighting: Option<StoredLighting>,
}

struct StoredLighting {
    capabilities: lighting::Capabilities,
    initial_revision: Vec<u8>,
    revision_number: u64,
    snapshot: lighting::Snapshot,
}

impl MemoryDevice {
    pub fn new(descriptor: Descriptor, state: State) -> Result<Self, String> {
        validate_state(&descriptor, &state)?;
        Ok(Self {
            descriptor,
            initial_revision: state.revision.clone(),
            state,
            revision_number: 0,
            macros: None,
            lighting: None,
        })
    }

    pub fn with_macros(
        mut self,
        capabilities: macros::Capabilities,
        snapshots: Vec<macros::Snapshot>,
    ) -> Result<Self, String> {
        if self.macros.is_some() {
            return Err("Memory device macros are already configured".into());
        }
        macros::validate_capabilities(&capabilities)?;
        if capabilities.backend_id != self.descriptor.backend_id {
            return Err("Macro capabilities belong to another backend".into());
        }
        let declared: BTreeSet<_> = capabilities
            .slots
            .iter()
            .map(|slot| slot.id.as_str())
            .collect();
        let mut stored = BTreeMap::new();
        for snapshot in snapshots {
            if snapshot.backend_id != capabilities.backend_id
                || !declared.contains(snapshot.slot.as_str())
            {
                return Err("Macro snapshot backend or slot is unsupported".into());
            }
            if let macros::Content::Editable(program) = &snapshot.content {
                macros::validate_program(&capabilities, program)?;
            }
            let slot = StoredMacro {
                initial_revision: snapshot.revision.clone(),
                revision_number: 0,
                snapshot,
            };
            if stored.insert(slot.snapshot.slot.clone(), slot).is_some() {
                return Err("Duplicate macro snapshot slot".into());
            }
        }
        if stored.len() != declared.len() {
            return Err("Provide exactly one macro snapshot for every declared slot".into());
        }
        self.macros = Some(MacroStorage {
            capabilities,
            slots: stored,
        });
        Ok(self)
    }

    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    pub fn with_lighting(
        mut self,
        capabilities: lighting::Capabilities,
        snapshot: lighting::Snapshot,
    ) -> Result<Self, String> {
        if self.lighting.is_some() {
            return Err("Memory device lighting is already configured".into());
        }
        lighting::validate_snapshot(&capabilities, &snapshot)?;
        if capabilities.backend_id != self.descriptor.backend_id {
            return Err("Lighting capabilities belong to another backend".into());
        }
        self.lighting = Some(StoredLighting {
            capabilities,
            initial_revision: snapshot.revision.clone(),
            revision_number: 0,
            snapshot,
        });
        Ok(self)
    }

    pub fn lighting_capabilities(&self) -> Option<&lighting::Capabilities> {
        self.lighting.as_ref().map(|stored| &stored.capabilities)
    }

    pub fn macro_capabilities(&self) -> Option<&macros::Capabilities> {
        self.macros.as_ref().map(|storage| &storage.capabilities)
    }
}

impl Device for MemoryDevice {
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

    fn read_macro(&mut self, slot: &str) -> Result<macros::Snapshot, String> {
        self.macros
            .as_ref()
            .ok_or("Macro operations are unsupported by this device")?
            .slots
            .get(slot)
            .map(|stored| stored.snapshot.clone())
            .ok_or_else(|| format!("Unknown macro slot: {slot}"))
    }

    fn apply_macro(
        &mut self,
        expected: &macros::Snapshot,
        desired: &macros::Program,
        _backup_dir: &Path,
    ) -> Result<macros::Snapshot, ApplyFailure> {
        let reject = |message| ApplyFailure {
            message,
            recovery: Recovery::NotAttempted,
        };
        let storage = self
            .macros
            .as_mut()
            .ok_or_else(|| reject("Macro operations are unsupported by this device".into()))?;
        let stored = storage
            .slots
            .get_mut(&expected.slot)
            .ok_or_else(|| reject(format!("Unknown macro slot: {}", expected.slot)))?;
        if &stored.snapshot != expected {
            return Err(reject("Stale expected macro snapshot".into()));
        }
        if !matches!(stored.snapshot.content, macros::Content::Editable(_)) {
            return Err(reject("Opaque macro cannot be edited".into()));
        }
        macros::validate_program(&storage.capabilities, desired).map_err(reject)?;
        let next_number = stored
            .revision_number
            .checked_add(1)
            .ok_or_else(|| reject("Memory macro revision exhausted".into()))?;
        let mut next = stored.snapshot.clone();
        next.revision = stored.initial_revision.clone();
        next.revision.extend_from_slice(&next_number.to_be_bytes());
        next.content = macros::Content::Editable(desired.clone());
        stored.snapshot = next.clone();
        stored.revision_number = next_number;
        Ok(next)
    }

    fn read_lighting(&mut self) -> Result<lighting::Snapshot, String> {
        self.lighting
            .as_ref()
            .map(|stored| stored.snapshot.clone())
            .ok_or_else(|| "Lighting operations are unsupported by this device".into())
    }

    fn apply_lighting(
        &mut self,
        expected: &lighting::Snapshot,
        desired: &lighting::Setting,
        _backup_dir: &Path,
    ) -> Result<lighting::Snapshot, ApplyFailure> {
        let reject = |message| ApplyFailure {
            message,
            recovery: Recovery::NotAttempted,
        };
        let stored = self
            .lighting
            .as_mut()
            .ok_or_else(|| reject("Lighting operations are unsupported by this device".into()))?;
        if &stored.snapshot != expected {
            return Err(reject("Stale expected lighting snapshot".into()));
        }
        if !matches!(stored.snapshot.content, lighting::Content::Editable(_)) {
            return Err(reject("Opaque lighting cannot be edited".into()));
        }
        lighting::validate_setting(&stored.capabilities, desired).map_err(reject)?;
        let next_number = stored
            .revision_number
            .checked_add(1)
            .ok_or_else(|| reject("Memory lighting revision exhausted".into()))?;
        let mut next = stored.snapshot.clone();
        next.revision = stored.initial_revision.clone();
        next.revision.extend_from_slice(&next_number.to_be_bytes());
        next.content = lighting::Content::Editable(desired.clone());
        stored.snapshot = next.clone();
        stored.revision_number = next_number;
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

    fn macro_capabilities() -> macros::Capabilities {
        macros::Capabilities {
            byte_budget: None,
            bindings: vec![],
            backend_id: "memory".into(),
            slots: ["first", "second"]
                .into_iter()
                .map(|id| macros::Choice {
                    id: id.into(),
                    label: id.into(),
                })
                .collect(),
            repeat_counts: 0..=10,
            delays_ms: 0..=100,
            keys: Some(4..=10),
            buttons: vec![],
            movement: None,
            backend_actions: vec![],
        }
    }

    fn macro_snapshot(slot: &str, content: macros::Content) -> macros::Snapshot {
        macros::Snapshot {
            backend_id: "memory".into(),
            slot: slot.into(),
            revision: vec![slot.len() as u8],
            content,
        }
    }

    fn program(count: u32) -> macros::Program {
        macros::Program {
            repeat_count: count,
            events: vec![],
        }
    }

    fn macro_device(second: macros::Content) -> MemoryDevice {
        let (descriptor, state) = fixture();
        MemoryDevice::new(descriptor, state)
            .unwrap()
            .with_macros(
                macro_capabilities(),
                vec![
                    macro_snapshot("first", macros::Content::Editable(program(1))),
                    macro_snapshot("second", second),
                ],
            )
            .unwrap()
    }

    #[test]
    fn session_and_single_executor_apply_then_reread_memory_macro() {
        use byakko_core::session::{Acceptance, Command, Session, Status};
        use std::time::Duration;
        fn run(session: &mut Session, worker: &crate::Executor, command: Command) {
            worker.try_submit(command).unwrap();
            let completion = worker
                .completions
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
            assert_eq!(session.accept(completion), Acceptance::Accepted);
            assert!(!session.busy());
        }
        let second = macros::Content::Opaque {
            reason: "Unknown firmware extension".into(),
        };
        let device = macro_device(second.clone());
        let mut session = Session::new(device.descriptor().clone())
            .unwrap()
            .with_macros(macro_capabilities())
            .unwrap();
        let worker = crate::Executor::spawn(device, Default::default()).unwrap();
        worker.set_generation(session.connect().unwrap());
        let read = session.request_read().unwrap();
        run(&mut session, &worker, read);
        let read = session.request_macro_read().unwrap();
        run(&mut session, &worker, read);
        session.edit_macro(macros::Edit::Repeat(3)).unwrap();
        let apply = session.request_macro_apply().unwrap();
        run(&mut session, &worker, apply);
        assert_eq!(session.macros().unwrap().draft(), Some(&program(3)));
        assert!(!session.dirty());
        assert!(matches!(session.status(), Status::Unverified { .. }));
        let read = session.request_read().unwrap();
        run(&mut session, &worker, read);
        assert_eq!(session.status(), &Status::Ready);
        let read = session.request_macro_read().unwrap();
        run(&mut session, &worker, read);
        assert_eq!(session.macros().unwrap().draft(), Some(&program(3)));
        session.select_macro("second").unwrap();
        let read = session.request_macro_read().unwrap();
        run(&mut session, &worker, read);
        assert_eq!(
            session.macros().unwrap().baseline().unwrap().content,
            second
        );
        assert!(session.macros().unwrap().draft().is_none());
    }

    #[test]
    fn stale_macro_snapshot_and_invalid_program_leave_all_slots_unchanged() {
        let mut device = macro_device(macros::Content::Editable(program(2)));
        let first = device.read_macro("first").unwrap();
        let second = device.read_macro("second").unwrap();
        let mut stale = first.clone();
        stale.revision.push(0xff);
        assert!(matches!(
            device.apply_macro(&stale, &program(3), Path::new("ignored")),
            Err(ApplyFailure {
                recovery: Recovery::NotAttempted,
                ..
            })
        ));
        assert!(
            device
                .apply_macro(&first, &program(11), Path::new("ignored"))
                .is_err()
        );
        assert_eq!(device.read_macro("first").unwrap(), first);
        assert_eq!(device.read_macro("second").unwrap(), second);
    }

    #[test]
    fn opaque_macro_is_read_only_and_other_slot_remains_unchanged() {
        let mut device = macro_device(macros::Content::Opaque {
            reason: "unknown".into(),
        });
        let first = device.read_macro("first").unwrap();
        let second = device.read_macro("second").unwrap();
        assert!(matches!(
            device.apply_macro(&second, &program(3), Path::new("ignored")),
            Err(ApplyFailure {
                recovery: Recovery::NotAttempted,
                ..
            })
        ));
        let updated = device
            .apply_macro(&first, &program(3), Path::new("ignored"))
            .unwrap();
        assert_ne!(updated.revision, first.revision);
        assert_eq!(updated.content, macros::Content::Editable(program(3)));
        assert_eq!(device.read_macro("second").unwrap(), second);
        assert!(
            device
                .apply_macro(&first, &program(4), Path::new("ignored"))
                .is_err()
        );
    }

    #[test]
    fn macro_revision_has_fixed_length_after_repeated_writes() {
        let mut device = macro_device(macros::Content::Editable(program(2)));
        let initial = device.read_macro("first").unwrap();
        let first = device
            .apply_macro(&initial, &program(3), Path::new("ignored"))
            .unwrap();
        let second = device
            .apply_macro(&first, &program(4), Path::new("ignored"))
            .unwrap();
        assert_eq!(first.revision.len(), initial.revision.len() + 8);
        assert_eq!(second.revision.len(), first.revision.len());
        assert_ne!(first.revision, second.revision);
    }

    #[test]
    fn macro_builder_requires_exact_declared_slots_and_valid_content() {
        let (descriptor, state) = fixture();
        let new_device = || MemoryDevice::new(descriptor.clone(), state.clone()).unwrap();
        let first = macro_snapshot("first", macros::Content::Editable(program(1)));
        assert!(
            new_device()
                .with_macros(macro_capabilities(), vec![first.clone()])
                .is_err()
        );
        assert!(
            new_device()
                .with_macros(macro_capabilities(), vec![first.clone(), first])
                .is_err()
        );
        assert!(
            new_device()
                .with_macros(
                    macro_capabilities(),
                    vec![
                        macro_snapshot("first", macros::Content::Editable(program(11))),
                        macro_snapshot("second", macros::Content::Editable(program(1))),
                    ]
                )
                .is_err()
        );
        assert!(
            macro_device(macros::Content::Editable(program(2)))
                .with_macros(
                    macro_capabilities(),
                    vec![
                        macro_snapshot("first", macros::Content::Editable(program(1))),
                        macro_snapshot("second", macros::Content::Editable(program(2))),
                    ]
                )
                .is_err()
        );
    }

    #[test]
    fn lighting_conflict_invalid_edit_and_opaque_state_preserve_snapshot() {
        let (descriptor, state) = fixture();
        let caps = lighting::Capabilities {
            backend_id: "memory".into(),
            effects: vec![lighting::Effect {
                id: "steady".into(),
                label: "Steady".into(),
                brightness: Some(0..=4),
                speed: None,
                options: vec![],
                color: Some(lighting::ColorCapability::Fixed),
            }],
        };
        let setting = lighting::Setting {
            effect: "steady".into(),
            brightness: Some(2),
            speed: None,
            option: None,
            color: Some(lighting::Color::Rgb([1, 2, 3])),
        };
        let initial = lighting::Snapshot {
            backend_id: "memory".into(),
            revision: vec![9],
            content: lighting::Content::Editable(setting.clone()),
        };
        let mut device = MemoryDevice::new(descriptor.clone(), state.clone())
            .unwrap()
            .with_lighting(caps.clone(), initial.clone())
            .unwrap();
        let mut stale = initial.clone();
        stale.revision.push(0);
        assert!(
            device
                .apply_lighting(&stale, &setting, Path::new("ignored"))
                .is_err()
        );
        let mut invalid = setting.clone();
        invalid.brightness = Some(5);
        assert!(
            device
                .apply_lighting(&initial, &invalid, Path::new("ignored"))
                .is_err()
        );
        assert_eq!(device.read_lighting().unwrap(), initial);
        let next = device
            .apply_lighting(&initial, &setting, Path::new("ignored"))
            .unwrap();
        assert_eq!(next.revision.len(), initial.revision.len() + 8);
        assert!(
            device
                .apply_lighting(&initial, &setting, Path::new("ignored"))
                .is_err()
        );
        let opaque = lighting::Snapshot {
            content: lighting::Content::Opaque {
                reason: "unknown".into(),
            },
            ..initial
        };
        let mut device = MemoryDevice::new(descriptor, state)
            .unwrap()
            .with_lighting(caps, opaque.clone())
            .unwrap();
        assert!(
            device
                .apply_lighting(&opaque, &setting, Path::new("ignored"))
                .is_err()
        );
        assert_eq!(device.read_lighting().unwrap(), opaque);
    }
}
