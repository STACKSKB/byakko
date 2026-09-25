//! Binding choices are advertised actions, not UI-generated firmware codes.
use super::{Desktop, Message, macro_editor::Message as Macro, panels};
use byakko_core::{
    Action,
    macros::{Binding, editor::Editor},
    session::Status,
};
use iced::{
    Element,
    widget::{button, column, row, text, text_input},
};

pub(super) fn view<'a>(app: &'a Desktop, editor: &'a Editor) -> Element<'a, Message> {
    let choices: Vec<_> = editor
        .capabilities()
        .bindings
        .iter()
        .filter(|binding| binding.slot == editor.slot())
        .collect();
    if choices.is_empty() {
        return text("This slot has no key-binding choices").into();
    }
    let descriptor = app.session.descriptor();
    let key = descriptor
        .keys
        .iter()
        .find(|key| Some(&key.id) == app.selected.as_ref());
    let editable = !app.busy()
        && *editor.status() == byakko_core::macros::editor::Status::Ready
        && editor.draft().is_some();
    let selected_choice = selected_binding(app, editor);
    let selected = selected_choice.map(|choice| choice.id.as_str());
    let modes = row(choices.iter().map(|choice| {
        panels::selectable_button(
            &app.ui,
            &choice.label,
            selected == Some(choice.id.as_str()),
            (!app.busy()).then(|| Message::Macro(Macro::ChooseBinding(choice.id.clone()))),
        )
    }))
    .spacing(app.ui.spacing.s)
    .wrap();
    let restriction = selected_choice.and_then(|choice| app.assignment_problem(&choice.id));
    let can_assign = !app.busy() && selected_choice.is_some() && restriction.is_none();
    let target = key.map_or_else(|| "Key: —".to_owned(), |key| format!("Key: {}", key.label));
    let can_save = editable
        && editor.dirty()
        && editor.request_apply().is_ok()
        && app.macro_repeat_input_valid();
    let mut content = column![
        text(target),
        modes,
        row![
            text("Repeat"),
            text_input("Count", &app.repeat_input)
                .width(app.ui.fields.compact)
                .on_input_maybe(
                    editable.then_some(|value| Message::Macro(Macro::RepeatInput(value)))
                ),
        ]
        .spacing(app.ui.spacing.s),
        button(if editor.dirty() {
            "Save & assign"
        } else {
            "Assign macro"
        })
        .on_press_maybe(
            selected_choice
                .filter(|_| can_assign)
                .map(|choice| Message::Macro(Macro::Assign(choice.id.clone())))
        ),
        row![
            button("Save only").on_press_maybe(can_save.then_some(Message::Macro(Macro::Apply))),
            button("Revert edits").on_press_maybe(
                (editable && editor.dirty()).then_some(Message::Macro(Macro::Revert))
            ),
        ]
        .spacing(app.ui.spacing.s),
    ]
    .spacing(app.ui.spacing.s);
    if let Some(reason) = restriction {
        content = content.push(text(reason));
    }
    if *app.session.status() != Status::Ready && *app.session.status() != Status::Disconnected {
        content = content
            .push(button("Reload keyboard").on_press_maybe((!app.busy()).then_some(Message::Read)));
    }
    content = content.push(super::macro_files::file_controls(app, editor));
    panels::panel(&app.ui, "Playback", content.into())
}

/// Prefer the user's explicit mode, then preserve the mode already bound to
/// the selected key, then pick a mode compatible with this macro's repeat
/// count. If none are compatible, keep the first choice selected so its reason
/// is visible beside the disabled Assign button.
pub(super) fn selected_binding<'a>(app: &Desktop, editor: &'a Editor) -> Option<&'a Binding> {
    let choices: Vec<_> = editor
        .capabilities()
        .bindings
        .iter()
        .filter(|binding| binding.slot == editor.slot())
        .collect();
    let explicit = app
        .macro_binding_choice
        .as_ref()
        .filter(|(slot, _)| slot == editor.slot())
        .and_then(|(_, id)| choices.iter().copied().find(|choice| choice.id == *id));
    let bound_action = app
        .selected
        .as_deref()
        .and_then(|key| app.session.draft()?.get(&app.layer)?.get(key));
    choose_binding(
        &choices,
        explicit,
        bound_action,
        editor.draft().map(|program| program.repeat_count),
    )
}

fn choose_binding<'a>(
    choices: &[&'a Binding],
    explicit: Option<&'a Binding>,
    bound_action: Option<&Action>,
    repeat_count: Option<u32>,
) -> Option<&'a Binding> {
    explicit
        .or_else(|| {
            bound_action.and_then(|action| choices.iter().copied().find(|c| &c.action == action))
        })
        .or_else(|| {
            choices.iter().copied().find(|choice| {
                choice
                    .required_repeat_count
                    .is_none_or(|required| Some(required) == repeat_count)
            })
        })
        .or_else(|| choices.first().copied())
}

#[cfg(test)]
mod tests {
    use super::choose_binding;
    use byakko_core::{Action, macros::Binding};

    fn binding(id: &str, required_repeat_count: Option<u32>) -> Binding {
        Binding {
            slot: "slot-00".into(),
            id: id.into(),
            label: id.into(),
            action: Action::Macro {
                slot: 0,
                mode: if id == "counted" { 0 } else { 1 },
            },
            required_repeat_count,
        }
    }

    #[test]
    fn explicit_choice_takes_precedence() {
        let counted = binding("counted", None);
        let hold = binding("hold", Some(1));
        let choices = [&counted, &hold];
        let selected = choose_binding(&choices, Some(&hold), Some(&counted.action), Some(1));
        assert_eq!(selected.map(|choice| choice.id.as_str()), Some("hold"));
    }

    #[test]
    fn preserves_the_selected_keys_advertised_binding() {
        let counted = binding("counted", None);
        let hold = binding("hold", Some(1));
        let choices = [&counted, &hold];
        let selected = choose_binding(&choices, None, Some(&hold.action), Some(2));
        assert_eq!(selected.map(|choice| choice.id.as_str()), Some("hold"));
    }

    #[test]
    fn defaults_to_first_mode_compatible_with_repeat_count() {
        let hold = binding("hold", Some(1));
        let counted = binding("counted", None);
        let choices = [&hold, &counted];
        let selected = choose_binding(&choices, None, None, Some(2));
        assert_eq!(selected.map(|choice| choice.id.as_str()), Some("counted"));
    }

    #[test]
    fn falls_back_to_first_choice_to_expose_its_restriction() {
        let hold = binding("hold", Some(1));
        let toggle = binding("toggle", Some(1));
        let choices = [&hold, &toggle];
        let selected = choose_binding(&choices, None, None, Some(2));
        assert_eq!(selected.map(|choice| choice.id.as_str()), Some("hold"));
    }
}
