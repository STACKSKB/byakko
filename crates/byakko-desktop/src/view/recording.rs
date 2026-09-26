//! Local recording controls; no device state or clocks live in the view.
use crate::{
    form::recording::{Message, Options},
    widget::panels::UiStyle,
};
use iced::{
    Element, Fill,
    widget::{button, checkbox, column, row, scrollable, text, text_input},
};

pub enum Phase {
    Idle { editable: bool },
    Waiting,
    Recording { events: usize },
}

pub fn preview<'a>(
    program: &'a byakko_core::model::macros::Program,
    descriptor: &'a byakko_core::model::keymap::Descriptor,
    caps: &'a byakko_core::model::macros::Capabilities,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    scrollable(
        column(program.events.iter().enumerate().map(|(index, event)| {
            text(format!(
                "{}: {} · {} ms",
                index + 1,
                super::macros::action_label(&event.action, descriptor, caps),
                event.delay_ms
            ))
            .into()
        }))
        .spacing(style.spacing.s),
    )
    .height(Fill)
    .into()
}

pub fn controls<'a>(
    options: &'a Options,
    phase: Phase,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    match phase {
        Phase::Waiting => row![
            text("Finishing the current library read before recording…"),
            button("Cancel recording").on_press(Message::Stop),
        ]
        .spacing(style.spacing.m)
        .into(),
        Phase::Recording { events } => column![
            row![
                button("Stop recording").on_press(Message::Stop),
                text(format!("Recording · {events} events")),
            ]
            .spacing(style.spacing.m),
            text("Type or click in a blank area. Stop or leave this window to finish."),
            text("Recorded events append to this macro. Review them before saving."),
        ]
        .spacing(style.spacing.s)
        .into(),
        Phase::Idle { editable } => row![
            button("Record").on_press_maybe(editable.then_some(Message::Start)),
            checkbox(options.fixed)
                .label("Fixed wait")
                .on_toggle_maybe(editable.then_some(Message::Fixed)),
            text_input("Wait (ms)", &options.delay)
                .width(style.fields.compact)
                .on_input_maybe((editable && options.fixed).then_some(Message::Delay)),
        ]
        .spacing(style.spacing.m)
        .into(),
    }
}
