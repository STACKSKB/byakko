//! Deterministic lifecycle for one temporary host-driven lighting activity.
use super::{
    Acceptance, Activity, ApplyFailure, HostPhase, HostStart, HostTicket, Problem, Recovery,
    Session, Status,
};
use crate::lighting::{self, Content, Snapshot, editor};

impl Session {
    pub fn request_host_start(&mut self, mode_id: &str) -> Result<HostStart, String> {
        self.require_idle()?;
        if self.status != Status::Ready {
            return Err("Read and verify the connected device before streaming".into());
        }
        let editor = self
            .lighting
            .as_ref()
            .ok_or("Device has no lighting capability")?;
        if editor.status() != &editor::Status::Ready || editor.dirty() {
            return Err("Read verified lighting and save or revert staged edits first".into());
        }
        let expected = editor.baseline().ok_or("No lighting baseline")?.clone();
        if !matches!(&expected.content, Content::Editable(_)) {
            return Err("Host lighting requires an editable baseline".into());
        }
        let mode = editor
            .capabilities()
            .host_modes
            .iter()
            .find(|mode| mode.id == mode_id)
            .ok_or("Host lighting mode is unavailable")?
            .clone();
        let ticket = HostTicket {
            generation: self.generation,
            operation: self.operation()?,
        };
        self.activity = Activity::HostLighting {
            ticket,
            phase: HostPhase::Starting,
            expected: expected.clone(),
        };
        Ok(HostStart {
            ticket,
            mode,
            expected,
        })
    }

    pub fn request_host_stop(&mut self) -> Result<HostTicket, String> {
        match &mut self.activity {
            Activity::HostLighting { ticket, phase, .. } => {
                *phase = HostPhase::Stopping;
                Ok(*ticket)
            }
            _ => Err("No host lighting activity is active".into()),
        }
    }

    pub fn accept_host_started(&mut self, ticket: HostTicket) -> Acceptance {
        match &mut self.activity {
            Activity::HostLighting {
                ticket: pending,
                phase,
                ..
            } if *pending == ticket && ticket.generation == self.generation => {
                if *phase == HostPhase::Starting {
                    *phase = HostPhase::Streaming;
                }
                Acceptance::Accepted
            }
            _ => Acceptance::IgnoredStale,
        }
    }

    pub fn accept_host_start_failed(
        &mut self,
        ticket: HostTicket,
        failure: ApplyFailure,
    ) -> Acceptance {
        if !self.host_ticket_matches(ticket) {
            return Acceptance::IgnoredStale;
        }
        self.activity = Activity::Idle;
        self.accept_host_failure(failure);
        Acceptance::Accepted
    }

    pub fn accept_host_finished(
        &mut self,
        ticket: HostTicket,
        result: Result<Snapshot, ApplyFailure>,
    ) -> Acceptance {
        if !self.host_ticket_matches(ticket) {
            return Acceptance::IgnoredStale;
        }
        let Activity::HostLighting { expected, .. } =
            std::mem::replace(&mut self.activity, Activity::Idle)
        else {
            unreachable!()
        };
        match result {
            Ok(restored)
                if restored == expected
                    && lighting::validate_snapshot(
                        self.lighting
                            .as_ref()
                            .expect("active lighting capability")
                            .capabilities(),
                        &restored,
                    )
                    .is_ok() =>
            {
                self.lighting
                    .as_mut()
                    .expect("active lighting capability")
                    .accept_read(Ok(restored));
                self.status = Status::Ready;
            }
            Ok(_) => self.accept_host_failure(ApplyFailure {
                message: "Host lighting restoration did not match the saved baseline".into(),
                recovery: Recovery::Unverified,
            }),
            Err(failure) => self.accept_host_failure(failure),
        }
        Acceptance::Accepted
    }

    fn host_ticket_matches(&self, ticket: HostTicket) -> bool {
        ticket.generation == self.generation
            && matches!(&self.activity, Activity::HostLighting { ticket: pending, .. } if *pending == ticket)
    }

    fn accept_host_failure(&mut self, failure: ApplyFailure) {
        if matches!(
            failure.recovery,
            Recovery::NotAttempted | Recovery::Verified
        ) {
            self.status = Status::Ready;
        } else {
            self.status = Status::Unverified {
                problem: Problem::Apply(failure),
            };
            self.invalidate_lighting();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Command, Completion};
    use super::*;
    use crate::{
        Action, Descriptor, Layer, PhysicalKey, State,
        lighting::{Capabilities, Effect, HostMode, HostSource, Setting},
    };
    use std::collections::BTreeMap;

    fn snapshot() -> Snapshot {
        Snapshot {
            backend_id: "memory".into(),
            revision: vec![1],
            content: Content::Editable(Setting {
                effect: "steady".into(),
                brightness: None,
                speed: None,
                option: None,
                color: None,
            }),
        }
    }

    fn loaded() -> Session {
        let mut session = Session::new(Descriptor {
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
        .with_lighting(Capabilities {
            backend_id: "memory".into(),
            effects: vec![Effect {
                id: "steady".into(),
                label: "Steady".into(),
                brightness: None,
                speed: None,
                options: vec![],
                color: None,
            }],
            host_modes: vec![HostMode {
                id: "screen".into(),
                label: "Screen".into(),
                source: HostSource::ScreenAverage,
            }],
        })
        .unwrap();
        session.connect().unwrap();
        let Command::Read {
            generation,
            operation,
        } = session.request_read().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(
            session.accept(Completion::Read {
                generation,
                operation,
                result: Ok(State {
                    revision: vec![1],
                    bindings: BTreeMap::from([(
                        "base".into(),
                        BTreeMap::from([("a".into(), Action::Disabled)])
                    )]),
                }),
            }),
            Acceptance::Accepted
        );
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
                result: Ok(snapshot()),
            }),
            Acceptance::Accepted
        );
        session
    }

    #[test]
    fn stop_during_start_and_stale_events_preserve_owned_activity() {
        let mut session = loaded();
        let start = session.request_host_start("screen").unwrap();
        assert_eq!(start.expected, snapshot());
        assert!(session.busy());
        assert!(session.request_lighting_read().is_err());
        let wrong = HostTicket {
            operation: start.ticket.operation + 1,
            ..start.ticket
        };
        assert_eq!(session.accept_host_started(wrong), Acceptance::IgnoredStale);
        assert_eq!(session.request_host_stop().unwrap(), start.ticket);
        assert_eq!(
            session.accept_host_started(start.ticket),
            Acceptance::Accepted
        );
        assert!(matches!(
            session.activity(),
            Activity::HostLighting {
                phase: HostPhase::Stopping,
                ..
            }
        ));
        assert_eq!(
            session.accept_host_finished(wrong, Ok(snapshot())),
            Acceptance::IgnoredStale
        );
        assert_eq!(
            session.accept_host_finished(start.ticket, Ok(snapshot())),
            Acceptance::Accepted
        );
        assert!(!session.busy());
        assert_eq!(session.status(), &Status::Ready);
    }

    #[test]
    fn uncertain_restore_invalidates_lighting_until_read() {
        let mut session = loaded();
        let ticket = session.request_host_start("screen").unwrap().ticket;
        assert_eq!(session.accept_host_started(ticket), Acceptance::Accepted);
        assert_eq!(
            session.accept_host_finished(
                ticket,
                Err(ApplyFailure {
                    message: "restore failed".into(),
                    recovery: Recovery::Failed,
                })
            ),
            Acceptance::Accepted
        );
        assert!(matches!(session.status(), Status::Unverified { .. }));
        assert_ne!(session.lighting().unwrap().status(), &editor::Status::Ready);
        assert!(session.request_host_start("screen").is_err());
    }
}
