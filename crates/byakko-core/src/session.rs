//! Correlates keymap effects without owning device I/O.
use crate::{
    Action, Change, Descriptor, State,
    contract::{
        Command, CommandPayload, Completion, CompletionPayload, FeatureCommand, FeatureResult,
        Problem,
    },
    keymap::{Bindings, Editor},
    validate_state,
};
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Connection {
    Disconnected,
    Connected { generation: u64 },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Ignored,
    Loaded,
    Saved,
    Conflict,
    Failed(Problem),
}
#[derive(Clone, Copy)]
enum Direction {
    Read,
    Save,
}
#[derive(Clone, Copy)]
struct Ticket {
    generation: u64,
    operation: u64,
    direction: Direction,
}
pub struct Session {
    descriptor: Descriptor,
    keymap: Editor,
    connection: Connection,
    generation: u64,
    operation: u64,
    pending: Option<Ticket>,
}
impl Session {
    pub fn new(descriptor: Descriptor) -> Result<Self, String> {
        let bindings: Bindings = descriptor
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
            keymap: Editor::new(),
            connection: Connection::Disconnected,
            generation: 0,
            operation: 0,
            pending: None,
        })
    }
    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
    pub fn keymap(&self) -> &Editor {
        &self.keymap
    }
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn connect(&mut self) -> Result<u64, String> {
        self.idle()?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("Connection generation exhausted")?;
        self.connection = Connection::Connected {
            generation: self.generation,
        };
        self.keymap.invalidate();
        Ok(self.generation)
    }
    pub fn disconnect(&mut self) {
        if self
            .pending
            .is_some_and(|ticket| matches!(ticket.direction, Direction::Save))
        {
            self.keymap.applied(
                &self.descriptor,
                Err(crate::contract::ApplyFailure {
                    message: "Connection lost during save".into(),
                    recovery: crate::contract::Recovery::Unverified,
                }),
            );
        }
        self.connection = Connection::Disconnected;
        self.pending = None;
        self.keymap.invalidate();
    }
    pub fn read(&mut self) -> Result<Command, String> {
        self.begin(Direction::Read, FeatureCommand::Read(()))
    }
    pub fn edit(&mut self, change: Change) -> Result<(), String> {
        self.idle()?;
        self.keymap.edit(&self.descriptor, change)
    }
    pub fn revert(&mut self) -> Result<(), String> {
        self.idle()?;
        self.keymap.revert()
    }
    pub fn save(&mut self) -> Result<Command, String> {
        let (expected, desired) = self.keymap.save(&self.descriptor)?;
        self.begin(Direction::Save, FeatureCommand::Apply { expected, desired })
    }
    fn idle(&self) -> Result<(), String> {
        if self.busy() {
            Err("Wait for the current operation".into())
        } else {
            Ok(())
        }
    }
    fn begin(
        &mut self,
        direction: Direction,
        payload: FeatureCommand<State, Vec<Change>>,
    ) -> Result<Command, String> {
        self.idle()?;
        let Connection::Connected { generation } = self.connection else {
            return Err("Connect before requesting device operations".into());
        };
        self.operation = self
            .operation
            .checked_add(1)
            .ok_or("Operation ID exhausted")?;
        self.pending = Some(Ticket {
            generation,
            operation: self.operation,
            direction,
        });
        Ok(Command {
            generation,
            operation: self.operation,
            payload: CommandPayload::Keymap(payload),
        })
    }
    pub fn accept(&mut self, completion: Completion) -> Outcome {
        let Some(ticket) = self.pending else {
            return Outcome::Ignored;
        };
        if completion.generation != ticket.generation || completion.operation != ticket.operation {
            return Outcome::Ignored;
        }
        let CompletionPayload::Keymap(result) = completion.payload else {
            return Outcome::Ignored;
        };
        let outcome = match (ticket.direction, result) {
            (Direction::Read, FeatureResult::Read(result)) => {
                self.keymap.read(&self.descriptor, result);
                Outcome::Loaded
            }
            (Direction::Save, FeatureResult::Apply(result)) => {
                self.keymap.applied(&self.descriptor, result);
                Outcome::Saved
            }
            _ => return Outcome::Ignored,
        };
        self.pending = None;
        if matches!(self.keymap.status(), crate::keymap::Status::Conflict { .. }) {
            Outcome::Conflict
        } else {
            self.keymap.problem().map_or(outcome, Outcome::Failed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActionCategory, ActionChoice, Layer, PhysicalKey,
        contract::{ApplyFailure, Recovery},
    };
    fn session() -> Session {
        Session::new(Descriptor {
            backend_id: "test".into(),
            device_name: "Test".into(),
            layers: vec![Layer {
                id: "base".into(),
                label: "Base".into(),
            }],
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
            actions: vec![
                ActionChoice {
                    label: "A".into(),
                    action: Action::Key(4),
                    category: ActionCategory::Alphanumeric,
                },
                ActionChoice {
                    label: "B".into(),
                    action: Action::Key(5),
                    category: ActionCategory::Alphanumeric,
                },
            ],
            shortcuts: None,
        })
        .unwrap()
    }
    fn state(action: Action) -> State {
        State {
            revision: vec![1],
            bindings: [("base".into(), [("a".into(), action)].into())].into(),
        }
    }
    fn result(command: &Command, payload: FeatureResult<State>) -> Completion {
        Completion {
            generation: command.generation,
            operation: command.operation,
            payload: CompletionPayload::Keymap(payload),
        }
    }
    fn loaded() -> Session {
        let mut s = session();
        s.connect().unwrap();
        let c = s.read().unwrap();
        assert_eq!(
            s.accept(result(&c, FeatureResult::Read(Ok(state(Action::Key(4)))))),
            Outcome::Loaded
        );
        s
    }
    fn edit(s: &mut Session) {
        s.edit(Change {
            layer: "base".into(),
            key: "a".into(),
            action: Action::Key(5),
        })
        .unwrap();
    }
    #[test]
    fn stale_and_wrong_direction_do_not_consume_ticket() {
        let mut s = session();
        s.connect().unwrap();
        let c = s.read().unwrap();
        let mut stale = result(&c, FeatureResult::Read(Ok(state(Action::Key(4)))));
        stale.generation += 1;
        assert_eq!(s.accept(stale), Outcome::Ignored);
        assert_eq!(
            s.accept(result(&c, FeatureResult::Apply(Ok(state(Action::Key(4)))))),
            Outcome::Ignored
        );
        assert!(s.busy());
        assert_eq!(
            s.accept(result(&c, FeatureResult::Read(Ok(state(Action::Key(4)))))),
            Outcome::Loaded
        );
    }
    #[test]
    fn reconnect_keeps_edits_until_one_read_reestablishes_baseline() {
        let mut s = loaded();
        edit(&mut s);
        s.disconnect();
        assert!(s.keymap().dirty());
        s.connect().unwrap();
        assert!(s.save().is_err());
        let c = s.read().unwrap();
        assert_eq!(
            s.accept(result(&c, FeatureResult::Read(Ok(state(Action::Key(4)))))),
            Outcome::Loaded
        );
        assert!(s.keymap().dirty());
        assert!(s.save().is_ok());
    }
    #[test]
    fn failed_save_and_disconnect_retain_edits() {
        let mut s = loaded();
        edit(&mut s);
        let c = s.save().unwrap();
        let failure = ApplyFailure {
            message: "write failed".into(),
            recovery: Recovery::Failed,
        };
        assert_eq!(
            s.accept(result(&c, FeatureResult::Apply(Err(failure.clone())))),
            Outcome::Failed(Problem::Apply(failure))
        );
        s.disconnect();
        assert!(s.keymap().dirty());
        assert_eq!(s.keymap().changes().len(), 1);
    }
    #[test]
    fn disconnect_during_save_records_uncertainty() {
        let mut s = loaded();
        edit(&mut s);
        let c = s.save().unwrap();
        s.disconnect();
        assert!(matches!(
            s.keymap().status(),
            crate::keymap::Status::Unverified {
                problem: Problem::Apply(ApplyFailure {
                    recovery: Recovery::Unverified,
                    ..
                })
            }
        ));
        assert!(s.keymap().dirty());
        assert_eq!(
            s.accept(result(&c, FeatureResult::Apply(Ok(state(Action::Key(5)))))),
            Outcome::Ignored
        );
    }
    #[test]
    fn invalid_edits_do_not_change_draft() {
        let mut s = loaded();
        let before = s.keymap().draft().cloned();
        for action in [
            Action::Key(99),
            Action::Opaque {
                backend_id: "test".into(),
                data: vec![1],
                label: "raw".into(),
            },
        ] {
            assert!(
                s.edit(Change {
                    layer: "base".into(),
                    key: "a".into(),
                    action
                })
                .is_err()
            );
            assert_eq!(s.keymap().draft(), before.as_ref());
        }
        s.descriptor.keys[0].writable = false;
        assert!(
            s.edit(Change {
                layer: "base".into(),
                key: "a".into(),
                action: Action::Key(5)
            })
            .is_err()
        );
    }
    #[test]
    fn readback_mismatch_and_read_failure_preserve_draft() {
        let mut s = loaded();
        edit(&mut s);
        let c = s.save().unwrap();
        assert_eq!(
            s.accept(result(&c, FeatureResult::Apply(Ok(state(Action::Key(4)))))),
            Outcome::Failed(Problem::ApplyReadbackMismatch)
        );
        assert!(s.keymap().dirty());
        let c = s.read().unwrap();
        assert_eq!(
            s.accept(result(&c, FeatureResult::Read(Err("read failed".into())))),
            Outcome::Failed(Problem::Read("read failed".into()))
        );
        assert!(s.keymap().dirty());
    }
    #[test]
    fn conflict_retains_edits_and_save_accepts_full_readback() {
        let mut s = loaded();
        edit(&mut s);
        let c = s.read().unwrap();
        let mut changed = state(Action::Key(4));
        changed.revision = vec![2];
        assert_eq!(
            s.accept(result(&c, FeatureResult::Read(Ok(changed)))),
            Outcome::Conflict
        );
        assert!(s.keymap().dirty());
        let mut s = loaded();
        edit(&mut s);
        let c = s.save().unwrap();
        assert_eq!(
            s.accept(result(&c, FeatureResult::Apply(Ok(state(Action::Key(5)))))),
            Outcome::Saved
        );
        assert!(!s.keymap().dirty());
    }
}
