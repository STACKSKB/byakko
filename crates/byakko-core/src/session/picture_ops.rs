use super::{Activity, Command, Session, Status};
use crate::picture::{Capabilities, Edit, editor::Editor, validate_capabilities};

impl Session {
    pub fn with_picture(mut self, capabilities: Capabilities) -> Result<Self, String> {
        if self.generation != 0 || self.picture.is_some() {
            return Err("Picture capabilities must be supplied once before connecting".into());
        }
        validate_capabilities(&capabilities, &self.descriptor)?;
        self.picture = Some(Editor::new(capabilities));
        Ok(self)
    }
    pub fn picture(&self) -> Option<&Editor> {
        self.picture.as_ref()
    }
    fn picture_editor(&mut self) -> Result<&mut Editor, String> {
        self.require_idle()?;
        self.picture
            .as_mut()
            .ok_or_else(|| "Device does not support picture editing".into())
    }
    pub fn edit_picture(&mut self, edit: Edit) -> Result<(), String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        self.picture_editor()?.edit(edit)
    }
    pub fn revert_picture(&mut self) -> Result<(), String> {
        self.picture_editor()?.revert()
    }
    pub fn request_picture_read(&mut self) -> Result<Command, String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        self.picture_editor()?;
        let operation = self.operation()?;
        self.activity = Activity::ReadPicture { operation };
        Ok(Command::ReadPicture {
            generation: self.generation,
            operation,
        })
    }
    pub fn request_picture_apply(&mut self) -> Result<Command, String> {
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        let (expected, desired) = self.picture_editor()?.request_apply()?;
        let operation = self.operation()?;
        self.activity = Activity::ApplyPicture { operation };
        self.macro_catalog_operation = None;
        self.invalidate_archive();
        Ok(Command::ApplyPicture {
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
    use crate::session::Problem;
    use crate::{
        Action, Descriptor, Layer, PhysicalKey, State,
        picture::{Channel, Content, Snapshot},
        session::{Acceptance, ApplyFailure, Completion, Recovery},
    };
    use std::collections::BTreeMap;

    fn descriptor() -> Descriptor {
        Descriptor {
            backend_id: "memory".into(),
            device_name: "test".into(),
            keys: [("a", true), ("fn", false)]
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
        }
    }
    fn caps() -> Capabilities {
        Capabilities {
            backend_id: "memory".into(),
            keys: vec!["a".into(), "fn".into()],
            lighting_effect: None,
        }
    }
    fn snapshot(revision: u8, rgb: [u8; 3]) -> Snapshot {
        Snapshot {
            backend_id: "memory".into(),
            revision: vec![revision],
            context_revision: Vec::new(),
            content: Content::Editable(BTreeMap::from([
                ("a".into(), rgb),
                ("fn".into(), [4, 5, 6]),
            ])),
        }
    }
    fn read(session: &mut Session, snapshot: Snapshot) {
        let Command::ReadPicture {
            generation,
            operation,
        } = session.request_picture_read().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(
            session.accept(Completion::ReadPicture {
                generation,
                operation,
                result: Ok(snapshot)
            }),
            Acceptance::Accepted
        );
    }
    #[test]
    fn capability_and_snapshot_shape_reject_invalid_keys_but_allow_fn_led() {
        assert!(
            Session::new(descriptor())
                .unwrap()
                .with_picture(caps())
                .is_ok()
        );
        let mut blank_effect = caps();
        blank_effect.lighting_effect = Some("  ".into());
        assert!(
            Session::new(descriptor())
                .unwrap()
                .with_picture(blank_effect)
                .is_err()
        );
        for keys in [
            vec!["a".into(), "a".into()],
            vec!["a".into(), "missing".into()],
        ] {
            assert!(
                Session::new(descriptor())
                    .unwrap()
                    .with_picture(Capabilities {
                        backend_id: "memory".into(),
                        keys,
                        lighting_effect: None,
                    })
                    .is_err()
            );
        }
        let mut session = Session::new(descriptor())
            .unwrap()
            .with_picture(caps())
            .unwrap();
        session.connect().unwrap();
        let mut invalid = snapshot(1, [1, 2, 3]);
        let Content::Editable(colors) = &mut invalid.content else {
            unreachable!()
        };
        colors.remove("fn");
        read(&mut session, invalid);
        assert!(matches!(
            session.picture().unwrap().status(),
            crate::picture::editor::Status::Unverified { .. }
        ));
        read(&mut session, snapshot(1, [1, 2, 3]));
        session
            .edit_picture(Edit::Channel {
                key: "fn".into(),
                channel: Channel::Green,
                value: 9,
            })
            .unwrap();
        assert!(session.picture().unwrap().dirty());
        let before = session.picture().unwrap().draft().unwrap().clone();
        assert!(
            session
                .edit_picture(Edit::Color {
                    key: "absent".into(),
                    color: [0; 3]
                })
                .is_err()
        );
        assert_eq!(session.picture().unwrap().draft(), Some(&before));
    }
    #[test]
    fn opaque_read_is_lossless_and_not_editable() {
        let mut session = Session::new(descriptor())
            .unwrap()
            .with_picture(caps())
            .unwrap();
        session.connect().unwrap();
        let opaque = Snapshot {
            backend_id: "memory".into(),
            revision: vec![0, 255, 9],
            context_revision: Vec::new(),
            content: Content::Opaque {
                reason: "unknown picture bytes".into(),
            },
        };
        read(&mut session, opaque.clone());
        assert_eq!(session.picture().unwrap().baseline(), Some(&opaque));
        assert!(session.picture().unwrap().draft().is_none());
        assert!(
            session
                .edit_picture(Edit::Color {
                    key: "a".into(),
                    color: [1, 2, 3]
                })
                .is_err()
        );
        assert!(session.request_picture_apply().is_err());
        assert_eq!(
            crate::picture::channels([1, 2, 3]).map(|control| control.value),
            [1, 2, 3]
        );
    }

    #[test]
    fn keymap_refresh_keeps_a_verified_picture_draft() {
        let mut session = Session::new(descriptor())
            .unwrap()
            .with_picture(caps())
            .unwrap();
        session.connect().unwrap();
        read(&mut session, snapshot(1, [1, 2, 3]));
        session
            .edit_picture(Edit::Color {
                key: "a".into(),
                color: [7, 8, 9],
            })
            .unwrap();
        let draft = session.picture().unwrap().draft().unwrap().clone();
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
                bindings: BTreeMap::from([(
                    "base".into(),
                    BTreeMap::from([
                        ("a".into(), Action::Key(4)),
                        ("fn".into(), Action::Disabled),
                    ]),
                )]),
            }),
        });
        assert_eq!(session.picture().unwrap().draft(), Some(&draft));
        assert_eq!(
            session.picture().unwrap().status(),
            &crate::picture::editor::Status::Ready
        );
        assert!(session.request_picture_apply().is_ok());
    }
    #[test]
    fn stale_conflict_and_failed_readback_preserve_draft() {
        let mut session = Session::new(descriptor())
            .unwrap()
            .with_picture(caps())
            .unwrap();
        session.connect().unwrap();
        read(&mut session, snapshot(1, [1, 2, 3]));
        session
            .edit_picture(Edit::Channel {
                key: "a".into(),
                channel: Channel::Red,
                value: 7,
            })
            .unwrap();
        let draft = session.picture().unwrap().draft().unwrap().clone();
        let command = session.request_picture_apply().unwrap();
        assert_eq!(
            serde_json::from_slice::<Command>(&serde_json::to_vec(&command).unwrap()).unwrap(),
            command
        );
        let Command::ApplyPicture {
            generation,
            operation,
            desired,
            ..
        } = command
        else {
            unreachable!()
        };
        assert_eq!(desired, draft);
        assert_eq!(
            session.accept(Completion::ReadPicture {
                generation,
                operation,
                result: Ok(snapshot(2, [7, 2, 3]))
            }),
            Acceptance::IgnoredStale
        );
        assert_eq!(
            session.accept(Completion::ApplyPicture {
                generation,
                operation,
                result: Ok(snapshot(2, [8, 2, 3]))
            }),
            Acceptance::Accepted
        );
        assert!(matches!(
            session.picture().unwrap().status(),
            crate::picture::editor::Status::Unverified {
                problem: Problem::ApplyReadbackMismatch
            }
        ));
        assert_eq!(session.picture().unwrap().draft(), Some(&draft));
        read(&mut session, snapshot(1, [1, 2, 3]));
        session.disconnect();
        session.connect().unwrap();
        read(&mut session, snapshot(3, [1, 2, 3]));
        assert!(matches!(
            session.picture().unwrap().status(),
            crate::picture::editor::Status::Conflict { .. }
        ));
        assert_eq!(session.picture().unwrap().draft(), Some(&draft));
        read(&mut session, snapshot(1, [1, 2, 3]));
        let Command::ApplyPicture {
            generation,
            operation,
            ..
        } = session.request_picture_apply().unwrap()
        else {
            unreachable!()
        };
        let failure = Completion::ApplyPicture {
            generation,
            operation,
            result: Err(ApplyFailure {
                message: "failed".into(),
                recovery: Recovery::Unverified,
            }),
        };
        assert_eq!(
            serde_json::from_slice::<Completion>(&serde_json::to_vec(&failure).unwrap()).unwrap(),
            failure
        );
        session.accept(failure);
        assert_eq!(session.picture().unwrap().draft(), Some(&draft));
    }
}
