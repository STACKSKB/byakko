// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared readable field and button states for the active application theme.
use iced::{
    Background, Color, Theme,
    widget::{button, pick_list as iced_pick_list, text_input as iced_text_input},
};
use std::borrow::Borrow;

pub fn text_input<'a, Message: Clone + 'a>(
    placeholder: &str,
    value: &str,
) -> iced_text_input::TextInput<'a, Message> {
    iced::widget::text_input(placeholder, value).style(input_style)
}

pub fn pick_list<'a, T, L, V, Message>(
    options: L,
    selected: Option<V>,
    on_selected: impl Fn(T) -> Message + 'a,
) -> iced_pick_list::PickList<'a, T, L, V, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: Borrow<[T]> + 'a,
    V: Borrow<T> + 'a,
    Message: Clone + 'a,
{
    iced::widget::pick_list(options, selected, on_selected).style(pick_style)
}

fn background(background: Background) -> Color {
    match background {
        Background::Color(color) => color,
        Background::Gradient(_) => Color::WHITE,
    }
}

fn luminance(color: Color) -> f32 {
    let linear = |channel: f32| {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    linear(color.r) * 0.2126 + linear(color.g) * 0.7152 + linear(color.b) * 0.0722
}

pub fn contrast(first: Color, second: Color) -> f32 {
    let first = luminance(first);
    let second = luminance(second);
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

/// Keep the requested foreground when readable; otherwise choose black or white.
pub fn readable(foreground: Color, background: Color) -> Color {
    let foreground = Color {
        a: 1.0,
        ..foreground
    };
    if contrast(foreground, background) >= 4.5 {
        foreground
    } else if contrast(Color::BLACK, background) >= contrast(Color::WHITE, background) {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

fn input_style(theme: &Theme, status: iced_text_input::Status) -> iced_text_input::Style {
    let mut style = iced_text_input::default(theme, status);
    let palette = theme.extended_palette();
    let surface = background(style.background);
    style.placeholder = readable(palette.secondary.base.text, surface);
    style.value = readable(
        if status == iced_text_input::Status::Disabled {
            palette.secondary.base.text
        } else {
            style.value
        },
        surface,
    );
    if contrast(style.value, style.selection) < 4.5 {
        let mix = |target: Color| {
            Color::from_rgb(
                surface.r * 0.85 + target.r * 0.15,
                surface.g * 0.85 + target.g * 0.15,
                surface.b * 0.85 + target.b * 0.15,
            )
        };
        let foreground = if contrast(Color::BLACK, surface) >= contrast(Color::WHITE, surface) {
            Color::BLACK
        } else {
            Color::WHITE
        };
        style.value = foreground;
        let candidate = mix(foreground);
        style.selection = if contrast(foreground, candidate) >= 4.5 {
            candidate
        } else {
            mix(if foreground == Color::BLACK {
                Color::WHITE
            } else {
                Color::BLACK
            })
        };
    }
    // Unfocused fields still need a visible boundary on pale surfaces.
    if contrast(style.border.color, surface) < 3.0 {
        style.border.color = style.placeholder;
    }
    style
}

fn pick_style(theme: &Theme, status: iced_pick_list::Status) -> iced_pick_list::Style {
    let mut style = iced_pick_list::default(theme, status);
    let surface = background(style.background);
    style.placeholder_color = readable(theme.extended_palette().secondary.base.text, surface);
    style.text_color = readable(style.text_color, surface);
    if contrast(style.border.color, surface) < 3.0 {
        style.border.color = style.placeholder_color;
    }
    style
}

pub fn button_style(theme: &Theme, mut style: button::Style) -> button::Style {
    let canvas = theme.palette().background;
    let surface = style.background.map_or(canvas, |fill| {
        let fill = background(fill);
        Color::from_rgb(
            fill.r * fill.a + canvas.r * (1.0 - fill.a),
            fill.g * fill.a + canvas.g * (1.0 - fill.a),
            fill.b * fill.a + canvas.b * (1.0 - fill.a),
        )
    });
    style.text_color = readable(style.text_color, surface);
    style
}

pub fn danger(theme: &Theme) -> iced::widget::text::Style {
    let palette = theme.extended_palette();
    let candidate = theme.palette().danger;
    let readable = [palette.background.base.color, palette.background.weak.color]
        .into_iter()
        .all(|surface| contrast(candidate, surface) >= 4.5);
    iced::widget::text::Style {
        color: Some(if readable {
            candidate
        } else {
            theme.palette().text
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_devices::storage::appearance::{Mode, Palette, Preferences};

    #[test]
    fn real_control_styles_keep_text_readable_in_every_palette_and_state() {
        for mode in [Mode::Light, Mode::Dark] {
            for palette in [
                Palette::Default,
                Palette::Slate,
                Palette::Forest,
                Palette::StacksKB,
                Palette::Custom,
            ] {
                let theme = crate::form::appearance::theme(&Preferences {
                    mode,
                    palette,
                    ..Preferences::default()
                });
                for status in [
                    iced_text_input::Status::Active,
                    iced_text_input::Status::Hovered,
                    iced_text_input::Status::Focused { is_hovered: false },
                    iced_text_input::Status::Disabled,
                ] {
                    let style = input_style(&theme, status);
                    let surface = background(style.background);
                    assert!(
                        contrast(style.placeholder, surface) >= 4.5,
                        "{mode:?}/{palette:?}/{status:?} placeholder"
                    );
                    assert!(contrast(style.value, surface) >= 4.5);
                    assert!(contrast(style.value, style.selection) >= 4.5);
                }
                for status in [
                    iced_pick_list::Status::Active,
                    iced_pick_list::Status::Hovered,
                    iced_pick_list::Status::Opened { is_hovered: false },
                ] {
                    let style = pick_style(&theme, status);
                    assert!(contrast(style.placeholder_color, background(style.background)) >= 4.5);
                    assert!(contrast(style.text_color, background(style.background)) >= 4.5);
                }
                for status in [
                    button::Status::Active,
                    button::Status::Hovered,
                    button::Status::Pressed,
                    button::Status::Disabled,
                ] {
                    for original in [
                        button::primary(&theme, status),
                        button::secondary(&theme, status),
                        button::danger(&theme, status),
                    ] {
                        let style = button_style(&theme, original);
                        let surface = style
                            .background
                            .map_or(theme.palette().background, background);
                        let canvas = theme.palette().background;
                        let surface = Color::from_rgb(
                            surface.r * surface.a + canvas.r * (1.0 - surface.a),
                            surface.g * surface.a + canvas.g * (1.0 - surface.a),
                            surface.b * surface.a + canvas.b * (1.0 - surface.a),
                        );
                        assert!(
                            contrast(style.text_color, surface) >= 4.5,
                            "{mode:?}/{palette:?}/{status:?} button"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn colored_keys_keep_legible_labels_without_changing_their_rgb() {
        for rgb in [
            [255, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
            [255, 128, 0],
            [255, 255, 255],
            [0, 0, 0],
        ] {
            let surface = Color::from_rgb8(rgb[0], rgb[1], rgb[2]);
            assert!(contrast(readable(Color::WHITE, surface), surface) >= 4.5);
        }
    }
}
