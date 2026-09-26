//! Enter picture editing through its advertised display effect and current selector.
use crate::{
    contract::Problem,
    editor::{Editor, Status, lighting::LightingRules, picture::PictureRules},
    model::lighting::{self, Setting},
    session::Outcome,
    workflow::Problem as WorkflowProblem,
};

pub(crate) enum Action {
    Ready,
    ReadLighting,
    SaveLighting(Setting),
    ReadPicture,
}
pub(crate) enum Preparation {
    ReadingLighting,
    SavingLighting,
    ReadingPicture { lighting_applied: bool },
}
pub(crate) enum Step {
    Next {
        action: Action,
        lighting_applied: bool,
    },
    Finished(Outcome),
}
pub(crate) fn plan(
    lighting: &Editor<LightingRules>,
    picture: &Editor<PictureRules>,
) -> Result<Action, String> {
    if lighting.dirty() {
        return Err("Save or revert lighting edits before preparing the picture".into());
    }
    if !matches!(
        picture.status(),
        Status::Ready
            | Status::Unloaded
            | Status::Unverified {
                problem: Problem::ReadRequired
            }
    ) {
        return Err("Read and resolve the picture problem before preparing it".into());
    }
    match lighting.status() {
        Status::Unloaded
        | Status::Unverified {
            problem: Problem::ReadRequired,
        } => {
            if picture.dirty() {
                return Err("Save or revert picture edits before preparing it".into());
            }
            return Ok(Action::ReadLighting);
        }
        Status::Ready => {}
        _ => {
            return Err(
                "Read and resolve the lighting problem before preparing the picture".into(),
            );
        }
    }
    if let Some(effect) = &picture.capabilities().lighting_effect {
        let current = lighting.draft().ok_or("Lighting is not editable")?;
        if current.effect != *effect {
            if picture.dirty() {
                return Err(
                    "Save or revert picture edits before switching its display effect".into(),
                );
            }
            return Ok(Action::SaveLighting(crate::editor::lighting::edit_setting(
                lighting.capabilities(),
                current,
                lighting::Edit::Effect(effect.clone()),
            )?));
        }
    }
    match picture.status() {
        Status::Ready => Ok(Action::Ready),
        Status::Unloaded
        | Status::Unverified {
            problem: Problem::ReadRequired,
        } if !picture.dirty() => Ok(Action::ReadPicture),
        _ => Err("Read and resolve the picture problem before preparing it".into()),
    }
}
impl Preparation {
    pub fn advance(
        self,
        outcome: Outcome,
        lighting: &Editor<LightingRules>,
        picture: &Editor<PictureRules>,
    ) -> Step {
        match (self, outcome) {
            (Self::ReadingLighting, Outcome::LightingLoaded) => match plan(lighting, picture) {
                Ok(action) => Step::Next {
                    action,
                    lighting_applied: false,
                },
                Err(reason) => Step::Finished(failed(false, WorkflowProblem::Validation(reason))),
            },
            (Self::SavingLighting, Outcome::LightingSaved) => Step::Next {
                action: Action::ReadPicture,
                lighting_applied: true,
            },
            (Self::ReadingPicture { .. }, Outcome::PictureLoaded) => {
                Step::Finished(Outcome::PictureLoaded)
            }
            (state, Outcome::Failed(problem)) => Step::Finished(failed(
                state.lighting_applied(),
                WorkflowProblem::Device(problem),
            )),
            (state, _) => Step::Finished(failed(
                state.lighting_applied(),
                WorkflowProblem::Validation(
                    "Picture preparation encountered a conflicting observation".into(),
                ),
            )),
        }
    }
    fn lighting_applied(&self) -> bool {
        matches!(
            self,
            Self::ReadingPicture {
                lighting_applied: true
            }
        )
    }
}
pub(crate) fn failed(lighting_applied: bool, problem: WorkflowProblem) -> Outcome {
    Outcome::PicturePreparationFailed {
        lighting_applied,
        problem,
    }
}
