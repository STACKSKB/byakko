//! Reusable native renderers for choice and bounded numeric controls.
use crate::panels::{self, UiStyle};
use iced::{
    Element, Fill,
    widget::{column, container, slider, text},
};
use std::ops::RangeInclusive;

pub struct Choice<Message> {
    pub label: String,
    pub selected: bool,
    pub message: Option<Message>,
}

pub fn choices<Message: Clone + 'static>(
    style: &UiStyle,
    title: impl Into<String>,
    entries: impl IntoIterator<Item = Choice<Message>>,
) -> Element<'static, Message> {
    let options = column(entries.into_iter().map(|choice| {
        container(panels::selectable_button(
            style,
            choice.label,
            choice.selected,
            choice.message,
        ))
        .width(Fill)
        .into()
    }))
    .spacing(style.spacing.xs);
    column![
        text(title.into()).size(style.type_scale.section_title),
        options
    ]
    .spacing(style.spacing.s)
    .into()
}

pub fn level<Message: Clone + 'static>(
    style: &UiStyle,
    title: impl Into<String>,
    range: RangeInclusive<u16>,
    value: u16,
    on_change: Option<impl Fn(u16) -> Message + 'static>,
) -> Element<'static, Message> {
    let label = text(format!(
        "{} · {value} ({}–{})",
        title.into(),
        range.start(),
        range.end()
    ));
    match on_change {
        Some(on_change) if range.start() != range.end() => {
            column![label, slider(range, value, on_change).step(1u16),]
                .spacing(style.spacing.xs)
                .into()
        }
        _ => label.into(),
    }
}
