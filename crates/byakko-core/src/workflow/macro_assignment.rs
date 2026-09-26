//! Save and assign is two ordered feature writes with an explicit partial result.
use crate::{
    editor::{Editor, Status, keymap::KeymapRules, macros::MacroRules},
    model::keymap::{Change, Descriptor},
    session::Outcome,
    workflow::Problem,
};
pub(crate) enum Assignment {
    SavingMacro { change: Change },
    Assigning { macro_saved: bool },
}
pub(crate) enum Step {
    Assign(Change),
    Finished(Outcome),
}
impl Assignment {
    pub fn advance(self, outcome: Outcome) -> Step {
        match (self, outcome) {
            (Self::SavingMacro { change }, Outcome::MacroSaved) => Step::Assign(change),
            (Self::Assigning { macro_saved }, Outcome::Saved) => {
                Step::Finished(Outcome::AssignmentSucceeded { macro_saved })
            }
            (Self::SavingMacro { .. }, Outcome::Failed(problem)) => {
                Step::Finished(Outcome::AssignmentFailed {
                    macro_saved: false,
                    problem: Problem::Device(problem),
                })
            }
            (Self::Assigning { macro_saved }, Outcome::Failed(problem)) => {
                Step::Finished(Outcome::AssignmentFailed {
                    macro_saved,
                    problem: Problem::Device(problem),
                })
            }
            (_, outcome) => Step::Finished(outcome),
        }
    }
}
pub(crate) fn assign(
    keymap: &mut Editor<KeymapRules>,
    change: Change,
) -> Result<Option<crate::contract::FeatureCommand<crate::model::keymap::State, Vec<Change>>>, String>
{
    keymap.bind_macro(change)?;
    if !keymap.dirty() {
        return Ok(None);
    }
    let (expected, desired) = keymap.request_apply()?;
    Ok(Some(crate::contract::FeatureCommand::Apply {
        expected,
        desired,
    }))
}
pub(crate) struct Plan {
    pub change: Change,
    pub save_macro: bool,
    pub already_assigned: bool,
}
pub(crate) fn plan(
    descriptor: &Descriptor,
    keymap: &Editor<KeymapRules>,
    editor: &Editor<MacroRules>,
    layer: &str,
    key: &str,
    binding: &str,
) -> Result<Plan, String> {
    if keymap.status() != &Status::Ready {
        return Err("Read and verify the keymap before assignment".into());
    }
    if keymap.dirty() {
        return Err("Save or revert unrelated keymap edits before macro assignment".into());
    }
    let change = Change {
        layer: layer.into(),
        key: key.into(),
        action: editor.planned_binding_action(binding)?,
    };
    crate::validation::keymap::validate_changes(descriptor, std::slice::from_ref(&change))?;
    let already_assigned = keymap
        .baseline()
        .and_then(|baseline| baseline.bindings.get(layer))
        .and_then(|values| values.get(key))
        == Some(&change.action);
    Ok(Plan {
        change,
        save_macro: editor.dirty(),
        already_assigned,
    })
}
