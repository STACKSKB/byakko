//! Capture and review opaque native archives without paths or device I/O.
use super::{Activity, Command, Session, Status};
use crate::archive::{
    self, ArchiveCapabilities, ArchiveProblem, ArchiveState, NativeArchive, Review,
};

impl Session {
    pub fn with_archive(mut self, capabilities: ArchiveCapabilities) -> Result<Self, String> {
        if self.generation != 0 || self.archive_capabilities.is_some() {
            return Err("Archive capabilities must be supplied once before connecting".into());
        }
        archive::validate_capabilities(&capabilities)?;
        if capabilities.backend_id != self.descriptor.backend_id {
            return Err("Archive capabilities belong to a different backend".into());
        }
        self.archive_capabilities = Some(capabilities);
        Ok(self)
    }
    pub fn archive_capabilities(&self) -> Option<&ArchiveCapabilities> {
        self.archive_capabilities.as_ref()
    }
    pub fn archive(&self) -> Option<&ArchiveState> {
        self.archive_capabilities
            .as_ref()
            .map(|_| &self.archive_state)
    }
    /// A different local file path must not be presented beside an old review.
    pub fn clear_archive_review(&mut self) {
        if let ArchiveState::Ready(review) = &self.archive_state {
            self.archive_state = ArchiveState::Captured(review.before.clone());
        }
    }
    pub fn request_archive_capture(&mut self) -> Result<Command, String> {
        self.require_idle()?;
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        if self.archive_capabilities.is_none() {
            return Err("Device does not support native archives".into());
        }
        let operation = self.operation()?;
        self.activity = Activity::CaptureArchive { operation };
        self.archive_state = ArchiveState::Unverified {
            problem: ArchiveProblem::ReadRequired,
        };
        Ok(Command::CaptureArchive {
            generation: self.generation,
            operation,
        })
    }
    pub fn request_archive_review(&mut self, target: NativeArchive) -> Result<Command, String> {
        self.require_idle()?;
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        let caps = self
            .archive_capabilities
            .as_ref()
            .ok_or("Device does not support native archives")?;
        archive::validate_archive(caps, &target)?;
        let operation = self.operation()?;
        self.activity = Activity::ReviewArchive {
            operation,
            target: target.clone(),
        };
        self.archive_state = ArchiveState::Unverified {
            problem: ArchiveProblem::ReadRequired,
        };
        Ok(Command::ReviewArchive {
            generation: self.generation,
            operation,
            target,
        })
    }
    pub(super) fn invalidate_archive(&mut self) {
        if matches!(self.archive_state, ArchiveState::Ready(_)) {
            self.archive_state = ArchiveState::Unverified {
                problem: ArchiveProblem::ReadRequired,
            };
        }
    }
    pub(super) fn accept_archive_capture(&mut self, result: Result<NativeArchive, String>) {
        let caps = self
            .archive_capabilities
            .as_ref()
            .expect("pending archive capability");
        self.archive_state = match result {
            Err(reason) => ArchiveState::Unverified {
                problem: ArchiveProblem::Capture(reason),
            },
            Ok(captured) => match archive::validate_archive(caps, &captured) {
                Ok(()) => ArchiveState::Captured(captured),
                Err(reason) => ArchiveState::Unverified {
                    problem: ArchiveProblem::InvalidResult(reason),
                },
            },
        };
    }
    pub(super) fn accept_archive_review(
        &mut self,
        target: NativeArchive,
        result: Result<Review, String>,
    ) {
        let caps = self
            .archive_capabilities
            .as_ref()
            .expect("pending archive capability");
        self.archive_state = match result {
            Err(reason) => ArchiveState::Unverified {
                problem: ArchiveProblem::Review(reason),
            },
            Ok(review) => match archive::validate_review(caps, &review) {
                Err(reason) => ArchiveState::Unverified {
                    problem: ArchiveProblem::InvalidResult(reason),
                },
                Ok(()) if review.target != target => ArchiveState::Unverified {
                    problem: ArchiveProblem::InvalidResult(
                        "Reviewed archive differs from requested target".into(),
                    ),
                },
                Ok(()) => ArchiveState::Ready(review),
            },
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Action, Change, Descriptor, Layer, PhysicalKey, State,
        session::{Acceptance, Completion},
    };
    use std::collections::BTreeMap;

    fn descriptor() -> Descriptor {
        Descriptor {
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
        }
    }
    fn caps() -> ArchiveCapabilities {
        ArchiveCapabilities {
            backend_id: "memory".into(),
            format_id: "native-v1".into(),
            max_bytes: 4,
        }
    }
    fn archive(bytes: &[u8]) -> NativeArchive {
        NativeArchive {
            backend_id: "memory".into(),
            format_id: "native-v1".into(),
            bytes: bytes.to_vec(),
        }
    }
    fn session() -> Session {
        Session::new(descriptor())
            .unwrap()
            .with_archive(caps())
            .unwrap()
    }
    fn review(before: &[u8], target: &[u8]) -> Review {
        Review {
            before: archive(before),
            target: archive(target),
            changes: vec![crate::archive::SectionChange {
                id: "keys".into(),
                label: "Keys".into(),
                count: Some(1),
            }],
        }
    }
    #[test]
    fn rejects_invalid_target_and_invalid_capture_without_losing_operation_guard() {
        let mut session = session();
        session.connect().unwrap();
        assert!(session.request_archive_review(archive(&[])).is_err());
        assert!(
            session
                .request_archive_review(archive(&[1, 2, 3, 4, 5]))
                .is_err()
        );
        let mut wrong = archive(&[1]);
        wrong.format_id = "other".into();
        assert!(session.request_archive_review(wrong).is_err());
        assert!(!session.busy());
        let command = session.request_archive_capture().unwrap();
        assert_eq!(
            serde_json::from_slice::<Command>(&serde_json::to_vec(&command).unwrap()).unwrap(),
            command
        );
        let Command::CaptureArchive {
            generation,
            operation,
        } = command
        else {
            unreachable!()
        };
        assert_eq!(
            session.accept(Completion::CaptureArchive {
                generation,
                operation,
                result: Ok(archive(&[0; 5]))
            }),
            Acceptance::Accepted
        );
        assert!(matches!(
            session.archive(),
            Some(ArchiveState::Unverified {
                problem: ArchiveProblem::InvalidResult(_)
            })
        ));
        let Command::CaptureArchive {
            generation,
            operation,
        } = session.request_archive_capture().unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion::CaptureArchive {
            generation,
            operation,
            result: Ok(archive(&[1, 2])),
        });
        assert_eq!(
            session.archive(),
            Some(&ArchiveState::Captured(archive(&[1, 2])))
        );
    }
    #[test]
    fn review_binds_target_and_ignores_stale_completions() {
        let mut session = session();
        session.connect().unwrap();
        let command = session.request_archive_review(archive(&[9, 8])).unwrap();
        assert_eq!(
            serde_json::from_slice::<Command>(&serde_json::to_vec(&command).unwrap()).unwrap(),
            command
        );
        let Command::ReviewArchive {
            generation,
            operation,
            ..
        } = command
        else {
            unreachable!()
        };
        let stale = Completion::ReviewArchive {
            generation: generation + 1,
            operation,
            result: Ok(review(&[1], &[9, 8])),
        };
        assert_eq!(session.accept(stale), Acceptance::IgnoredStale);
        assert!(session.busy());
        let wrong = Completion::ReviewArchive {
            generation,
            operation,
            result: Ok(review(&[1], &[7])),
        };
        assert_eq!(session.accept(wrong), Acceptance::Accepted);
        assert!(matches!(
            session.archive(),
            Some(ArchiveState::Unverified {
                problem: ArchiveProblem::InvalidResult(_)
            })
        ));
        let Command::ReviewArchive {
            generation,
            operation,
            ..
        } = session.request_archive_review(archive(&[9, 8])).unwrap()
        else {
            unreachable!()
        };
        let valid = Completion::ReviewArchive {
            generation,
            operation,
            result: Ok(review(&[1], &[9, 8])),
        };
        assert_eq!(
            serde_json::from_slice::<Completion>(&serde_json::to_vec(&valid).unwrap()).unwrap(),
            valid
        );
        session.accept(valid);
        assert_eq!(
            session.archive(),
            Some(&ArchiveState::Ready(review(&[1], &[9, 8])))
        );
        session.disconnect();
        assert!(matches!(
            session.archive(),
            Some(ArchiveState::Unverified {
                problem: ArchiveProblem::ReadRequired
            })
        ));
    }
    #[test]
    fn another_device_write_invalidates_review_but_a_read_does_not() {
        let mut session = session();
        session.connect().unwrap();
        let Command::ReviewArchive {
            generation,
            operation,
            ..
        } = session.request_archive_review(archive(&[2])).unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion::ReviewArchive {
            generation,
            operation,
            result: Ok(review(&[1], &[2])),
        });
        let Command::Read {
            generation,
            operation,
        } = session.request_read().unwrap()
        else {
            unreachable!()
        };
        let state = State {
            revision: vec![1],
            bindings: BTreeMap::from([(
                "base".into(),
                BTreeMap::from([("a".into(), Action::Disabled)]),
            )]),
        };
        session.accept(Completion::Read {
            generation,
            operation,
            result: Ok(state),
        });
        assert!(matches!(session.archive(), Some(ArchiveState::Ready(_))));
        session
            .stage(Change {
                layer: "base".into(),
                key: "a".into(),
                action: Action::Key(4),
            })
            .unwrap();
        session.request_apply().unwrap();
        assert!(matches!(
            session.archive(),
            Some(ArchiveState::Unverified {
                problem: ArchiveProblem::ReadRequired
            })
        ));
    }
}
