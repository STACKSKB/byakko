//! Reusable native renderers for choice and bounded numeric controls.
use crate::panels::UiStyle;
use iced::{
    Element,
    widget::{button, row},
};

/// Shared visible RGB presets for lighting and individual keys.
pub fn color_presets<Message: Clone + 'static>(
    style: &UiStyle,
    selected: [u8; 3],
    on_change: Option<impl Fn([u8; 3]) -> Message>,
) -> Element<'static, Message> {
    let presets = [
        [255, 0, 0],
        [255, 128, 0],
        [255, 255, 0],
        [0, 255, 0],
        [0, 0, 255],
        [128, 0, 255],
        [255, 255, 255],
        [0, 0, 0],
    ];
    row(presets.into_iter().map(|rgb| {
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
        swatch.into()
    }))
    .spacing(style.spacing.xs)
    .wrap()
    .into()
}
