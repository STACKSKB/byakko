//! Deterministic device for exercising the same executor without hardware.

use crate::Device;
use byakko_core::validation;
use byakko_core::{
    contract::{ApplyFailure, Recovery},
    model::{
        archive,
        keymap::{
            Action, ActionCategory, ActionChoice, Change, Descriptor, Layer, PhysicalKey, State,
        },
        lighting, macros, picture, settings,
    },
    validation::keymap::{validate_changes, validate_state},
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
    picture: Option<StoredPicture>,
    settings: Option<StoredSettings>,
    archive: Option<StoredArchive>,
}

struct StoredLighting {
    capabilities: lighting::Capabilities,
    initial_revision: Vec<u8>,
    revision_number: u64,
    snapshot: lighting::Snapshot,
}

struct StoredPicture {
    capabilities: picture::Capabilities,
    initial_revision: Vec<u8>,
    revision_number: u64,
    snapshot: picture::Snapshot,
}

struct StoredSettings {
    capabilities: settings::Capabilities,
    initial_revision: Vec<u8>,
    revision_number: u64,
    snapshot: settings::Snapshot,
}

struct StoredArchive {
    capabilities: archive::ArchiveCapabilities,
    snapshot: archive::NativeArchive,
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
            picture: None,
            settings: None,
            archive: None,
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
        validation::macros::validate_capabilities(&capabilities)?;
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
                validation::macros::validate_program(&capabilities, program)?;
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

    /// Build a client session from this simulator's advertised features.
    pub fn session(&self) -> Result<byakko_core::session::Session, String> {
        let mut session = byakko_core::session::Session::new(self.descriptor.clone())?;
        if let Some(capabilities) = self.macro_capabilities() {
            session = session.with_macros(capabilities.clone())?;
        }
        if let Some(capabilities) = self.lighting_capabilities() {
            session = session.with_lighting(capabilities.clone())?;
        }
        if let Some(capabilities) = self.picture_capabilities() {
            session = session.with_picture(capabilities.clone())?;
        }
        if let Some(capabilities) = self.settings_capabilities() {
            session = session.with_settings(capabilities.clone())?;
        }
        if let Some(storage) = &self.archive {
            session = session.with_archive(storage.capabilities.clone())?;
        }
        Ok(session)
    }
    pub fn with_lighting(
        mut self,
        capabilities: lighting::Capabilities,
        snapshot: lighting::Snapshot,
    ) -> Result<Self, String> {
        if self.lighting.is_some() {
            return Err("Memory device lighting is already configured".into());
        }
        validation::lighting::validate_snapshot(&capabilities, &snapshot)?;
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

    pub fn with_picture(
        mut self,
        capabilities: picture::Capabilities,
        snapshot: picture::Snapshot,
    ) -> Result<Self, String> {
        if self.picture.is_some() {
            return Err("Memory device picture is already configured".into());
        }
        validation::picture::validate_capabilities(&capabilities, &self.descriptor)?;
        validation::picture::validate_snapshot(&capabilities, &snapshot)?;
        self.picture = Some(StoredPicture {
            capabilities,
            initial_revision: snapshot.revision.clone(),
            revision_number: 0,
            snapshot,
        });
        Ok(self)
    }

    pub fn picture_capabilities(&self) -> Option<&picture::Capabilities> {
        self.picture.as_ref().map(|stored| &stored.capabilities)
    }

    pub fn with_settings(
        mut self,
        capabilities: settings::Capabilities,
        snapshot: settings::Snapshot,
    ) -> Result<Self, String> {
        if self.settings.is_some() {
            return Err("Memory device settings are already configured".into());
        }
        validation::settings::validate_capabilities(&capabilities)?;
        if capabilities.backend_id != self.descriptor.backend_id
            || snapshot.backend_id != capabilities.backend_id
        {
            return Err("Settings capabilities or snapshot belong to another backend".into());
        }
        validation::settings::validate_snapshot(&capabilities, &snapshot)?;
        self.settings = Some(StoredSettings {
            capabilities,
            initial_revision: snapshot.revision.clone(),
            revision_number: 0,
            snapshot,
        });
        Ok(self)
    }

    pub fn settings_capabilities(&self) -> Option<&settings::Capabilities> {
        self.settings.as_ref().map(|stored| &stored.capabilities)
    }

    pub fn with_archive(
        mut self,
        capabilities: archive::ArchiveCapabilities,
        snapshot: archive::NativeArchive,
    ) -> Result<Self, String> {
        if self.archive.is_some() {
            return Err("Memory device archive is already configured".into());
        }
        validation::archive::validate_capabilities(&capabilities)?;
        if capabilities.backend_id != self.descriptor.backend_id {
            return Err("Archive capabilities belong to another backend".into());
        }
        validation::archive::validate_archive(&capabilities, &snapshot)?;
        self.archive = Some(StoredArchive {
            capabilities,
            snapshot,
        });
        Ok(self)
    }
}

impl Device for MemoryDevice {
    fn archive_capabilities(&self) -> Option<archive::ArchiveCapabilities> {
        self.archive
            .as_ref()
            .map(|stored| stored.capabilities.clone())
    }

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
        validation::macros::validate_program(&storage.capabilities, desired).map_err(reject)?;
        if !storage
            .capabilities
            .editable_repeat_counts
            .contains(&desired.repeat_count)
        {
            return Err(reject("Repeat count cannot be newly programmed".into()));
        }
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
            .map(|stored| lighting::Snapshot {
                evidence: lighting::Evidence::Readback,
                ..stored.snapshot.clone()
            })
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
        let current = lighting::Snapshot {
            evidence: expected.evidence,
            ..stored.snapshot.clone()
        };
        if &current != expected {
            return Err(reject("Stale expected lighting snapshot".into()));
        }
        if matches!(stored.snapshot.content, lighting::Content::Opaque { .. }) {
            return Err(reject("Opaque lighting cannot be edited".into()));
        }
        validation::lighting::validate_setting(&stored.capabilities, desired).map_err(reject)?;
        let next_number = stored
            .revision_number
            .checked_add(1)
            .ok_or_else(|| reject("Memory lighting revision exhausted".into()))?;
        let mut next = stored.snapshot.clone();
        next.revision = stored.initial_revision.clone();
        next.revision.extend_from_slice(&next_number.to_be_bytes());
        next.content = lighting::Content::Editable(desired.clone());
        next.evidence = lighting::Evidence::TransportAccepted;
        if !next.picture_context.is_empty() {
            next.picture_context = desired.effect.as_bytes().to_vec();
        }
        if let Some(picture) = &mut self.picture
            && !picture.snapshot.context_revision.is_empty()
        {
            picture.snapshot.context_revision = desired.effect.as_bytes().to_vec();
        }
        stored.snapshot = next.clone();
        stored.revision_number = next_number;
        Ok(next)
    }

    fn read_picture(&mut self) -> Result<picture::Snapshot, String> {
        self.picture
            .as_ref()
            .map(|stored| picture::Snapshot {
                evidence: picture::Evidence::Readback,
                ..stored.snapshot.clone()
            })
            .ok_or_else(|| "Picture operations are unsupported by this device".into())
    }

    fn apply_picture(
        &mut self,
        expected: &picture::Snapshot,
        desired: &BTreeMap<String, [u8; 3]>,
        _backup_dir: &Path,
    ) -> Result<picture::Snapshot, ApplyFailure> {
        let reject = |message| ApplyFailure {
            message,
            recovery: Recovery::NotAttempted,
        };
        let stored = self
            .picture
            .as_mut()
            .ok_or_else(|| reject("Picture operations are unsupported by this device".into()))?;
        let current = picture::Snapshot {
            evidence: expected.evidence,
            ..stored.snapshot.clone()
        };
        if &current != expected {
            return Err(reject("Stale expected picture snapshot".into()));
        }
        if !matches!(stored.snapshot.content, picture::Content::Editable(_)) {
            return Err(reject("Opaque picture cannot be edited".into()));
        }
        let next_content = picture::Content::Editable(desired.clone());
        let candidate = picture::Snapshot {
            content: next_content,
            ..stored.snapshot.clone()
        };
        validation::picture::validate_snapshot(&stored.capabilities, &candidate).map_err(reject)?;
        let next_number = stored
            .revision_number
            .checked_add(1)
            .ok_or_else(|| reject("Memory picture revision exhausted".into()))?;
        let mut next = candidate;
        next.evidence = picture::Evidence::TransportAccepted;
        next.revision = stored.initial_revision.clone();
        next.revision.extend_from_slice(&next_number.to_be_bytes());
        stored.snapshot = next.clone();
        stored.revision_number = next_number;
        Ok(next)
    }

    fn read_settings(&mut self) -> Result<settings::Snapshot, String> {
        self.settings
            .as_ref()
            .map(|stored| stored.snapshot.clone())
            .ok_or_else(|| "Settings operations are unsupported by this device".into())
    }

    fn apply_setting(
        &mut self,
        expected: &settings::Snapshot,
        edit: &settings::Edit,
        _backup_dir: &Path,
    ) -> Result<settings::Snapshot, ApplyFailure> {
        let reject = |message| ApplyFailure {
            message,
            recovery: Recovery::NotAttempted,
        };
        let stored = self
            .settings
            .as_mut()
            .ok_or_else(|| reject("Settings operations are unsupported by this device".into()))?;
        if &stored.snapshot != expected {
            return Err(reject("Stale expected settings snapshot".into()));
        }
        let content = &stored.snapshot.content;
        let settings::Content::Editable(current) = content else {
            return Err(reject("Opaque settings cannot be edited".into()));
        };
        validation::settings::validate_value(&stored.capabilities, edit).map_err(reject)?;
        if !current.contains_key(&edit.id) {
            return Err(reject("Unknown settings field".into()));
        }
        let next_number = stored
            .revision_number
            .checked_add(1)
            .ok_or_else(|| reject("Memory settings revision exhausted".into()))?;
        let mut next = stored.snapshot.clone();
        let settings::Content::Editable(values) = &mut next.content else {
            unreachable!()
        };
        values.insert(edit.id.clone(), edit.value.clone());
        next.revision = stored.initial_revision.clone();
        next.revision.extend_from_slice(&next_number.to_be_bytes());
        validation::settings::validate_snapshot(&stored.capabilities, &next).map_err(reject)?;
        stored.snapshot = next.clone();
        stored.revision_number = next_number;
        Ok(next)
    }

    fn capture_archive(&mut self) -> Result<archive::NativeArchive, String> {
        let stored = self
            .archive
            .as_ref()
            .ok_or("Native archive operations are unsupported by this device")?;
        if stored.capabilities.format_id == "memory-demo-v1" {
            let bytes=serde_json::to_vec(&serde_json::json!({
                "keymap": self.state,
                "macros": self.macros.as_ref().map(|storage| storage.slots.iter().map(|(id,slot)| (id,&slot.snapshot)).collect::<BTreeMap<_,_>>()),
                "lighting": self.lighting.as_ref().map(|storage| &storage.snapshot),
                "picture": self.picture.as_ref().map(|storage| &storage.snapshot),
                "settings": self.settings.as_ref().map(|storage| &storage.snapshot),
            })).map_err(|error|error.to_string())?;
            let capture = archive::NativeArchive {
                bytes,
                backend_id: stored.capabilities.backend_id.clone(),
                format_id: stored.capabilities.format_id.clone(),
            };
            validation::archive::validate_archive(&stored.capabilities, &capture)?;
            Ok(capture)
        } else {
            Ok(stored.snapshot.clone())
        }
    }
    fn review_archive(
        &mut self,
        target: &archive::NativeArchive,
    ) -> Result<archive::Review, String> {
        let stored = self
            .archive
            .as_ref()
            .ok_or("Native archive operations are unsupported by this device")?;
        validation::archive::validate_archive(&stored.capabilities, target)?;
        let changes = if target == &stored.snapshot {
            Vec::new()
        } else {
            vec![archive::SectionChange {
                id: "archive".into(),
                label: "Native archive".into(),
                count: None,
            }]
        };
        Ok(archive::Review {
            before: stored.snapshot.clone(),
            target: target.clone(),
            changes,
        })
    }

    fn apply_archive(
        &mut self,
        expected: &archive::NativeArchive,
        target: &archive::NativeArchive,
        _backup_dir: &Path,
    ) -> Result<archive::NativeArchive, ApplyFailure> {
        let reject = |message| ApplyFailure {
            message,
            recovery: Recovery::NotAttempted,
        };
        let stored = self.archive.as_mut().ok_or_else(|| {
            reject("Native archive operations are unsupported by this device".into())
        })?;
        validation::archive::validate_archive(&stored.capabilities, expected).map_err(reject)?;
        validation::archive::validate_archive(&stored.capabilities, target).map_err(reject)?;
        if &stored.snapshot != expected {
            return Err(reject("Stale expected native archive".into()));
        }
        stored.snapshot = target.clone();
        Ok(target.clone())
    }
}

/// A small keyboard with deliberately different geometry and layers from Nia87.
fn demo_descriptor() -> Descriptor {
    Descriptor {
        backend_id: "memory".into(),
        device_name: "Demo keyboard".into(),
        keys: ["Alpha", "Beta", "Fixed"]
            .into_iter()
            .enumerate()
            .map(|(index, id)| PhysicalKey {
                id: id.into(),
                label: id.into(),
                x: index as f32,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                visible: true,
                writable: index != 2,
            })
            .collect(),
        layers: ["Typing", "Navigation", "Studio"]
            .into_iter()
            .map(|id| Layer {
                id: id.into(),
                label: id.into(),
            })
            .collect(),
        actions: [
            ("A", Action::Key(4)),
            ("B", Action::Key(5)),
            ("Disabled", Action::Disabled),
        ]
        .into_iter()
        .map(|(label, action)| ActionChoice {
            label: label.into(),
            action,
            category: ActionCategory::Alphanumeric,
        })
        .collect(),
        shortcuts: None,
    }
}

fn demo_macro_capabilities() -> macros::Capabilities {
    let slots = ["Greeting", "Spare", "Preserved"]
        .into_iter()
        .map(|id| macros::Choice {
            id: id.into(),
            label: id.into(),
        })
        .collect::<Vec<_>>();
    let bindings = slots
        .iter()
        .map(|slot| macros::Binding {
            slot: slot.id.clone(),
            id: format!("play-{}", slot.id),
            label: format!("Play {}", slot.label),
            action: Action::Named {
                id: format!("play-{}", slot.id),
            },
            required_repeat_count: None,
        })
        .collect();
    macros::Capabilities {
        backend_id: "memory".into(),
        slots,
        repeat_counts: 0..=65535,
        editable_repeat_counts: 1..=65535,
        delays_ms: 0..=65535,
        keys: Some(4..=231),
        buttons: Vec::new(),
        movement: None,
        backend_actions: Vec::new(),
        bindings,
        byte_budget: Some(macros::ByteBudget {
            limit: 250,
            overhead: 3,
            key: 3,
            button: 3,
            movement: 5,
            backend: 3,
            inline_delays: 0..=254,
            extended_delay: 3,
        }),
    }
}
pub fn demo() -> Result<MemoryDevice, String> {
    let descriptor = demo_descriptor();
    let state = State {
        revision: vec![1],
        bindings: descriptor
            .layers
            .iter()
            .map(|layer| {
                (
                    layer.id.clone(),
                    descriptor
                        .keys
                        .iter()
                        .map(|key| (key.id.clone(), Action::Key(4)))
                        .collect(),
                )
            })
            .collect(),
    };
    let capabilities = demo_macro_capabilities();
    let snapshots = capabilities
        .slots
        .iter()
        .enumerate()
        .map(|(index, slot)| macros::Snapshot {
            backend_id: capabilities.backend_id.clone(),
            slot: slot.id.clone(),
            revision: vec![index as u8, 0],
            content: match index {
                0 => macros::Content::Editable(macros::Program {
                    repeat_count: 1,
                    events: vec![
                        macros::Event {
                            action: macros::Action::Key {
                                usage: 4,
                                pressed: true,
                            },
                            delay_ms: 20,
                        },
                        macros::Event {
                            action: macros::Action::Key {
                                usage: 4,
                                pressed: false,
                            },
                            delay_ms: 0,
                        },
                    ],
                }),
                1 => macros::Content::Editable(macros::Program {
                    repeat_count: 0,
                    events: vec![],
                }),
                _ => macros::Content::Opaque {
                    reason: "Unrecognized demo macro bytes".into(),
                },
            },
        })
        .collect();
    let lighting_capabilities = lighting::Capabilities {
        backend_id: "memory".into(),
        host_modes: vec![],
        effects: vec![
            lighting::Effect {
                id: "steady".into(),
                label: "Steady".into(),
                brightness: Some(1..=5),
                speed: None,
                options: vec![],
                color: Some(lighting::ColorCapability::Fixed),
            },
            lighting::Effect {
                id: "picture".into(),
                label: "Per-key colors".into(),
                brightness: Some(1..=5),
                speed: None,
                options: vec![],
                color: None,
            },
        ],
    };
    let lighting_snapshot = lighting::Snapshot {
        picture_context: b"steady".to_vec(),
        backend_id: "memory".into(),
        revision: vec![10, 0xaa],
        evidence: lighting::Evidence::Readback,
        content: lighting::Content::Editable(lighting::Setting {
            effect: "steady".into(),
            brightness: Some(3),
            speed: None,
            option: None,
            color: Some(lighting::Color::Rgb([40, 100, 180])),
        }),
    };
    let picture_capabilities = picture::Capabilities {
        backend_id: "memory".into(),
        keys: vec!["Alpha".into(), "Beta".into()],
        lighting_effect: Some("picture".into()),
    };
    let picture_snapshot = picture::Snapshot {
        backend_id: "memory".into(),
        revision: vec![20, 0xbb],
        context_revision: b"steady".to_vec(),
        evidence: picture::Evidence::Readback,
        content: picture::Content::Editable(BTreeMap::from([
            ("Alpha".into(), [20, 30, 40]),
            ("Beta".into(), [60, 70, 80]),
        ])),
    };
    let settings_capabilities = settings::Capabilities {
        backend_id: "memory".into(),
        fields: vec![
            settings::Field {
                id: "indicator".into(),
                label: "Indicator".into(),
                kind: settings::Kind::Toggle,
            },
            settings::Field {
                id: "sleep".into(),
                label: "Sleep after".into(),
                kind: settings::Kind::Number {
                    min: 1,
                    max: 10,
                    step: 1,
                    unit: "minutes".into(),
                    disabled_zero: true,
                },
            },
        ],
    };
    let settings_snapshot = settings::Snapshot {
        backend_id: "memory".into(),
        revision: vec![30, 0xcc],
        content: settings::Content::Editable(BTreeMap::from([
            ("indicator".into(), settings::Value::Toggle(true)),
            ("sleep".into(), settings::Value::Number(5)),
        ])),
    };
    MemoryDevice::new(descriptor, state)?
        .with_macros(capabilities, snapshots)?
        .with_lighting(lighting_capabilities, lighting_snapshot)?
        .with_picture(picture_capabilities, picture_snapshot)?
        .with_settings(settings_capabilities, settings_snapshot)?
        .with_archive(
            archive::ArchiveCapabilities {
                backend_id: "memory".into(),
                format_id: "memory-demo-v1".into(),
                max_bytes: 1024 * 1024,
            },
            archive::NativeArchive {
                backend_id: "memory".into(),
                format_id: "memory-demo-v1".into(),
                bytes: b"{}".to_vec(),
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::model::keymap::{Action, Layer, PhysicalKey};
    use std::collections::BTreeMap;

    #[test]
    fn demo_macros_keep_configured_free_and_opaque_slots_distinct() {
        let mut device = demo().unwrap();
        let caps = device.macro_capabilities().unwrap().clone();
        assert_eq!(caps.slots.len(), 3);
        assert_eq!(caps.editable_repeat_counts, 1..=65535);
        assert!(
            matches!(device.read_macro("Greeting").unwrap().content, macros::Content::Editable(program) if !program.events.is_empty())
        );
        let free = device.read_macro("Spare").unwrap();
        assert!(
            matches!(&free.content, macros::Content::Editable(program) if program.repeat_count == 0 && program.events.is_empty())
        );
        assert!(
            device
                .apply_macro(
                    &free,
                    &macros::Program {
                        repeat_count: 0,
                        events: vec![]
                    },
                    Path::new("unused")
                )
                .is_err()
        );
        assert!(matches!(
            device.read_macro("Preserved").unwrap().content,
            macros::Content::Opaque { .. }
        ));
        assert_eq!(device.read_macro("Spare").unwrap(), free);
        for binding in &caps.bindings {
            assert!(
                !device
                    .descriptor()
                    .actions
                    .iter()
                    .any(|choice| choice.action == binding.action)
            );
        }
    }
    #[test]
    fn demo_bindings_require_macro_workflow_and_still_save_and_assign() {
        use crate::Executor;
        use byakko_core::session::{Outcome, Session};
        use std::time::Duration;
        let device = demo().unwrap();
        let caps = device.macro_capabilities().unwrap().clone();
        let mut session = Session::new(device.descriptor().clone())
            .unwrap()
            .with_macros(caps.clone())
            .unwrap();
        let worker = Executor::spawn(device, Default::default()).unwrap();
        worker.set_generation(session.connect().unwrap());
        worker.try_submit(session.read().unwrap()).unwrap();
        assert_eq!(
            session.accept(worker.receive(Some(Duration::from_secs(2))).unwrap()),
            Outcome::Loaded
        );
        for binding in &caps.bindings {
            assert!(
                session
                    .edit(Change {
                        layer: "Typing".into(),
                        key: "Alpha".into(),
                        action: binding.action.clone()
                    })
                    .is_err()
            );
        }
        worker.try_submit(session.read_macro().unwrap()).unwrap();
        assert_eq!(
            session.accept(worker.receive(Some(Duration::from_secs(2))).unwrap()),
            Outcome::MacroLoaded
        );
        session.edit_macro(macros::Edit::Repeat(2)).unwrap();
        worker
            .try_submit(
                session
                    .save_and_assign_macro("Typing", "Alpha", "play-Greeting")
                    .unwrap(),
            )
            .unwrap();
        let Outcome::Continue(assign) =
            session.accept(worker.receive(Some(Duration::from_secs(2))).unwrap())
        else {
            panic!("saved macro must request assignment")
        };
        worker.try_submit(assign).unwrap();
        assert_eq!(
            session.accept(worker.receive(Some(Duration::from_secs(2))).unwrap()),
            Outcome::AssignmentSucceeded { macro_saved: true }
        );
        assert_eq!(
            session.keymap().baseline().unwrap().bindings["Typing"]["Alpha"],
            caps.bindings[0].action
        );
    }
    #[test]
    fn demo_setters_distinguish_transport_acceptance_and_preserve_selector_on_parameters() {
        let mut device = demo().unwrap();
        let lighting = device.read_lighting().unwrap();
        let picture = device.read_picture().unwrap();
        let lighting::Content::Editable(mut setting) = lighting.content.clone() else {
            panic!("demo lighting is editable")
        };
        setting.brightness = Some(4);
        let accepted = device
            .apply_lighting(&lighting, &setting, Path::new("unused"))
            .unwrap();
        assert_eq!(accepted.evidence, lighting::Evidence::TransportAccepted);
        assert_eq!(accepted.picture_context, picture.context_revision);
        assert_eq!(
            device.read_lighting().unwrap().evidence,
            lighting::Evidence::Readback
        );
        assert_eq!(
            device.read_picture().unwrap().context_revision,
            picture.context_revision
        );
        // The accepted before-image can be used directly for the next setter.
        setting.brightness = Some(5);
        device
            .apply_lighting(&accepted, &setting, Path::new("unused"))
            .unwrap();
        let picture::Content::Editable(mut colors) = picture.content.clone() else {
            panic!("demo picture is editable")
        };
        colors.insert("Alpha".into(), [1, 2, 3]);
        let accepted = device
            .apply_picture(&picture, &colors, Path::new("unused"))
            .unwrap();
        assert_eq!(accepted.evidence, picture::Evidence::TransportAccepted);
        assert_eq!(
            device.read_picture().unwrap().evidence,
            picture::Evidence::Readback
        );
        colors.insert("Beta".into(), [4, 5, 6]);
        device
            .apply_picture(&accepted, &colors, Path::new("unused"))
            .unwrap();
    }

    #[test]
    fn demo_real_selector_change_rejects_old_picture_context_without_changing_colors() {
        let mut device = demo().unwrap();
        let lighting = device.read_lighting().unwrap();
        let picture = device.read_picture().unwrap();
        let colors = match &picture.content {
            picture::Content::Editable(colors) => colors.clone(),
            _ => panic!("editable picture"),
        };
        let setting = lighting::Setting {
            effect: "picture".into(),
            brightness: Some(3),
            speed: None,
            option: None,
            color: None,
        };
        device
            .apply_lighting(&lighting, &setting, Path::new("unused"))
            .unwrap();
        assert_eq!(device.read_picture().unwrap().context_revision, b"picture");
        assert!(
            device
                .apply_picture(&picture, &colors, Path::new("unused"))
                .is_err()
        );
        assert_eq!(device.read_picture().unwrap().content, picture.content);
    }

    #[test]
    fn demo_scalar_apply_changes_one_field_and_retains_cached_reserved_bytes() {
        let mut device = demo().unwrap();
        let before = device.read_settings().unwrap();
        let after = device
            .apply_setting(
                &before,
                &settings::Edit {
                    id: "sleep".into(),
                    value: settings::Value::Number(0),
                },
                Path::new("unused"),
            )
            .unwrap();
        let settings::Content::Editable(values) = &after.content else {
            panic!("editable settings")
        };
        assert_eq!(values["sleep"], settings::Value::Number(0));
        assert_eq!(values["indicator"], settings::Value::Toggle(true));
        assert!(after.revision.starts_with(&before.revision));
        assert!(
            device
                .apply_setting(
                    &after,
                    &settings::Edit {
                        id: "sleep".into(),
                        value: settings::Value::Number(11)
                    },
                    Path::new("unused")
                )
                .is_err()
        );
        assert_eq!(device.read_settings().unwrap(), after);
    }
    #[test]
    fn demo_session_exposes_each_advertised_feature() {
        let device = demo().unwrap();
        let session = device.session().unwrap();
        assert_eq!(
            session.macros().unwrap().capabilities(),
            device.macro_capabilities().unwrap()
        );
        assert_eq!(
            session.lighting().unwrap().capabilities(),
            device.lighting_capabilities().unwrap()
        );
        assert_eq!(
            session.picture().unwrap().capabilities(),
            device.picture_capabilities().unwrap()
        );
        assert_eq!(
            session.settings().unwrap().capabilities(),
            device.settings_capabilities().unwrap()
        );
    }
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
            shortcuts: None,
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
            editable_repeat_counts: 0..=10,
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
            host_modes: vec![],
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
            picture_context: vec![],
            evidence: byakko_core::model::SnapshotEvidence::Readback,
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

    #[test]
    fn picture_conflict_and_invalid_color_map_leave_snapshot_unchanged() {
        let (descriptor, state) = fixture();
        let caps = picture::Capabilities {
            backend_id: "memory".into(),
            keys: vec!["editable".into()],
            lighting_effect: None,
        };
        let initial = picture::Snapshot {
            evidence: byakko_core::model::SnapshotEvidence::Readback,
            backend_id: "memory".into(),
            revision: vec![0x12, 0x34],
            context_revision: Vec::new(),
            content: picture::Content::Editable(BTreeMap::from([("editable".into(), [1, 2, 3])])),
        };
        let mut device = MemoryDevice::new(descriptor.clone(), state.clone())
            .unwrap()
            .with_picture(caps.clone(), initial.clone())
            .unwrap();
        let mut stale = initial.clone();
        stale.revision.push(0xff);
        let desired = BTreeMap::from([("editable".into(), [4, 5, 6])]);
        assert!(
            device
                .apply_picture(&stale, &desired, Path::new("ignored"))
                .is_err()
        );
        let missing = BTreeMap::new();
        assert!(
            device
                .apply_picture(&initial, &missing, Path::new("ignored"))
                .is_err()
        );
        assert_eq!(device.read_picture().unwrap(), initial);
        let next = device
            .apply_picture(&initial, &desired, Path::new("ignored"))
            .unwrap();
        assert_eq!(next.content, picture::Content::Editable(desired));
        assert_eq!(next.revision.len(), initial.revision.len() + 8);
        assert!(
            device
                .apply_picture(
                    &initial,
                    &BTreeMap::from([("editable".into(), [7, 8, 9])]),
                    Path::new("ignored")
                )
                .is_err()
        );

        let opaque = picture::Snapshot {
            content: picture::Content::Opaque {
                reason: "unknown".into(),
            },
            ..initial.clone()
        };
        let mut opaque_device = MemoryDevice::new(descriptor, state)
            .unwrap()
            .with_picture(caps, opaque.clone())
            .unwrap();
        assert!(
            opaque_device
                .apply_picture(
                    &opaque,
                    &BTreeMap::from([("editable".into(), [0; 3])]),
                    Path::new("ignored")
                )
                .is_err()
        );
        assert_eq!(opaque_device.read_picture().unwrap(), opaque);
    }

    fn settings_fixture() -> (
        Descriptor,
        State,
        settings::Capabilities,
        settings::Snapshot,
    ) {
        let (descriptor, state) = fixture();
        let capabilities = settings::Capabilities {
            backend_id: "memory".into(),
            fields: vec![
                settings::Field {
                    id: "enabled".into(),
                    label: "Enabled".into(),
                    kind: settings::Kind::Toggle,
                },
                settings::Field {
                    id: "timer".into(),
                    label: "Timer".into(),
                    kind: settings::Kind::Number {
                        min: 1,
                        max: 60,
                        step: 1,
                        unit: "min".into(),
                        disabled_zero: true,
                    },
                },
            ],
        };
        let snapshot = settings::Snapshot {
            backend_id: "memory".into(),
            revision: vec![0xaa],
            content: settings::Content::Editable(BTreeMap::from([
                ("enabled".into(), settings::Value::Toggle(false)),
                ("timer".into(), settings::Value::Number(5)),
            ])),
        };
        (descriptor, state, capabilities, snapshot)
    }

    #[test]
    fn one_field_memory_setting_apply_checks_baseline_and_constraints() {
        let (descriptor, state, capabilities, initial) = settings_fixture();
        let mut device = MemoryDevice::new(descriptor, state)
            .unwrap()
            .with_settings(capabilities, initial.clone())
            .unwrap();
        let edit = settings::Edit {
            id: "timer".into(),
            value: settings::Value::Number(6),
        };
        let mut stale = initial.clone();
        stale.revision.push(0xff);
        assert!(
            device
                .apply_setting(&stale, &edit, Path::new("ignored"))
                .is_err()
        );
        let invalid = settings::Edit {
            id: "timer".into(),
            value: settings::Value::Number(61),
        };
        assert!(
            device
                .apply_setting(&initial, &invalid, Path::new("ignored"))
                .is_err()
        );
        assert_eq!(device.read_settings().unwrap(), initial);
        let updated = device
            .apply_setting(&initial, &edit, Path::new("ignored"))
            .unwrap();
        assert_eq!(
            updated.content,
            settings::Content::Editable(BTreeMap::from([
                ("enabled".into(), settings::Value::Toggle(false)),
                ("timer".into(), settings::Value::Number(6)),
            ]))
        );
        assert_eq!(updated.revision.len(), initial.revision.len() + 8);
        assert!(
            device
                .apply_setting(&initial, &edit, Path::new("ignored"))
                .is_err()
        );
    }
}
