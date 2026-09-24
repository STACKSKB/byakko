use super::{Activity, Command, Session, Status};
use crate::lighting::{Capabilities, Edit, Setting, editor::Editor};

impl Session {
    pub fn with_lighting(mut self, capabilities: Capabilities) -> Result<Self, String> {
        if self.generation != 0 || self.lighting.is_some() {
            return Err("Lighting capabilities must be supplied once before connecting".into());
        }
        if capabilities.backend_id != self.descriptor.backend_id {
            return Err("Lighting capabilities belong to a different backend".into());
        }
        self.lighting = Some(Editor::new(capabilities)?);
        self.select_default_host_mode();
        Ok(self)
    }
    pub fn lighting(&self) -> Option<&Editor> {
        self.lighting.as_ref()
    }
    fn lighting_editor(&mut self) -> Result<&mut Editor, String> {
        self.require_idle()?;
        self.lighting
            .as_mut()
            .ok_or_else(|| "Device does not support lighting editing".into())
    }
    pub fn stage_lighting(&mut self, setting: Setting) -> Result<(), String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        self.lighting_editor()?.stage(setting)
    }
    pub fn edit_lighting(&mut self, edit: Edit) -> Result<(), String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        self.lighting_editor()?.edit(edit)
    }
    pub fn revert_lighting(&mut self) -> Result<(), String> {
        self.lighting_editor()?.revert()
    }
    pub fn request_lighting_read(&mut self) -> Result<Command, String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        self.lighting_editor()?;
        let operation = self.operation()?;
        self.activity = Activity::ReadLighting { operation };
        Ok(Command::ReadLighting {
            generation: self.generation,
            operation,
        })
    }
    pub fn request_lighting_apply(&mut self) -> Result<Command, String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        let (expected, desired) = self.lighting_editor()?.request_apply()?;
        let operation = self.operation()?;
        self.activity = Activity::ApplyLighting { operation };
        // This transaction verifies lighting only. Keep the last observed
        // keymap and unrelated drafts in this single-owner session.
        self.invalidate_archive();
        Ok(Command::ApplyLighting {
            generation: self.generation,
            operation,
            expected,
            desired,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Action, Descriptor, Layer, PhysicalKey, State,
        lighting::{self, Color, ColorCapability, Content, Effect, Snapshot},
        session::{Acceptance, ApplyFailure, Completion, Problem, Recovery},
    };

    fn caps() -> Capabilities {
        Capabilities {
            backend_id: "memory".into(),
            host_modes: vec![],
            effects: vec![Effect {
                id: "pulse".into(),
                label: "Pulse".into(),
                brightness: Some(0..=10),
                speed: Some(1..=5),
                options: vec![],
                color: Some(ColorCapability::FixedOrRainbow),
            }],
        }
    }
    fn setting(value: u16) -> Setting {
        Setting {
            brightness: Some(value),
            ..lighting::default_setting(&caps(), "pulse").unwrap()
        }
    }
    fn snapshot(revision: u8, value: u16) -> Snapshot {
        Snapshot {
            backend_id: "memory".into(),
            revision: vec![revision],
            content: Content::Editable(setting(value)),
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
            shortcuts: None,
        })
        .unwrap()
        .with_lighting(caps())
        .unwrap()
    }
    fn read(session: &mut Session, result: Result<Snapshot, String>) {
        let Command::ReadLighting {
            generation,
            operation,
        } = session.request_lighting_read().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(
            session.accept(Completion::ReadLighting {
                generation,
                operation,
                result
            }),
            Acceptance::Accepted
        );
    }
    fn read_keymap(session: &mut Session) {
        let Command::Read {
            generation,
            operation,
        } = session.request_read().unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion::Read {
            generation,
            operation,
            result: Ok(State {
                revision: vec![1],
                bindings: std::collections::BTreeMap::from([(
                    "base".into(),
                    std::collections::BTreeMap::from([("a".into(), Action::Disabled)]),
                )]),
            }),
        });
        assert_eq!(session.status(), &Status::Ready);
    }
    #[test]
    fn successful_lighting_apply_preserves_last_observed_keymap() {
        let mut session = session();
        session.connect().unwrap();
        read_keymap(&mut session);
        read(&mut session, Ok(snapshot(1, 10)));
        session.stage_lighting(setting(5)).unwrap();
        let original = session.baseline().cloned();
        let Command::ApplyLighting {
            generation,
            operation,
            ..
        } = session.request_lighting_apply().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(session.status(), &Status::Ready);
        session.accept(Completion::ApplyLighting {
            generation,
            operation,
            result: Ok(snapshot(2, 5)),
        });
        assert_eq!(session.status(), &Status::Ready);
        assert_eq!(session.baseline(), original.as_ref());
        assert_eq!(
            session.lighting().unwrap().status(),
            &lighting::editor::Status::Ready
        );
    }
    #[test]
    fn brightness_write_keeps_picture_verified_but_selector_write_invalidates_it() {
        use crate::picture::{self, Content as PictureContent};
        let mut session = session()
            .with_picture(picture::Capabilities {
                backend_id: "memory".into(),
                keys: vec!["a".into()],
                lighting_effect: Some("pulse".into()),
            })
            .unwrap();
        session.connect().unwrap();
        read(&mut session, Ok(snapshot(1, 10)));
        let picture = picture::Snapshot {
            backend_id: "memory".into(),
            revision: vec![1],
            context_revision: vec![1],
            content: PictureContent::Editable(std::collections::BTreeMap::from([(
                "a".into(),
                [1, 2, 3],
            )])),
        };
        let Command::ReadPicture {
            generation,
            operation,
        } = session.request_picture_read().unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion::ReadPicture {
            generation,
            operation,
            result: Ok(picture.clone()),
        });
        session.stage_lighting(setting(5)).unwrap();
        let Command::ApplyLighting {
            generation,
            operation,
            ..
        } = session.request_lighting_apply().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(
            session.picture().unwrap().status(),
            &picture::editor::Status::Ready
        );
        session.accept(Completion::ApplyLighting {
            generation,
            operation,
            result: Ok(snapshot(2, 5)),
        });
        assert_eq!(session.picture().unwrap().baseline(), Some(&picture));
        assert_eq!(
            session.picture().unwrap().status(),
            &picture::editor::Status::Ready
        );

        let mut other_caps = caps();
        other_caps.effects.push(Effect {
            id: "other".into(),
            label: "Other".into(),
            brightness: Some(0..=10),
            speed: Some(1..=5),
            options: vec![],
            color: Some(ColorCapability::FixedOrRainbow),
        });
        let mut session = Session::new(session.descriptor().clone())
            .unwrap()
            .with_lighting(other_caps)
            .unwrap()
            .with_picture(picture::Capabilities {
                backend_id: "memory".into(),
                keys: vec!["a".into()],
                lighting_effect: Some("pulse".into()),
            })
            .unwrap();
        session.connect().unwrap();
        read(&mut session, Ok(snapshot(1, 10)));
        let Command::ReadPicture {
            generation,
            operation,
        } = session.request_picture_read().unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion::ReadPicture {
            generation,
            operation,
            result: Ok(picture),
        });
        session.edit_lighting(Edit::Effect("other".into())).unwrap();
        let desired = session.lighting().unwrap().draft().unwrap().clone();
        let Command::ApplyLighting {
            generation,
            operation,
            ..
        } = session.request_lighting_apply().unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion::ApplyLighting {
            generation,
            operation,
            result: Ok(Snapshot {
                backend_id: "memory".into(),
                revision: vec![2],
                content: Content::Editable(desired),
            }),
        });
        assert!(matches!(
            session.picture().unwrap().status(),
            picture::editor::Status::Unverified { .. }
        ));
    }
    #[test]
    fn scalar_successes_keep_other_drafts_and_failure_invalidates_ready_caches() {
        use crate::settings::{self, Field, Kind, Value};
        let mut session = session()
            .with_settings(settings::Capabilities {
                backend_id: "memory".into(),
                fields: vec![Field {
                    id: "enabled".into(),
                    label: "Enabled".into(),
                    kind: Kind::Toggle,
                }],
            })
            .unwrap();
        session.connect().unwrap();
        read_keymap(&mut session);
        read(&mut session, Ok(snapshot(1, 10)));
        let Command::ReadSettings {
            generation,
            operation,
        } = session.request_settings_read().unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion::ReadSettings {
            generation,
            operation,
            result: Ok(settings::Snapshot {
                backend_id: "memory".into(),
                revision: vec![1],
                content: settings::Content::Editable(std::collections::BTreeMap::from([(
                    "enabled".into(),
                    Value::Toggle(false),
                )])),
            }),
        });
        session
            .edit_setting(settings::Edit {
                id: "enabled".into(),
                value: Value::Toggle(true),
            })
            .unwrap();
        session.stage_lighting(setting(5)).unwrap();
        let Command::ApplyLighting {
            generation,
            operation,
            ..
        } = session.request_lighting_apply().unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion::ApplyLighting {
            generation,
            operation,
            result: Ok(snapshot(2, 5)),
        });
        assert_eq!(
            session.settings().unwrap().status(),
            &settings::editor::Status::Ready
        );
        assert_eq!(
            session.settings().unwrap().draft().unwrap()["enabled"],
            Value::Toggle(true)
        );
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
            result: Ok(settings::Snapshot {
                backend_id: "memory".into(),
                revision: vec![2],
                content: settings::Content::Editable(std::collections::BTreeMap::from([(
                    "enabled".into(),
                    Value::Toggle(true),
                )])),
            }),
        });
        assert_eq!(
            session.lighting().unwrap().status(),
            &lighting::editor::Status::Ready
        );
        assert_eq!(session.status(), &Status::Ready);
        session
            .stage(crate::Change {
                layer: "base".into(),
                key: "a".into(),
                action: Action::Key(4),
            })
            .unwrap();
        let Command::Apply {
            generation,
            operation,
            ..
        } = session.request_apply().unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion::Apply {
            generation,
            operation,
            result: Ok(State {
                revision: vec![2],
                bindings: std::collections::BTreeMap::from([(
                    "base".into(),
                    std::collections::BTreeMap::from([("a".into(), Action::Key(4))]),
                )]),
            }),
        });
        assert_eq!(session.status(), &Status::Ready);
        assert_eq!(
            session.lighting().unwrap().status(),
            &lighting::editor::Status::Ready
        );
        assert_eq!(
            session.settings().unwrap().status(),
            &settings::editor::Status::Ready
        );
        session.stage_lighting(setting(4)).unwrap();
        let Command::ApplyLighting {
            generation,
            operation,
            ..
        } = session.request_lighting_apply().unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion::ApplyLighting {
            generation,
            operation,
            result: Err(ApplyFailure {
                message: "uncertain".into(),
                recovery: Recovery::Unverified,
            }),
        });
        assert!(matches!(
            session.status(),
            Status::Unverified {
                problem: Problem::ReadRequired
            }
        ));
        assert!(matches!(
            session.settings().unwrap().status(),
            settings::editor::Status::Unverified {
                problem: Problem::ReadRequired
            }
        ));
    }
    #[test]
    fn opaque_and_invalid_edits_do_not_create_drafts() {
        let mut session = session();
        session.connect().unwrap();
        let opaque = Snapshot {
            backend_id: "memory".into(),
            revision: vec![0, 255],
            content: Content::Opaque {
                reason: "unrecognized bytes".into(),
            },
        };
        read(&mut session, Ok(opaque.clone()));
        assert_eq!(session.lighting().unwrap().baseline(), Some(&opaque));
        assert!(session.lighting().unwrap().draft().is_none());
        assert!(session.stage_lighting(setting(5)).is_err());
        read(&mut session, Ok(snapshot(1, 10)));
        assert!(session.edit_lighting(Edit::Brightness(11)).is_err());
        assert_eq!(session.lighting().unwrap().draft(), Some(&setting(10)));
        assert_eq!(
            lighting::edit(&caps(), &setting(5), Edit::Effect("pulse".into())).unwrap(),
            setting(5)
        );
        assert!(lighting::edit(&caps(), &setting(5), Edit::Color(Color::Rgb([1, 2, 3]))).is_ok());
    }
    #[test]
    fn dirty_reconnect_conflict_and_mismatched_apply_preserve_draft() {
        let mut session = session();
        session.connect().unwrap();
        read(&mut session, Ok(snapshot(1, 10)));
        session.stage_lighting(setting(5)).unwrap();
        session.disconnect();
        session.connect().unwrap();
        read(&mut session, Ok(snapshot(1, 10)));
        assert!(session.lighting().unwrap().dirty());
        session.disconnect();
        session.connect().unwrap();
        read(&mut session, Ok(snapshot(2, 9)));
        assert!(matches!(
            session.lighting().unwrap().status(),
            lighting::editor::Status::Conflict { .. }
        ));
        assert_eq!(
            session.reconnect_cautions(),
            vec![super::super::ReconnectCaution {
                surface: super::super::ReconnectSurface::Lighting,
                cause: super::super::ReconnectCause::Conflict,
            }]
        );
        assert_eq!(session.lighting().unwrap().draft(), Some(&setting(5)));
        read(&mut session, Ok(snapshot(1, 10)));
        let command = session.request_lighting_apply().unwrap();
        let encoded = serde_json::to_vec(&command).unwrap();
        assert_eq!(
            serde_json::from_slice::<Command>(&encoded).unwrap(),
            command
        );
        let Command::ApplyLighting {
            generation,
            operation,
            expected,
            desired,
        } = command
        else {
            unreachable!()
        };
        assert_eq!(expected, snapshot(1, 10));
        assert_eq!(desired, setting(5));
        assert_eq!(
            session.accept(Completion::ReadLighting {
                generation,
                operation,
                result: Ok(snapshot(3, 5))
            }),
            Acceptance::IgnoredStale
        );
        assert_eq!(
            session.accept(Completion::ApplyLighting {
                generation: generation + 1,
                operation,
                result: Ok(snapshot(3, 5))
            }),
            Acceptance::IgnoredStale
        );
        let completion = Completion::ApplyLighting {
            generation,
            operation,
            result: Ok(snapshot(3, 4)),
        };
        assert_eq!(
            serde_json::from_slice::<Completion>(&serde_json::to_vec(&completion).unwrap())
                .unwrap(),
            completion
        );
        assert_eq!(session.accept(completion), Acceptance::Accepted);
        assert!(matches!(
            session.lighting().unwrap().status(),
            lighting::editor::Status::Unverified {
                problem: Problem::ApplyReadbackMismatch
            }
        ));
        assert_eq!(
            session.reconnect_cautions(),
            vec![super::super::ReconnectCaution {
                surface: super::super::ReconnectSurface::Lighting,
                cause: super::super::ReconnectCause::ApplyReadbackMismatch,
            }]
        );
        assert_eq!(session.lighting().unwrap().draft(), Some(&setting(5)));
    }
    #[test]
    fn shared_activity_and_failed_write_invalidate_other_surfaces() {
        let mut session = session();
        session.connect().unwrap();
        read(&mut session, Ok(snapshot(1, 10)));
        session.stage_lighting(setting(5)).unwrap();
        assert!(session.dirty());
        let Command::Read {
            generation,
            operation,
        } = session.request_read().unwrap()
        else {
            unreachable!()
        };
        assert!(session.request_lighting_apply().is_err());
        session.accept(Completion::Read {
            generation,
            operation,
            result: Err("read failed".into()),
        });
        read_keymap(&mut session);
        assert!(matches!(
            session.lighting().unwrap().status(),
            lighting::editor::Status::Ready
        ));
        let Command::ApplyLighting {
            generation,
            operation,
            ..
        } = session.request_lighting_apply().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(session.status(), &Status::Ready);
        assert!(session.reconnect_cautions().is_empty());
        assert_eq!(
            session.accept(Completion::ApplyLighting {
                generation,
                operation,
                result: Err(ApplyFailure {
                    message: "failed".into(),
                    recovery: Recovery::Unverified
                })
            }),
            Acceptance::Accepted
        );
        assert_eq!(session.lighting().unwrap().draft(), Some(&setting(5)));
        assert!(matches!(
            session.status(),
            Status::Unverified {
                problem: Problem::ReadRequired
            }
        ));
        assert_eq!(
            session.reconnect_cautions(),
            vec![super::super::ReconnectCaution {
                surface: super::super::ReconnectSurface::Lighting,
                cause: super::super::ReconnectCause::Apply(&ApplyFailure {
                    message: "failed".into(),
                    recovery: Recovery::Unverified,
                }),
            }]
        );
    }
}
