//! Shared controls for onboard effects and host-mode parameters.
use super::{
    color_picker,
    panels::{self, UiStyle},
};
use byakko_core::{
    model::lighting::{Color, Edit, Setting},
    projection::lighting::Control,
};
use iced::{
    Element,
    widget::{column, row, slider, text},
};

pub(crate) fn parameters<'a, M: Clone + 'static>(
    setting: &Setting,
    controls: Vec<Control>,
    editable: bool,
    style: &'a UiStyle,
    picker_id: String,
    edit_message: fn(Edit) -> M,
    picker_message: fn(color_picker::Interaction) -> M,
) -> Element<'a, M> {
    let mut content = column![].spacing(style.spacing.m);
    for control in controls {
        let control: Element<'a, M> = match control {
            Control::Choices { label, choices } => column![
                text(label),
                row(choices.into_iter().map(|choice| panels::selectable_button(
                    style,
                    choice.label,
                    choice.selected,
                    editable.then(|| edit_message(choice.edit)),
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
                let control: Element<'a, M> = if editable {
                    slider(range, value, move |value| {
                        edit_message(edit.edit(value).expect("projected lighting range"))
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
            picker_id,
            picker_message,
            editable.then_some(move |rgb| edit_message(Edit::Color(Color::Rgb(rgb)))),
        ));
    }
    content.into()
}
