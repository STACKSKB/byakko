//! Capture and review opaque native archives without paths or device I/O.
use super::{Activity, Command, Session, Status};
use crate::archive::{
    self, ArchiveCapabilities, ArchiveProblem, ArchiveState, NativeArchive, Review,
};
use crate::session::CommandPayload;
#[cfg(test)]
use crate::session::CompletionPayload;
use crate::session::FeatureCommand;
#[cfg(test)]
use crate::session::FeatureResult;
use crate::session::{DeviceActivity, Feature};

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
        } else if let ArchiveState::Unverified { review, .. } = &mut self.archive_state {
            *review = None;
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
        let command = self.begin_feature(
            Feature::Archive,
            FeatureCommand::Read(()),
            CommandPayload::Archive,
        )?;
        self.archive_state = ArchiveState::Unverified {
            problem: ArchiveProblem::ReadRequired,
            review: None,
        };
        Ok(command)
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
        self.activity = Activity::Device {
            operation,
            request: DeviceActivity::ReviewArchive {
                target: target.clone(),
            },
        };
        self.archive_state = ArchiveState::Unverified {
            problem: ArchiveProblem::ReadRequired,
            review: None,
        };
        Ok(Command {
            generation: self.generation,
            operation,
            payload: CommandPayload::ReviewArchive { target },
        })
    }
    pub fn request_archive_apply(&mut self) -> Result<Command, String> {
        self.require_idle()?;
        if self.status == Status::Disconnected {
            return Err("Device is disconnected".into());
        }
        let ArchiveState::Ready(review) = &self.archive_state else {
            return Err("Review a fresh native archive before applying".into());
        };
        let caps = self
            .archive_capabilities
            .as_ref()
            .expect("ready archive capability");
        archive::validate_review(caps, review)?;
        if review.changes.is_empty() || review.before == review.target {
            return Err("Reviewed archive has no changes to apply".into());
        }
        let expected = review.before.clone();
        let target = review.target.clone();
        let command = self.begin_feature(
            Feature::Archive,
            FeatureCommand::Apply {
                expected,
                desired: target,
            },
            CommandPayload::Archive,
        )?;
        self.status = Status::Unverified {
            problem: super::Problem::ReadRequired,
        };
        self.invalidate_macros();
        self.invalidate_lighting();
        self.invalidate_picture();
        self.invalidate_settings();
        Ok(command)
    }
    pub(super) fn invalidate_archive(&mut self) {
        let previous = std::mem::replace(&mut self.archive_state, ArchiveState::Idle);
        self.archive_state = match previous {
            ArchiveState::Ready(review) => ArchiveState::Unverified {
                problem: ArchiveProblem::ReadRequired,
                review: Some(review),
            },
            other => other,
        };
    }
    pub(super) fn accept_archive_capture(&mut self, result: Result<NativeArchive, String>) {
        let caps = self
            .archive_capabilities
            .as_ref()
            .expect("pending archive capability");
        self.archive_state = match result {
            Err(reason) => ArchiveState::Unverified {
                problem: ArchiveProblem::Capture(reason),
                review: None,
            },
            Ok(captured) => match archive::validate_archive(caps, &captured) {
                Ok(()) => ArchiveState::Captured(captured),
                Err(reason) => ArchiveState::Unverified {
                    problem: ArchiveProblem::InvalidResult(reason),
                    review: None,
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
                review: None,
            },
            Ok(review) => match archive::validate_review(caps, &review) {
                Err(reason) => ArchiveState::Unverified {
                    problem: ArchiveProblem::InvalidResult(reason),
                    review: None,
                },
                Ok(()) if review.target != target => ArchiveState::Unverified {
                    problem: ArchiveProblem::InvalidResult(
                        "Reviewed archive differs from requested target".into(),
                    ),
                    review: None,
                },
                Ok(()) => ArchiveState::Ready(review),
            },
        };
    }
    pub(super) fn accept_archive_apply(
        &mut self,
        result: Result<NativeArchive, super::ApplyFailure>,
    ) {
        let previous = std::mem::replace(&mut self.archive_state, ArchiveState::Idle);
        let ArchiveState::Ready(review) = previous else {
            unreachable!("pending archive review")
        };
        let caps = self
            .archive_capabilities
            .as_ref()
            .expect("pending archive capability");
        self.archive_state = match result {
            Err(failure) => ArchiveState::Unverified {
                problem: ArchiveProblem::Apply(failure),
                review: Some(review),
            },
            Ok(actual) => match archive::validate_archive(caps, &actual) {
                Err(reason) => ArchiveState::Unverified {
                    problem: ArchiveProblem::InvalidResult(reason),
                    review: Some(review),
                },
                Ok(()) if actual != review.target => ArchiveState::Unverified {
                    problem: ArchiveProblem::ApplyReadbackMismatch,
                    review: Some(review),
                },
                Ok(()) => ArchiveState::Captured(actual),
            },
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Action, Change, Descriptor, Layer, PhysicalKey, State,
        session::{Acceptance, ApplyFailure, Completion, Recovery},
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
            shortcuts: None,
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
    fn ready(session: &mut Session) {
        let Command {
            generation,
            operation,
            payload: CommandPayload::ReviewArchive { .. },
        } = session.request_archive_review(archive(&[2])).unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion {
            generation,
            operation,
            payload: CompletionPayload::ReviewArchive {
                result: Ok(review(&[1], &[2])),
            },
        });
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
        let Command {
            generation,
            operation,
            payload: CommandPayload::Archive(FeatureCommand::Read(())),
        } = command
        else {
            unreachable!()
        };
        assert_eq!(
            session.accept(Completion {
                generation,
                operation,
                payload: CompletionPayload::Archive(FeatureResult::Read(Ok(archive(&[0; 5]))))
            }),
            Acceptance::Accepted
        );
        assert!(matches!(
            session.archive(),
            Some(ArchiveState::Unverified {
                problem: ArchiveProblem::InvalidResult(_),
                ..
            })
        ));
        let Command {
            generation,
            operation,
            payload: CommandPayload::Archive(FeatureCommand::Read(())),
        } = session.request_archive_capture().unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion {
            generation,
            operation,
            payload: CompletionPayload::Archive(FeatureResult::Read(Ok(archive(&[1, 2])))),
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
        let Command {
            generation,
            operation,
            payload: CommandPayload::ReviewArchive { .. },
        } = command
        else {
            unreachable!()
        };
        let stale = Completion {
            generation: generation + 1,
            operation,
            payload: CompletionPayload::ReviewArchive {
                result: Ok(review(&[1], &[9, 8])),
            },
        };
        assert_eq!(session.accept(stale), Acceptance::IgnoredStale);
        assert!(session.busy());
        let wrong = Completion {
            generation,
            operation,
            payload: CompletionPayload::ReviewArchive {
                result: Ok(review(&[1], &[7])),
            },
        };
        assert_eq!(session.accept(wrong), Acceptance::Accepted);
        assert!(matches!(
            session.archive(),
            Some(ArchiveState::Unverified {
                problem: ArchiveProblem::InvalidResult(_),
                ..
            })
        ));
        let Command {
            generation,
            operation,
            payload: CommandPayload::ReviewArchive { .. },
        } = session.request_archive_review(archive(&[9, 8])).unwrap()
        else {
            unreachable!()
        };
        let valid = Completion {
            generation,
            operation,
            payload: CompletionPayload::ReviewArchive {
                result: Ok(review(&[1], &[9, 8])),
            },
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
                problem: ArchiveProblem::ReadRequired,
                ..
            })
        ));
    }
    #[test]
    fn another_device_write_invalidates_review_but_a_read_does_not() {
        let mut session = session();
        session.connect().unwrap();
        let Command {
            generation,
            operation,
            payload: CommandPayload::ReviewArchive { .. },
        } = session.request_archive_review(archive(&[2])).unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion {
            generation,
            operation,
            payload: CompletionPayload::ReviewArchive {
                result: Ok(review(&[1], &[2])),
            },
        });
        let Command {
            generation,
            operation,
            payload: CommandPayload::Keymap(FeatureCommand::Read(())),
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
        session.accept(Completion {
            generation,
            operation,
            payload: CompletionPayload::Keymap(FeatureResult::Read(Ok(state))),
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
                problem: ArchiveProblem::ReadRequired,
                ..
            })
        ));
    }
    #[test]
    fn archive_apply_requires_change_and_checks_stale_and_readback() {
        let mut session = session();
        session.connect().unwrap();
        let Command {
            generation,
            operation,
            payload: CommandPayload::ReviewArchive { .. },
        } = session.request_archive_review(archive(&[1])).unwrap()
        else {
            unreachable!()
        };
        let mut unchanged = review(&[1], &[1]);
        unchanged.changes.clear();
        session.accept(Completion {
            generation,
            operation,
            payload: CompletionPayload::ReviewArchive {
                result: Ok(unchanged),
            },
        });
        assert!(session.request_archive_apply().is_err());
        ready(&mut session);
        let command = session.request_archive_apply().unwrap();
        assert_eq!(
            serde_json::from_slice::<Command>(&serde_json::to_vec(&command).unwrap()).unwrap(),
            command
        );
        let Command {
            generation,
            operation,
            payload:
                CommandPayload::Archive(FeatureCommand::Apply {
                    expected,
                    desired: target,
                }),
        } = command
        else {
            unreachable!()
        };
        assert_eq!(expected, archive(&[1]));
        assert_eq!(target, archive(&[2]));
        assert_eq!(
            session.accept(Completion {
                generation,
                operation,
                payload: CompletionPayload::Archive(FeatureResult::Read(Ok(target.clone())))
            }),
            Acceptance::IgnoredStale
        );
        assert_eq!(
            session.accept(Completion {
                generation: generation + 1,
                operation,
                payload: CompletionPayload::Archive(FeatureResult::Apply(Ok(target.clone())))
            }),
            Acceptance::IgnoredStale
        );
        assert!(session.busy());
        let mismatch = Completion {
            generation,
            operation,
            payload: CompletionPayload::Archive(FeatureResult::Apply(Ok(archive(&[3])))),
        };
        assert_eq!(
            serde_json::from_slice::<Completion>(&serde_json::to_vec(&mismatch).unwrap()).unwrap(),
            mismatch
        );
        assert_eq!(session.accept(mismatch), Acceptance::Accepted);
        assert!(matches!(
            session.archive(),
            Some(ArchiveState::Unverified {
                problem: ArchiveProblem::ApplyReadbackMismatch,
                review: Some(_)
            })
        ));
        assert!(session.request_archive_apply().is_err());
        ready(&mut session);
        let Command {
            generation,
            operation,
            payload: CommandPayload::Archive(FeatureCommand::Apply { .. }),
        } = session.request_archive_apply().unwrap()
        else {
            unreachable!()
        };
        session.accept(Completion {
            generation,
            operation,
            payload: CompletionPayload::Archive(FeatureResult::Apply(Ok(archive(&[2])))),
        });
        assert_eq!(
            session.archive(),
            Some(&ArchiveState::Captured(archive(&[2])))
        );
    }
    #[test]
    fn failed_apply_retains_review_but_path_change_clears_intent() {
        let mut session = session();
        session.connect().unwrap();
        ready(&mut session);
        let Command {
            generation,
            operation,
            payload: CommandPayload::Archive(FeatureCommand::Apply { .. }),
        } = session.request_archive_apply().unwrap()
        else {
            unreachable!()
        };
        let failure = ApplyFailure {
            message: "recovery failed".into(),
            recovery: Recovery::Failed,
        };
        session.accept(Completion {
            generation,
            operation,
            payload: CompletionPayload::Archive(FeatureResult::Apply(Err(failure.clone()))),
        });
        assert_eq!(
            session.archive(),
            Some(&ArchiveState::Unverified {
                problem: ArchiveProblem::Apply(failure.clone()),
                review: Some(review(&[1], &[2]))
            })
        );
        assert_eq!(
            session.reconnect_cautions(),
            vec![super::super::ReconnectCaution {
                surface: super::super::ReconnectSurface::Archive,
                cause: super::super::ReconnectCause::Apply(&failure),
            }]
        );
        assert!(session.request_archive_apply().is_err());
        session.clear_archive_review();
        assert_eq!(
            session.archive(),
            Some(&ArchiveState::Unverified {
                problem: ArchiveProblem::Apply(failure),
                review: None
            })
        );
    }
}
