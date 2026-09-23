use super::{Activity, Command, Problem, Session, Status};
use crate::settings::{Capabilities, Edit, editor::Editor};

impl Session {
    pub fn with_settings(mut self, capabilities: Capabilities) -> Result<Self, String> {
        if self.generation != 0 || self.settings.is_some() {
            return Err("Settings capabilities must be supplied once before connecting".into());
        }
        if capabilities.backend_id != self.descriptor.backend_id {
            return Err("Settings capabilities belong to a different backend".into());
        }
        self.settings = Some(Editor::new(capabilities)?);
        Ok(self)
    }
    pub fn settings(&self) -> Option<&Editor> {
        self.settings.as_ref()
    }
    fn settings_editor(&mut self) -> Result<&mut Editor, String> {
        self.require_idle()?;
        self.settings
            .as_mut()
            .ok_or_else(|| "Device does not support settings editing".into())
    }
    pub fn edit_setting(&mut self, edit: Edit) -> Result<(), String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        self.settings_editor()?.edit(edit)
    }
    pub fn revert_settings(&mut self) -> Result<(), String> {
        self.settings_editor()?.revert()
    }
    pub fn request_settings_read(&mut self) -> Result<Command, String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        self.settings_editor()?;
        let operation = self.operation()?;
        self.activity = Activity::ReadSettings { operation };
        Ok(Command::ReadSettings {
            generation: self.generation,
            operation,
        })
    }
    pub fn request_setting_apply(&mut self) -> Result<Command, String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        let (expected, edit) = self.settings_editor()?.request_apply()?;
        let operation = self.operation()?;
        self.activity = Activity::ApplySetting { operation };
        self.status = Status::Unverified {
            problem: Problem::ReadRequired,
        };
        self.invalidate_macros();
        self.invalidate_lighting();
        self.invalidate_picture();
        self.invalidate_archive();
        Ok(Command::ApplySetting {
            generation: self.generation,
            operation,
            expected,
            edit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Descriptor, Layer, PhysicalKey,
        session::{Acceptance, ApplyFailure, Completion, Recovery},
        settings::{Content, Field, Kind, Snapshot, Value},
    };
    use std::collections::BTreeMap;

    fn caps() -> Capabilities {
        Capabilities {
            backend_id: "memory".into(),
            fields: vec![
                Field {
                    id: "enabled".into(),
                    label: "Enabled".into(),
                    kind: Kind::Toggle,
                },
                Field {
                    id: "timer".into(),
                    label: "Timer".into(),
                    kind: Kind::Number {
                        min: 10,
                        max: 60,
                        step: 5,
                        unit: "min".into(),
                        disabled_zero: true,
                    },
                },
            ],
        }
    }
    fn session() -> Session {
        Session::new(Descriptor {
            backend_id: "memory".into(),
            device_name: "test".into(),
            keys: vec![PhysicalKey {
                id: "a".into(),
                label: "A".into(),
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                visible: true,
                writable: true,
            }],
            layers: vec![Layer {
                id: "base".into(),
                label: "Base".into(),
            }],
            actions: vec![],
        })
        .unwrap()
        .with_settings(caps())
        .unwrap()
    }
    fn snapshot(revision: u8, timer: u16) -> Snapshot {
        Snapshot {
            backend_id: "memory".into(),
            revision: vec![revision],
            content: Content::Editable(BTreeMap::from([
                ("enabled".into(), Value::Toggle(false)),
                ("timer".into(), Value::Number(timer)),
            ])),
        }
    }
    fn read(session: &mut Session, snapshot: Snapshot) {
        let Command::ReadSettings {
            generation,
            operation,
        } = session.request_settings_read().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(
            session.accept(Completion::ReadSettings {
                generation,
                operation,
                result: Ok(snapshot)
            }),
            Acceptance::Accepted
        );
    }
    fn edit(id: &str, value: Value) -> Edit {
        Edit {
            id: id.into(),
            value,
        }
    }
    #[test]
    fn catalog_and_snapshot_validation_reject_invalid_values() {
        let mut invalid = caps();
        invalid.fields[1].kind = Kind::Number {
            min: 10,
            max: 60,
            step: 0,
            unit: "min".into(),
            disabled_zero: true,
        };
        assert!(
            Session::new(session().descriptor().clone())
                .unwrap()
                .with_settings(invalid)
                .is_err()
        );
        let mut session = session();
        session.connect().unwrap();
        read(&mut session, snapshot(1, 11));
        assert!(matches!(
            session.settings().unwrap().status(),
            crate::settings::editor::Status::Unverified { .. }
        ));
        read(&mut session, snapshot(1, 10));
        assert!(
            session
                .edit_setting(edit("timer", Value::Number(11)))
                .is_err()
        );
        assert!(
            session
                .edit_setting(edit("enabled", Value::Number(1)))
                .is_err()
        );
        assert!(!session.settings().unwrap().dirty());
        session
            .edit_setting(edit("timer", Value::Number(0)))
            .unwrap();
        assert_eq!(
            session.settings().unwrap().changes(),
            vec![edit("timer", Value::Number(0))]
        );
    }
    #[test]
    fn only_one_field_can_be_staged_and_apply_carries_one_edit() {
        let mut session = session();
        session.connect().unwrap();
        read(&mut session, snapshot(1, 10));
        session
            .edit_setting(edit("timer", Value::Number(20)))
            .unwrap();
        assert!(
            session
                .edit_setting(edit("enabled", Value::Toggle(true)))
                .is_err()
        );
        session
            .edit_setting(edit("timer", Value::Number(25)))
            .unwrap();
        let Command::ApplySetting {
            generation,
            operation,
            expected,
            edit: requested_edit,
        } = session.request_setting_apply().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(expected, snapshot(1, 10));
        assert_eq!(
            requested_edit,
            Edit {
                id: "timer".into(),
                value: Value::Number(25)
            }
        );
        assert_eq!(
            session.accept(Completion::ReadSettings {
                generation,
                operation,
                result: Ok(snapshot(2, 25))
            }),
            Acceptance::IgnoredStale
        );
        let mismatch = Completion::ApplySetting {
            generation,
            operation,
            result: Ok(snapshot(2, 20)),
        };
        assert_eq!(
            serde_json::from_slice::<Completion>(&serde_json::to_vec(&mismatch).unwrap()).unwrap(),
            mismatch
        );
        session.accept(mismatch);
        assert!(matches!(
            session.settings().unwrap().status(),
            crate::settings::editor::Status::Unverified {
                problem: Problem::ApplyReadbackMismatch
            }
        ));
        assert_eq!(
            session.settings().unwrap().draft().unwrap()["timer"],
            Value::Number(25)
        );
        read(&mut session, snapshot(1, 10));
        session.revert_settings().unwrap();
        session
            .edit_setting(edit("enabled", Value::Toggle(true)))
            .unwrap();
        assert_eq!(
            session.settings().unwrap().changes(),
            vec![edit("enabled", Value::Toggle(true))]
        );
    }
    #[test]
    fn opaque_conflict_and_failure_preserve_draft() {
        let mut session = session();
        session.connect().unwrap();
        let opaque = Snapshot {
            backend_id: "memory".into(),
            revision: vec![0, 255],
            content: Content::Opaque {
                reason: "unknown bytes".into(),
            },
        };
        read(&mut session, opaque.clone());
        assert_eq!(session.settings().unwrap().baseline(), Some(&opaque));
        assert!(
            session
                .edit_setting(edit("enabled", Value::Toggle(true)))
                .is_err()
        );
        read(&mut session, snapshot(1, 10));
        session
            .edit_setting(edit("timer", Value::Number(20)))
            .unwrap();
        session.disconnect();
        session.connect().unwrap();
        read(&mut session, snapshot(2, 10));
        assert!(matches!(
            session.settings().unwrap().status(),
            crate::settings::editor::Status::Conflict { .. }
        ));
        read(&mut session, snapshot(1, 10));
        let Command::ApplySetting {
            generation,
            operation,
            ..
        } = session.request_setting_apply().unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion::ApplySetting {
            generation,
            operation,
            result: Err(ApplyFailure {
                message: "write failed".into(),
                recovery: Recovery::Failed,
            }),
        });
        assert_eq!(
            session.settings().unwrap().draft().unwrap()["timer"],
            Value::Number(20)
        );
        assert!(session.request_setting_apply().is_err());
    }
}
