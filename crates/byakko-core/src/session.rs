//! Deterministic keymap session decisions. An outer executor performs commands.

use crate::{Action, Change, Descriptor, State, validate_changes, validate_state};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Bindings = BTreeMap<String, BTreeMap<String, Action>>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Command {
    Read {
        generation: u64,
        operation: u64,
    },
    Apply {
        generation: u64,
        operation: u64,
        expected: State,
        changes: Vec<Change>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Completion {
    Read {
        generation: u64,
        operation: u64,
        result: Result<State, String>,
    },
    Apply {
        generation: u64,
        operation: u64,
        result: Result<State, ApplyFailure>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Recovery {
    Verified,
    Failed,
    NotAttempted,
    /// The executor could not establish whether recovery ran or succeeded.
    Unverified,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplyFailure {
    pub message: String,
    pub recovery: Recovery,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Problem {
    ReadRequired,
    Read(String),
    Apply(ApplyFailure),
    InvalidApplyResult(String),
    ApplyReadbackMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Status {
    Disconnected,
    Loading { operation: u64 },
    Ready,
    Applying { operation: u64 },
    Conflict { device: State },
    Unverified { problem: Problem },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Acceptance {
    Accepted,
    IgnoredStale,
}

pub struct KeymapSession {
    descriptor: Descriptor,
    baseline: Option<State>,
    draft: Option<Bindings>,
    status: Status,
    generation: u64,
    next_operation: u64,
}

impl KeymapSession {
    pub fn new(descriptor: Descriptor) -> Result<Self, String> {
        let bindings = descriptor
            .layers
            .iter()
            .map(|layer| {
                (
                    layer.id.clone(),
                    descriptor
                        .keys
                        .iter()
                        .map(|key| (key.id.clone(), Action::Disabled))
                        .collect(),
                )
            })
            .collect();
        validate_state(
            &descriptor,
            &State {
                revision: Vec::new(),
                bindings,
            },
        )?;
        Ok(Self {
            descriptor,
            baseline: None,
            draft: None,
            status: Status::Disconnected,
            generation: 0,
            next_operation: 0,
        })
    }

    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
    pub fn baseline(&self) -> Option<&State> {
        self.baseline.as_ref()
    }
    pub fn draft(&self) -> Option<&Bindings> {
        self.draft.as_ref()
    }
    pub fn status(&self) -> &Status {
        &self.status
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn connect(&mut self) -> Result<u64, String> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("Connection generation exhausted")?;
        self.status = Status::Unverified {
            problem: Problem::ReadRequired,
        };
        Ok(self.generation)
    }

    pub fn disconnect(&mut self) {
        self.status = Status::Disconnected;
    }

    pub fn changes(&self) -> Vec<Change> {
        let (Some(baseline), Some(draft)) = (&self.baseline, &self.draft) else {
            return Vec::new();
        };
        draft
            .iter()
            .flat_map(|(layer, bindings)| {
                bindings
                    .iter()
                    .filter(|(key, action)| {
                        baseline.bindings.get(layer).and_then(|b| b.get(*key)) != Some(*action)
                    })
                    .map(|(key, action)| Change {
                        layer: layer.clone(),
                        key: key.clone(),
                        action: action.clone(),
                    })
            })
            .collect()
    }

    pub fn stage(&mut self, change: Change) -> Result<(), String> {
        if self.status != Status::Ready {
            return Err("Read and verify the connected device before editing".into());
        }
        validate_changes(&self.descriptor, std::slice::from_ref(&change))?;
        let draft = self.draft.as_mut().ok_or("No keymap draft")?;
        *draft
            .get_mut(&change.layer)
            .and_then(|layer| layer.get_mut(&change.key))
            .ok_or("Draft is missing a binding")? = change.action;
        Ok(())
    }

    pub fn revert(&mut self) -> Result<(), String> {
        if matches!(
            self.status,
            Status::Loading { .. } | Status::Applying { .. }
        ) {
            return Err("Wait for the keymap operation before reverting".into());
        }
        let baseline = self.baseline.as_ref().ok_or("No keymap baseline")?;
        self.draft = Some(baseline.bindings.clone());
        Ok(())
    }

    fn operation(&mut self) -> Result<u64, String> {
        self.next_operation = self
            .next_operation
            .checked_add(1)
            .ok_or("Operation ID exhausted")?;
        Ok(self.next_operation)
    }

    pub fn request_read(&mut self) -> Result<Command, String> {
        if matches!(
            self.status,
            Status::Disconnected | Status::Loading { .. } | Status::Applying { .. }
        ) {
            return Err("No connected idle device is available for reading".into());
        }
        let operation = self.operation()?;
        self.status = Status::Loading { operation };
        Ok(Command::Read {
            generation: self.generation,
            operation,
        })
    }

    pub fn request_apply(&mut self) -> Result<Command, String> {
        if self.status != Status::Ready {
            return Err("Read and verify the device before applying".into());
        }
        let expected = self.baseline.as_ref().ok_or("No keymap baseline")?.clone();
        let changes = self.changes();
        if changes.is_empty() {
            return Err("No keymap changes are staged".into());
        }
        validate_changes(&self.descriptor, &changes)?;
        let operation = self.operation()?;
        self.status = Status::Applying { operation };
        Ok(Command::Apply {
            generation: self.generation,
            operation,
            expected,
            changes,
        })
    }

    pub fn accept(&mut self, completion: Completion) -> Acceptance {
        let pending = match &completion {
            Completion::Read {
                generation,
                operation,
                ..
            } => {
                *generation == self.generation
                    && self.status
                        == Status::Loading {
                            operation: *operation,
                        }
            }
            Completion::Apply {
                generation,
                operation,
                ..
            } => {
                *generation == self.generation
                    && self.status
                        == Status::Applying {
                            operation: *operation,
                        }
            }
        };
        if !pending {
            return Acceptance::IgnoredStale;
        }
        match completion {
            Completion::Read { result, .. } => self.accept_read(result),
            Completion::Apply { result, .. } => self.accept_apply(result),
        }
        Acceptance::Accepted
    }

    fn accept_read(&mut self, result: Result<State, String>) {
        let state = match result.and_then(|state| {
            validate_state(&self.descriptor, &state)?;
            Ok(state)
        }) {
            Ok(state) => state,
            Err(reason) => {
                self.status = Status::Unverified {
                    problem: Problem::Read(reason),
                };
                return;
            }
        };
        let dirty = !self.changes().is_empty();
        if dirty && self.baseline.as_ref() != Some(&state) {
            self.status = Status::Conflict { device: state };
            return;
        }
        if !dirty {
            self.draft = Some(state.bindings.clone());
        }
        self.baseline = Some(state);
        self.status = Status::Ready;
    }

    fn accept_apply(&mut self, result: Result<State, ApplyFailure>) {
        let state = match result {
            Ok(state) => state,
            Err(failure) => {
                self.status = Status::Unverified {
                    problem: Problem::Apply(failure),
                };
                return;
            }
        };
        if let Err(reason) = validate_state(&self.descriptor, &state) {
            self.status = Status::Unverified {
                problem: Problem::InvalidApplyResult(reason),
            };
            return;
        }
        if self.draft.as_ref() != Some(&state.bindings) {
            self.status = Status::Unverified {
                problem: Problem::ApplyReadbackMismatch,
            };
            return;
        }
        self.baseline = Some(state);
        self.status = Status::Ready;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Layer, PhysicalKey};

    fn descriptor() -> Descriptor {
        Descriptor {
            backend_id: "memory".into(),
            device_name: "Test keyboard".into(),
            keys: ["a", "b"]
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
                    writable: index == 0,
                })
                .collect(),
            layers: vec![Layer {
                id: "base".into(),
                label: "Base".into(),
            }],
            actions: Vec::new(),
        }
    }

    fn state(revision: u8, usage: u16) -> State {
        State {
            revision: vec![revision],
            bindings: BTreeMap::from([(
                "base".into(),
                BTreeMap::from([
                    ("a".into(), Action::Key(usage)),
                    ("b".into(), Action::Disabled),
                ]),
            )]),
        }
    }

    fn edit(usage: u16) -> Change {
        Change {
            layer: "base".into(),
            key: "a".into(),
            action: Action::Key(usage),
        }
    }

    fn read(session: &mut KeymapSession, result: Result<State, String>) {
        let Command::Read {
            generation,
            operation,
        } = session.request_read().unwrap()
        else {
            panic!("expected read command")
        };
        assert_eq!(
            session.accept(Completion::Read {
                generation,
                operation,
                result
            }),
            Acceptance::Accepted
        );
    }

    #[test]
    fn rejects_old_connection_and_out_of_order_completions() {
        let mut session = KeymapSession::new(descriptor()).unwrap();
        let first_generation = session.connect().unwrap();
        let Command::Read {
            generation,
            operation,
        } = session.request_read().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(generation, first_generation);
        session.disconnect();
        session.connect().unwrap();
        let Command::Read {
            generation: next_generation,
            operation: next_operation,
        } = session.request_read().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(
            session.accept(Completion::Read {
                generation,
                operation,
                result: Ok(state(1, 4))
            }),
            Acceptance::IgnoredStale
        );
        assert_eq!(
            session.accept(Completion::Read {
                generation: next_generation,
                operation: next_operation + 1,
                result: Ok(state(2, 4))
            }),
            Acceptance::IgnoredStale
        );
        assert_eq!(
            session.accept(Completion::Apply {
                generation: next_generation,
                operation: next_operation,
                result: Ok(state(2, 4))
            }),
            Acceptance::IgnoredStale
        );
        assert_eq!(
            session.status(),
            &Status::Loading {
                operation: next_operation
            }
        );
        assert!(session.baseline().is_none());
        assert_eq!(
            session.accept(Completion::Read {
                generation: next_generation,
                operation: next_operation,
                result: Ok(state(2, 4))
            }),
            Acceptance::Accepted
        );
        assert_eq!(session.status(), &Status::Ready);
    }

    #[test]
    fn dirty_reconnect_matching_read_retains_draft_and_difference_conflicts() {
        let mut session = KeymapSession::new(descriptor()).unwrap();
        session.connect().unwrap();
        read(&mut session, Ok(state(1, 4)));
        assert!(
            session
                .stage(Change {
                    key: "b".into(),
                    ..edit(5)
                })
                .is_err()
        );
        session.stage(edit(5)).unwrap();
        session.disconnect();
        assert!(session.stage(edit(6)).is_err());
        session.connect().unwrap();
        read(&mut session, Ok(state(1, 4)));
        assert_eq!(session.status(), &Status::Ready);
        assert_eq!(session.changes(), vec![edit(5)]);
        session.disconnect();
        session.connect().unwrap();
        read(&mut session, Ok(state(2, 6)));
        assert_eq!(
            session.status(),
            &Status::Conflict {
                device: state(2, 6)
            }
        );
        assert_eq!(session.baseline(), Some(&state(1, 4)));
        assert_eq!(session.draft().unwrap()["base"]["a"], Action::Key(5));
        assert!(session.request_apply().is_err());
    }

    #[test]
    fn failed_and_mismatched_apply_keep_baseline_and_draft_unverified() {
        let mut session = KeymapSession::new(descriptor()).unwrap();
        session.connect().unwrap();
        read(&mut session, Ok(state(1, 4)));
        session.stage(edit(5)).unwrap();
        let Command::Apply {
            generation,
            operation,
            expected,
            changes,
        } = session.request_apply().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(expected, state(1, 4));
        assert_eq!(changes, vec![edit(5)]);
        let failure = ApplyFailure {
            message: "write failed".into(),
            recovery: Recovery::Verified,
        };
        assert_eq!(
            session.accept(Completion::Apply {
                generation,
                operation,
                result: Err(failure.clone())
            }),
            Acceptance::Accepted
        );
        assert_eq!(
            session.status(),
            &Status::Unverified {
                problem: Problem::Apply(failure)
            }
        );
        assert_eq!(session.baseline(), Some(&state(1, 4)));
        assert_eq!(session.changes(), vec![edit(5)]);
        assert!(session.request_apply().is_err());
        let mut malformed = state(1, 4);
        malformed.bindings.get_mut("base").unwrap().remove("b");
        read(&mut session, Ok(malformed));
        assert!(matches!(
            session.status(),
            Status::Unverified {
                problem: Problem::Read(_)
            }
        ));
        assert_eq!(session.changes(), vec![edit(5)]);
        read(&mut session, Ok(state(1, 4)));
        let Command::Apply {
            generation,
            operation,
            ..
        } = session.request_apply().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(
            session.accept(Completion::Apply {
                generation,
                operation,
                result: Ok(state(2, 6))
            }),
            Acceptance::Accepted
        );
        assert_eq!(
            session.status(),
            &Status::Unverified {
                problem: Problem::ApplyReadbackMismatch
            }
        );
        assert_eq!(session.changes(), vec![edit(5)]);
    }

    #[test]
    fn failed_read_preserves_draft_and_matching_apply_becomes_ready() {
        let mut session = KeymapSession::new(descriptor()).unwrap();
        session.connect().unwrap();
        read(&mut session, Ok(state(1, 4)));
        session.stage(edit(5)).unwrap();
        read(&mut session, Err("transport unavailable".into()));
        assert_eq!(
            session.status(),
            &Status::Unverified {
                problem: Problem::Read("transport unavailable".into())
            }
        );
        assert_eq!(session.baseline(), Some(&state(1, 4)));
        assert_eq!(session.changes(), vec![edit(5)]);
        assert!(session.request_apply().is_err());
        read(&mut session, Ok(state(1, 4)));
        let Command::Apply {
            generation,
            operation,
            ..
        } = session.request_apply().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(
            session.accept(Completion::Apply {
                generation,
                operation,
                result: Ok(state(2, 5))
            }),
            Acceptance::Accepted
        );
        assert_eq!(session.status(), &Status::Ready);
        assert_eq!(session.baseline(), Some(&state(2, 5)));
        assert!(session.changes().is_empty());
    }

    #[test]
    fn commands_and_results_round_trip_as_owned_json_values() {
        let mut session = KeymapSession::new(descriptor()).unwrap();
        session.connect().unwrap();
        let command = session.request_read().unwrap();
        let encoded = serde_json::to_vec(&command).unwrap();
        assert_eq!(
            serde_json::from_slice::<Command>(&encoded).unwrap(),
            command
        );
        let Command::Read {
            generation,
            operation,
        } = command
        else {
            unreachable!()
        };
        let completion = Completion::Read {
            generation,
            operation,
            result: Ok(state(1, 4)),
        };
        let encoded = serde_json::to_vec(&completion).unwrap();
        let decoded = serde_json::from_slice::<Completion>(&encoded).unwrap();
        assert_eq!(decoded, completion);
        assert_eq!(session.accept(decoded), Acceptance::Accepted);
        session.stage(edit(5)).unwrap();
        let apply = session.request_apply().unwrap();
        let encoded = serde_json::to_vec(&apply).unwrap();
        assert_eq!(serde_json::from_slice::<Command>(&encoded).unwrap(), apply);
        let Command::Apply {
            generation,
            operation,
            ..
        } = apply
        else {
            unreachable!()
        };
        let failure = Completion::Apply {
            generation,
            operation,
            result: Err(ApplyFailure {
                message: "write failed".into(),
                recovery: Recovery::Failed,
            }),
        };
        let encoded = serde_json::to_vec(&failure).unwrap();
        assert_eq!(
            serde_json::from_slice::<Completion>(&encoded).unwrap(),
            failure
        );
    }

    #[test]
    fn opaque_action_survives_apply_command_and_verified_completion() {
        let mut session = KeymapSession::new(descriptor()).unwrap();
        session.connect().unwrap();
        read(&mut session, Ok(state(1, 4)));
        let opaque = Action::Opaque {
            backend_id: "memory".into(),
            data: vec![0, 255, 7, 42],
            label: "Unknown action".into(),
        };
        session
            .stage(Change {
                action: opaque.clone(),
                ..edit(4)
            })
            .unwrap();
        let command = session.request_apply().unwrap();
        let encoded = serde_json::to_vec(&command).unwrap();
        assert_eq!(
            serde_json::from_slice::<Command>(&encoded).unwrap(),
            command
        );
        let Command::Apply {
            generation,
            operation,
            expected,
            changes,
        } = command
        else {
            unreachable!()
        };
        assert_eq!(expected.revision, [1]);
        assert_eq!(changes[0].action, opaque);
        let mut applied = state(2, 4);
        applied
            .bindings
            .get_mut("base")
            .unwrap()
            .insert("a".into(), opaque);
        let completion = Completion::Apply {
            generation,
            operation,
            result: Ok(applied.clone()),
        };
        let encoded = serde_json::to_vec(&completion).unwrap();
        assert_eq!(
            serde_json::from_slice::<Completion>(&encoded).unwrap(),
            completion
        );
        assert_eq!(session.accept(completion), Acceptance::Accepted);
        assert_eq!(session.status(), &Status::Ready);
        assert_eq!(session.baseline(), Some(&applied));
    }
}
