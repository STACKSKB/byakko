//! Reusable native renderers for choice and bounded numeric controls.
use crate::panels::{self, UiStyle};
use iced::{
    Element, Fill,
    widget::{button, column, container, row, slider, text},
};
use std::ops::RangeInclusive;

/// Shared visible RGB presets for lighting and individual keys.
pub fn color_presets<Message: Clone + 'static>(
    style: &UiStyle,
    selected: [u8; 3],
    on_change: Option<impl Fn([u8; 3]) -> Message>,
) -> Element<'static, Message> {
    let presets = [
        ("Red", [255, 0, 0]),
        ("Orange", [255, 128, 0]),
        ("Yellow", [255, 255, 0]),
        ("Green", [0, 255, 0]),
        ("Blue", [0, 0, 255]),
        ("Violet", [128, 0, 255]),
        ("White", [255, 255, 255]),
        ("Off", [0, 0, 0]),
    ];
    row(presets.into_iter().map(|(label, rgb)| {
        let foreground = if u32::from(rgb[0]) * 299
            + u32::from(rgb[1]) * 587
            + u32::from(rgb[2]) * 114
            > 128_000
        {
            iced::Color::BLACK
        } else {
            iced::Color::WHITE
        };
        let swatch = button(iced::widget::space())
            .width(style.color_hue_width)
            .height(style.color_hue_width)
            .padding(0)
            .on_press_maybe(on_change.as_ref().map(|on_change| on_change(rgb)))
            .style(move |theme, status| {
                let mut appearance = button::secondary(theme, status);
                appearance.background = Some(iced::Background::Color(iced::Color::from_rgb8(
                    rgb[0], rgb[1], rgb[2],
                )));
                appearance.text_color = foreground;
                appearance.border.width = if selected == rgb { 3.0 } else { 1.0 };
                appearance.border.color = foreground;
                appearance
            });
        iced::widget::tooltip(swatch, text(label), iced::widget::tooltip::Position::Top).into()
    }))
    .spacing(style.spacing.xs)
    .wrap()
    .into()
}

#[derive(Clone)]
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
                .width(style.fields.regular)
                .into()
        }
        _ => label.into(),
    }
}
