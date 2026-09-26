//! Capability-driven onboard lighting controls over the sole core draft.
use crate::{
    form::lighting::{Form, Message},
    widget::panels::{self, UiStyle},
};
use byakko_core::{
    editor::{Editor, Status, lighting::LightingRules},
    model::lighting::Content,
    projection::lighting,
};
use iced::{
    Element,
    widget::{button, column, row, text},
};

pub fn view<'a>(
    _form: &'a Form,
    editor: &'a Editor<LightingRules>,
    editable: bool,
    idle: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let editable = editable
        && editor.status() == &Status::Ready
        && (editor.draft().is_some()
            || matches!(
                editor.baseline().map(|snapshot| &snapshot.content),
                Some(Content::HostActive { .. })
            ));
    let controls = lighting::effect_choices(
        editor.capabilities(),
        editor.draft().map(|s| s.effect.as_str()),
    );
    let effects = row(controls.into_iter().map(|choice| {
        panels::selectable_button(
            style,
            choice.label,
            choice.selected,
            editable.then_some(Message::Edit(choice.edit)),
        )
    }))
    .spacing(style.spacing.s)
    .wrap();
    let mut content = column![
        row![
            button("Read").on_press_maybe(idle.then_some(Message::Read)),
            button("Revert").on_press_maybe((idle && editor.dirty()).then_some(Message::Revert)),
            button("Apply now")
                .on_press_maybe((idle && editor.dirty() && editable).then_some(Message::Save)),
        ]
        .spacing(style.spacing.s),
        text("Changes apply automatically."),
        effects,
    ]
    .spacing(style.spacing.m);
    if let Some(setting) = editor.draft() {
        match lighting::controls(editor.capabilities(), setting) {
            Ok(controls) => {
                content = content.push(crate::widget::lighting::parameters(
                    setting,
                    controls.settings,
                    editable,
                    style,
                    format!("lighting:{}", setting.effect),
                    Message::Edit,
                    Message::Picker,
                ));
            }
            Err(reason) => content = content.push(text(reason)),
        }
    } else {
        let explanation = match editor.baseline().map(|s| &s.content) {
            Some(Content::Opaque { reason }) => reason.as_str(),
            Some(Content::HostActive { .. }) => {
                "A host mode is active. Select an onboard effect to replace it."
            }
            _ => "Read lighting to load its controls.",
        };
        content = content.push(text(explanation));
    }
    content.into()
}
