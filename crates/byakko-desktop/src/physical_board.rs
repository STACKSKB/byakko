//! Device-neutral projection of physical-key coordinates into an Iced board.
use crate::panels::{self, UiStyle};
use byakko_core::PhysicalKey;
use iced::{
    Element, Fill, Length, Size,
    widget::{container, pin, responsive, scrollable, space, stack},
};

fn bounds(keys: &[&PhysicalKey]) -> (f32, f32) {
    keys.iter().fold((0.0, 0.0), |(width, height), key| {
        (width.max(key.x + key.width), height.max(key.y + key.height))
    })
}

/// Uses the descriptor's physical geometry. Narrow windows scroll horizontally;
/// normal windows scale up only to the style's maximum key size.
pub fn view<'a, Message: Clone + 'a>(
    style: &'a UiStyle,
    keys: Vec<&'a PhysicalKey>,
    selected: Option<String>,
    on_select: impl Fn(&PhysicalKey) -> Option<Message> + 'a,
) -> Element<'a, Message> {
    colored_view(
        style,
        keys,
        selected,
        std::collections::BTreeMap::new(),
        on_select,
    )
}

pub fn colored_view<'a, Message: Clone + 'a>(
    style: &'a UiStyle,
    keys: Vec<&'a PhysicalKey>,
    selected: Option<String>,
    colors: std::collections::BTreeMap<String, [u8; 3]>,
    on_select: impl Fn(&PhysicalKey) -> Option<Message> + 'a,
) -> Element<'a, Message> {
    let (span, rise) = bounds(&keys);
    container(responsive(move |size: Size| {
        let unit = (size.width / span.max(1.0))
            .min(size.height / rise.max(1.0))
            .clamp(style.board.min_unit, style.board.max_unit);
        let board_width = span * unit;
        let board_height = rise * unit;
        let base: Element<'_, Message> = space()
            .width(Length::Fixed(board_width))
            .height(Length::Fixed(board_height))
            .into();
        let board = keys.iter().fold(stack![base], |board, key| {
            let width = (key.width * unit - style.board.key_gap).max(1.0);
            let height = (key.height * unit - style.board.key_gap).max(1.0);
            let mut key_button = panels::selectable_button_with_size(
                style,
                key.label.clone(),
                selected.as_deref() == Some(key.id.as_str()),
                on_select(key),
                Some((width, height)),
            );
            if let Some(rgb) = colors.get(&key.id) {
                let [r, g, b] = *rgb;
                let selected = selected.as_deref() == Some(key.id.as_str());
                key_button = iced::widget::button(
                    iced::widget::text(key.label.clone())
                        .size(style.board.key_label_size)
                        .center(),
                )
                .width(width)
                .height(height)
                .padding(0)
                .on_press_maybe(on_select(key))
                .style(move |theme, status| {
                    let mut appearance = iced::widget::button::secondary(theme, status);
                    appearance.background =
                        Some(iced::Background::Color(iced::Color::from_rgb8(r, g, b)));
                    appearance.text_color =
                        if u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 > 128_000 {
                            iced::Color::BLACK
                        } else {
                            iced::Color::WHITE
                        };
                    if selected {
                        appearance.border.width = 2.0;
                        appearance.border.color = theme.extended_palette().primary.strong.color;
                    }
                    appearance
                })
                .into();
            }
            board.push(pin(key_button).x(key.x * unit).y(key.y * unit))
        });
        scrollable(
            container(
                board
                    .width(Length::Fixed(board_width))
                    .height(Length::Fixed(board_height)),
            )
            .center_x(Fill),
        )
        .horizontal()
        .width(Fill)
        .into()
    }))
    .height(Length::Fixed(
        rise * style.board.min_unit + style.spacing.s as f32,
    ))
    .width(Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(id: &str, x: f32, y: f32, width: f32) -> PhysicalKey {
        PhysicalKey {
            id: id.into(),
            label: id.into(),
            x,
            y,
            width,
            height: 1.0,
            visible: true,
            writable: true,
        }
    }

    #[test]
    fn descriptor_bounds_include_stagger_and_tall_keys() {
        let esc = key("esc", 0.0, 0.0, 1.0);
        let mut enter = key("enter", 13.0, 2.0, 2.0);
        enter.height = 2.0;
        let arrow = key("arrow", 17.5, 5.5, 1.0);
        assert_eq!(bounds(&[&enter, &arrow, &esc]), (18.5, 6.5));
    }
}
