//! Quiet reads of loaded features share the ordinary editor acceptance rules.
use crate::{
    contract::{Command, Completion, DeviceChange, Feature, Problem},
    editor::{Editor, Feature as EditorFeature, Status},
    session::{Connection, Session},
};
use std::collections::VecDeque;

#[derive(Default)]
pub struct Observation {
    remaining: VecDeque<Feature>,
    pending: Option<(u64, u64)>,
    due: Option<u64>,
    picture_required: bool,
    pub unavailable: bool,
}

impl Observation {
    /// `now_ms` is elapsed milliseconds from the caller's monotonic clock.
    pub fn receive(&mut self, change: DeviceChange, now_ms: u64, session: &Session) {
        let mut features = match change {
            DeviceChange::Lighting => vec![Feature::Lighting, Feature::Picture],
            DeviceChange::Settings => vec![Feature::Settings],
            DeviceChange::Configuration => {
                self.picture_required = true;
                let mut features = vec![Feature::Keymap, Feature::Lighting, Feature::Settings];
                if let Some(editor) = session.macros() {
                    features.push(Feature::Macro {
                        slot: editor.slot().into(),
                    });
                }
                features.push(Feature::Picture);
                features
            }
        };
        // A new lighting observation must precede any queued picture refresh.
        if features.contains(&Feature::Lighting) {
            self.remaining
                .retain(|feature| feature != &Feature::Picture);
        }
        for feature in features.drain(..) {
            if !self.remaining.contains(&feature) {
                self.remaining.push_back(feature);
            }
        }
        let delay_ms = if change == DeviceChange::Configuration {
            2_000
        } else {
            500
        };
        let candidate = now_ms.saturating_add(delay_ms);
        self.due = Some(self.due.map_or(candidate, |due| due.max(candidate)));
    }

    pub fn queued(&self) -> bool {
        !self.remaining.is_empty()
    }

    /// Absolute elapsed-millisecond deadline for a caller-owned timer.
    pub fn due_ms(&self) -> Option<u64> {
        self.due
    }

    pub fn ready(&mut self, now_ms: u64) -> bool {
        if self.due.is_some_and(|due| now_ms < due) {
            return false;
        }
        self.due = None;
        true
    }

    pub fn matches(&self, completion: &Completion) -> bool {
        self.pending == Some((completion.generation, completion.operation))
    }

    pub fn next(&mut self, session: &mut Session) -> Result<Option<Command>, String> {
        self.pending = None;
        while let Some(feature) = self.remaining.pop_front() {
            let command = match feature {
                Feature::Keymap if readable(session.keymap()) => session.observe(Feature::Keymap),
                Feature::Settings if session.settings().is_some_and(readable) => {
                    session.observe(Feature::Settings)
                }
                Feature::Lighting if session.lighting().is_some_and(readable) => {
                    session.observe(Feature::Lighting)
                }
                Feature::Macro { slot }
                    if session
                        .macros()
                        .is_some_and(|editor| editor.slot() == slot && readable(editor)) =>
                {
                    session.observe(Feature::Macro { slot })
                }
                Feature::Picture
                    if session.picture().is_some_and(|editor| {
                        readable(editor)
                            && (self.picture_required || editor.status() != &Status::Ready)
                    }) && picture_active(session) =>
                {
                    self.picture_required = false;
                    session.observe(Feature::Picture)
                }
                _ => continue,
            }?;
            self.pending = Some((command.generation, command.operation));
            return Ok(Some(command));
        }
        self.due = None;
        self.picture_required = false;
        Ok(None)
    }
}

/// Session-only observation gate shared by native and browser frontends.
/// A frontend also blocks reads for its own activities such as drag, file I/O
/// or a locally active write deadline.
pub fn can_observe(session: &Session) -> bool {
    matches!(session.connection(), Connection::Connected { .. })
        && !session.busy()
        && !session.catalog_scanning()
        && !session.recording()
        && session.host().is_idle()
        && !session.requires_manual_read()
        && !has_read_error(session)
}

fn readable<F: EditorFeature>(editor: &Editor<F>) -> bool {
    editor.baseline().is_some()
        && matches!(
            editor.status(),
            Status::Ready
                | Status::Unverified {
                    problem: Problem::ReadRequired
                }
        )
}

fn picture_active(session: &Session) -> bool {
    session.picture().is_some_and(|picture| {
        picture
            .capabilities()
            .lighting_effect
            .as_ref()
            .is_none_or(|effect| {
                session.lighting().is_some_and(|lighting| {
                    lighting.status() == &Status::Ready
                        && !lighting.dirty()
                        && lighting
                            .draft()
                            .is_some_and(|setting| setting.effect == *effect)
                })
            })
    })
}

pub fn has_read_error(session: &Session) -> bool {
    fn failed<F: EditorFeature>(editor: &Editor<F>) -> bool {
        matches!(
            editor.status(),
            Status::Unverified {
                problem: Problem::Read(_)
            }
        )
    }
    failed(session.keymap())
        || session.lighting().is_some_and(failed)
        || session.settings().is_some_and(failed)
        || session.picture().is_some_and(failed)
        || session.macros().is_some_and(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        contract::{CommandPayload, CompletionPayload, FeatureCommand, FeatureResult},
        model::{
            keymap::{
                Action, ActionCategory, ActionChoice, Change, Descriptor, Layer, PhysicalKey, State,
            },
            lighting::{self, Capabilities, Content, Effect, Setting, Snapshot},
            settings,
        },
        session::Outcome,
    };

    fn session() -> Session {
        let descriptor = Descriptor {
            backend_id: "test".into(),
            device_name: "Test".into(),
            layers: vec![Layer {
                id: "base".into(),
                label: "Base".into(),
                read_only_keys: vec![],
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
            actions: [4, 5, 6]
                .into_iter()
                .map(|usage| ActionChoice {
                    label: format!("Key {usage}"),
                    action: Action::Key(usage),
                    category: ActionCategory::Alphanumeric,
                })
                .collect(),
            shortcuts: None,
        };
        let mut session = Session::new(descriptor)
            .unwrap()
            .with_lighting(Capabilities {
                backend_id: "test".into(),
                effects: vec![Effect {
                    id: "solid".into(),
                    label: "Solid".into(),
                    brightness: None,
                    speed: None,
                    options: vec![],
                    color: None,
                }],
                host_modes: vec![],
            })
            .unwrap()
            .with_settings(settings::Capabilities {
                backend_id: "test".into(),
                fields: vec![settings::Field {
                    id: "win-lock".into(),
                    label: "Windows lock".into(),
                    kind: settings::Kind::Toggle,
                }],
            })
            .unwrap();
        session.connect().unwrap();
        let keymap = session.read().unwrap();
        assert_eq!(
            session.accept(
                keymap
                    .clone()
                    .map(|_| CompletionPayload::Keymap(FeatureResult::Read(Ok(state(4)))))
            ),
            Outcome::Loaded
        );
        let lighting = session.read_lighting().unwrap();
        assert_eq!(
            session.accept(lighting.clone().map(|_| CompletionPayload::Lighting(
                FeatureResult::Read(Ok(lighting_snapshot()))
            ))),
            Outcome::LightingLoaded
        );
        let settings = session.read_settings().unwrap();
        assert_eq!(
            session.accept(
                settings.map(|_| CompletionPayload::Settings(FeatureResult::Read(Ok(
                    settings_snapshot()
                ))))
            ),
            Outcome::SettingsLoaded
        );
        session
    }

    fn state(usage: u16) -> State {
        State {
            revision: vec![usage as u8],
            bindings: [("base".into(), [("a".into(), Action::Key(usage))].into())].into(),
        }
    }

    fn lighting_snapshot() -> Snapshot {
        Snapshot {
            backend_id: "test".into(),
            revision: vec![1],
            picture_context: vec![],
            evidence: lighting::Evidence::Readback,
            content: Content::Editable(Setting {
                effect: "solid".into(),
                brightness: None,
                speed: None,
                option: None,
                color: None,
            }),
        }
    }

    fn settings_snapshot() -> settings::Snapshot {
        settings::Snapshot {
            backend_id: "test".into(),
            revision: vec![1],
            content: settings::Content::Editable(
                [("win-lock".into(), settings::Value::Toggle(false))].into(),
            ),
        }
    }

    #[test]
    fn coalescing_and_configuration_settle_use_elapsed_milliseconds() {
        let session = session();
        let mut observation = Observation::default();
        observation.receive(DeviceChange::Lighting, 100, &session);
        observation.receive(DeviceChange::Settings, 400, &session);
        assert_eq!(observation.due_ms(), Some(900));
        assert!(!observation.ready(899));
        assert!(observation.ready(900));
        observation.receive(DeviceChange::Configuration, 1_000, &session);
        observation.receive(DeviceChange::Lighting, 1_200, &session);
        assert_eq!(observation.due_ms(), Some(3_000));
        assert!(!observation.ready(2_999));
        assert!(observation.ready(3_000));
    }

    #[test]
    fn lighting_event_reads_only_loaded_lighting_and_configuration_reads_keymap_first() {
        let mut session = session();
        let mut observation = Observation::default();
        observation.receive(DeviceChange::Lighting, 0, &session);
        assert!(observation.ready(500));
        let command = observation.next(&mut session).unwrap().unwrap();
        assert!(matches!(
            command.payload,
            CommandPayload::Lighting(FeatureCommand::Read(()))
        ));
        assert!(
            observation.matches(&command.clone().map(|_| CompletionPayload::Lighting(
                FeatureResult::Read(Ok(lighting_snapshot()))
            )))
        );
        session
            .accept(command.map(|_| {
                CompletionPayload::Lighting(FeatureResult::Read(Ok(lighting_snapshot())))
            }));
        assert!(observation.next(&mut session).unwrap().is_none());

        observation.receive(DeviceChange::Configuration, 1_000, &session);
        assert!(observation.ready(3_000));
        let keymap = observation.next(&mut session).unwrap().unwrap();
        assert!(matches!(
            keymap.payload,
            CommandPayload::Keymap(FeatureCommand::Read(()))
        ));
        session
            .accept(keymap.map(|_| CompletionPayload::Keymap(FeatureResult::Read(Ok(state(4))))));
        let lighting = observation.next(&mut session).unwrap().unwrap();
        assert!(matches!(
            lighting.payload,
            CommandPayload::Lighting(FeatureCommand::Read(()))
        ));
        session
            .accept(lighting.map(|_| {
                CompletionPayload::Lighting(FeatureResult::Read(Ok(lighting_snapshot())))
            }));
        let settings = observation.next(&mut session).unwrap().unwrap();
        assert!(matches!(
            settings.payload,
            CommandPayload::Settings(FeatureCommand::Read(()))
        ));
        session
            .accept(settings.map(|_| {
                CompletionPayload::Settings(FeatureResult::Read(Ok(settings_snapshot())))
            }));
        assert!(observation.next(&mut session).unwrap().is_none());
    }

    #[test]
    fn settings_event_reads_only_loaded_settings_including_windows_lock() {
        let mut session = session();
        let mut observation = Observation::default();
        observation.receive(DeviceChange::Settings, 50, &session);
        assert!(!observation.ready(549));
        assert!(observation.ready(550));
        let command = observation.next(&mut session).unwrap().unwrap();
        assert!(matches!(
            command.payload,
            CommandPayload::Settings(FeatureCommand::Read(()))
        ));
        session
            .accept(command.map(|_| {
                CompletionPayload::Settings(FeatureResult::Read(Ok(settings_snapshot())))
            }));
        assert!(observation.next(&mut session).unwrap().is_none());
    }

    #[test]
    fn dirty_observation_keeps_draft_and_records_device_conflict() {
        let mut session = session();
        assert!(can_observe(&session));
        session
            .edit(Change {
                layer: "base".into(),
                key: "a".into(),
                action: Action::Key(5),
            })
            .unwrap();
        let mut observation = Observation::default();
        observation.receive(DeviceChange::Configuration, 0, &session);
        assert!(observation.ready(2_000));
        let command = observation.next(&mut session).unwrap().unwrap();
        assert_eq!(
            session.accept(
                command.map(|_| CompletionPayload::Keymap(FeatureResult::Read(Ok(state(6)))))
            ),
            Outcome::Conflict
        );
        assert_eq!(
            session.keymap().draft().unwrap()["base"]["a"],
            Action::Key(5)
        );
        assert!(matches!(session.keymap().status(), Status::Conflict { .. }));
        assert!(!can_observe(&session));
    }
}
