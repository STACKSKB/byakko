//! Quiet reads of loaded features share the ordinary editor acceptance rules.
use byakko_core::{
    contract::{Command, Completion, Feature, Problem},
    editor::{Editor, Feature as EditorFeature, Status},
    session::Session,
};
use std::collections::VecDeque;

#[derive(Default)]
pub struct Observation {
    remaining: VecDeque<Feature>,
    pending: Option<(u64, u64)>,
}

impl Observation {
    pub fn start(&mut self, preferred: Feature, session: &Session) {
        self.remaining = [
            preferred.clone(),
            Feature::Keymap,
            Feature::Settings,
            Feature::Lighting,
        ]
        .into_iter()
        .enumerate()
        .filter_map(|(index, feature)| (index == 0 || feature != preferred).then_some(feature))
        .collect();
        if let Some(editor) = session.macros() {
            self.remaining.push_back(Feature::Macro {
                slot: editor.slot().into(),
            });
        }
        // Selector-dependent colors follow lighting, never activate a mode.
        self.remaining.push_back(Feature::Picture);
    }

    pub fn matches(&self, completion: &Completion) -> bool {
        self.pending == Some((completion.generation, completion.operation))
    }

    pub fn next(&mut self, session: &mut Session) -> Result<Option<Command>, String> {
        self.pending = None;
        while let Some(feature) = self.remaining.pop_front() {
            let command = match feature {
                Feature::Keymap if readable(session.keymap()) => session.read(),
                Feature::Settings if session.settings().is_some_and(readable) => {
                    session.read_settings()
                }
                Feature::Lighting if session.lighting().is_some_and(readable) => {
                    session.read_lighting()
                }
                Feature::Macro { slot }
                    if session
                        .macros()
                        .is_some_and(|editor| editor.slot() == slot && readable(editor)) =>
                {
                    session.read_macro()
                }
                Feature::Picture
                    if session.picture().is_some_and(readable) && picture_active(session) =>
                {
                    session.read_picture()
                }
                _ => continue,
            }?;
            self.pending = Some((command.generation, command.operation));
            return Ok(Some(command));
        }
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
