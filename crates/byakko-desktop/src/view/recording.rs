//! Local recording controls; no device state or clocks live in the view.
use crate::widget::controls::text_input;
use crate::widget::focus_button::button;
use crate::{
    form::recording::{Message, Options},
    widget::{focus_control, panels::UiStyle},
};
use iced::{
    Element,
    widget::{checkbox, column, row, text},
};

pub enum Phase {
    Idle { editable: bool, empty: bool },
    Waiting,
    Recording { events: usize },
}

pub fn preview<'a>(
    program: &'a byakko_core::model::macros::Program,
    descriptor: &'a byakko_core::model::keymap::Descriptor,
    caps: &'a byakko_core::model::macros::Capabilities,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    column(program.events.iter().enumerate().map(|(index, event)| {
        text(format!(
            "{}: {} · {} ms",
            index + 1,
            super::macros::action_label(&event.action, descriptor, caps),
            event.delay_ms
        ))
        .into()
    }))
    .spacing(style.spacing.s)
    .into()
}

pub fn controls<'a>(
    options: &'a Options,
    phase: Phase,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    match phase {
        Phase::Waiting => row![text("Preparing…"), button("Cancel").on_press(Message::Stop),]
            .spacing(style.spacing.m)
            .wrap()
            .into(),
        Phase::Recording { events } => row![
            button("Stop recording").on_press(Message::Stop),
            text(format!("{events} events")),
        ]
        .spacing(style.spacing.s)
        .wrap()
        .into(),
        Phase::Idle { editable, empty } => {
            let record = button("Record").on_press_maybe(editable.then_some(Message::Start));
            let record = if empty {
                record
            } else {
                record.style(iced::widget::button::secondary)
            };
            row![
                record,
                focus_control::toggle(
                    checkbox(options.fixed)
                        .label("Fixed timing")
                        .style(|theme: &iced::Theme, status| {
                            let mut style = checkbox::primary(theme, status);
                            let palette = theme.extended_palette();
                            style.border.color = palette.background.base.text;
                            style.border.width = 1.0;
                            style.text_color = Some(palette.background.base.text);
                            style
                        })
                        .on_toggle_maybe(editable.then_some(Message::Fixed)),
                    Message::Fixed(!options.fixed),
                    editable,
                ),
                text_input("Wait (ms)", &options.delay)
                    .width(style.fields.compact)
                    .on_input_maybe((editable && options.fixed).then_some(Message::Delay)),
            ]
            .spacing(style.spacing.m)
            .wrap()
            .into()
        }
    }
}
