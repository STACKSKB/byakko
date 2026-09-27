//! Quiet reads of loaded features share the ordinary editor acceptance rules.
use byakko_core::{
    contract::{Command, Completion, DeviceChange, Feature, Problem},
    editor::{Editor, Feature as EditorFeature, Status},
    session::Session,
};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Observation {
    remaining: VecDeque<Feature>,
    pending: Option<(u64, u64)>,
    due: Option<Instant>,
    picture_required: bool,
    pub unavailable: bool,
}

impl Observation {
    pub fn receive(&mut self, change: DeviceChange, now: Instant, session: &Session) {
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
        let delay = if change == DeviceChange::Configuration {
            Duration::from_secs(2)
        } else {
            Duration::from_millis(500)
        };
        self.due = Some(self.due.map_or(now + delay, |due| due.max(now + delay)));
    }

    pub fn queued(&self) -> bool {
        !self.remaining.is_empty()
    }

    pub fn ready(&mut self, now: Instant) -> bool {
        if self.due.is_some_and(|due| now < due) {
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
