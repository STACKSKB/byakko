//! Compact native color picker beside the effect's advertised controls.
use super::{
    color_picker,
    panels::{self, UiStyle},
};
use byakko_core::{
    model::lighting::{Color, Edit, Setting},
    projection::lighting::{Control, LevelEdit},
};
use iced::{
    Alignment, Element,
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
    let mut settings = column![].spacing(style.spacing.s);
    for control in controls {
        let control: Element<'a, M> = match control {
            Control::Level {
                edit: LevelEdit::Channel(_),
                ..
            } => continue,
            Control::Choices { label, choices } => row![
                text(label).width(style.fields.compact),
                row(choices.into_iter().map(|choice| panels::selectable_button(
                    style,
                    choice.label,
                    choice.selected,
                    editable.then(|| edit_message(choice.edit))
                )))
                .spacing(style.spacing.xs)
                .width(style.fields.regular)
                .wrap()
            ]
            .spacing(style.spacing.s)
            .align_y(Alignment::Center)
            .into(),
            Control::Level {
                label,
                range,
                value,
                edit,
            } => {
                let mut controls = row![
                    text(label).width(style.fields.compact),
                    text(value.to_string()).width(style.color_hue_width)
                ]
                .spacing(style.spacing.s)
                .align_y(Alignment::Center);
                if editable && range.start() != range.end() {
                    controls = controls.push(
                        slider(range, value, move |value| {
                            edit_message(edit.edit(value).expect("projected lighting range"))
                        })
                        .step(1u16)
                        .width(style.fields.regular),
                    );
                }
                controls.into()
            }
        };
        settings = settings.push(control);
    }
    let picker = match setting.color {
        Some(Color::Rgb(rgb)) => color_picker::view(
            style,
            rgb,
            picker_id,
            picker_message,
            editable.then_some(move |rgb| edit_message(Edit::Color(Color::Rgb(rgb)))),
        ),
        _ => column![].into(),
    };
    row![picker, settings].spacing(style.spacing.m).into()
}
