//! Capability-driven onboard lighting controls over the sole core draft.
use crate::{
    form::lighting::{Form, Message},
    widget::{
        color_picker,
        panels::{self, UiStyle},
    },
};
use byakko_core::{
    editor::{Editor, Status, lighting::LightingRules},
    model::lighting::{Color, Content, Edit},
    projection::lighting::{self, Control},
};
use iced::{
    Element, Fill,
    widget::{button, column, row, scrollable, slider, text},
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
                for control in controls.settings {
                    let control: Element<'a, Message> = match control {
                        Control::Choices { label, choices } => column![
                            text(label),
                            row(choices.into_iter().map(|choice| panels::selectable_button(
                                style,
                                choice.label,
                                choice.selected,
                                editable.then_some(Message::Edit(choice.edit)),
                            )))
                            .spacing(style.spacing.s)
                            .wrap(),
                        ]
                        .spacing(style.spacing.s)
                        .into(),
                        Control::Level {
                            label,
                            range,
                            value,
                            edit,
                        } => {
                            let control: Element<'a, Message> = if editable {
                                slider(range, value, move |value| {
                                    Message::Edit(
                                        edit.edit(value).expect("projected lighting range"),
                                    )
                                })
                                .into()
                            } else {
                                text(value.to_string()).into()
                            };
                            column![text(format!("{label}: {value}")), control]
                                .spacing(style.spacing.s)
                                .into()
                        }
                    };
                    content = content.push(control);
                }
                if let Some(Color::Rgb(rgb)) = setting.color {
                    content = content.push(color_picker::view(
                        style,
                        rgb,
                        format!("lighting:{}", setting.effect),
                        Message::Picker,
                        editable.then_some(|rgb| Message::Edit(Edit::Color(Color::Rgb(rgb)))),
                    ));
                }
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
    scrollable(content).height(Fill).into()
}
