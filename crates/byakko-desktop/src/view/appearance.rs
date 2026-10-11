// SPDX-License-Identifier: GPL-3.0-or-later
//! Compact appearance dialog content; device lighting remains unrelated.
use crate::widget::controls::{pick_list, text_input};
use crate::{
    form::appearance::{self, Form, Message, ROLES},
    widget::{focus_button::button, focus_control, panels::UiStyle},
};
use byakko_devices::storage::appearance::{Mode, Palette};
use iced::{
    Alignment, Color, Element, Fill,
    widget::{column, container, row, space, text},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ModeChoice(Mode);
impl std::fmt::Display for ModeChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            Mode::Light => "Light",
            Mode::Dark => "Dark",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PaletteChoice(Palette);
impl std::fmt::Display for PaletteChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            Palette::Default => "Default",
            Palette::Slate => "Slate",
            Palette::Forest => "Forest",
            Palette::StacksKB => "StacksKB",
            Palette::Custom => "Custom",
        })
    }
}

pub fn view<'a>(form: &'a Form, style: &'a UiStyle) -> Element<'a, Message> {
    let busy = form.submitted.is_some();
    let modes = [ModeChoice(Mode::Light), ModeChoice(Mode::Dark)];
    let palettes = [
        PaletteChoice(Palette::Default),
        PaletteChoice(Palette::Slate),
        PaletteChoice(Palette::Forest),
        PaletteChoice(Palette::StacksKB),
        PaletteChoice(Palette::Custom),
    ];
    let mode = focus_control::select(
        pick_list(modes, Some(ModeChoice(form.draft.mode)), |choice| {
            Message::Mode(choice.0)
        })
        .width(300),
        modes.iter().map(|choice| Message::Mode(choice.0)).collect(),
        modes.iter().position(|choice| choice.0 == form.draft.mode),
        !busy,
    );
    let palette = focus_control::select(
        pick_list(
            palettes,
            Some(PaletteChoice(form.draft.palette)),
            |choice| Message::Palette(choice.0),
        )
        .width(300),
        palettes
            .iter()
            .map(|choice| Message::Palette(choice.0))
            .collect(),
        palettes
            .iter()
            .position(|choice| choice.0 == form.draft.palette),
        !busy,
    );

    let mut body = column![
        text("Appearance").size(style.type_scale.section_title),
        text("Choose colours for the app.").size(12),
        row![text("Theme").width(112), mode]
            .spacing(style.spacing.s)
            .align_y(Alignment::Center)
            .height(40),
        row![text("Colour palette").width(112), palette]
            .spacing(style.spacing.s)
            .align_y(Alignment::Center)
            .height(40),
    ]
    .spacing(style.spacing.s);

    let colors = appearance::colours(&form.draft);
    let values = [
        colors.canvas,
        colors.surface,
        colors.text,
        colors.muted,
        colors.accent,
        colors.rule,
        colors.focus,
    ];
    for (index, label) in ROLES.iter().enumerate() {
        let mut field = text_input("#RRGGBB", &form.fields[index]).width(130);
        if !busy {
            field = field.on_input(move |value| Message::Colour(index, value));
        }
        let [red, green, blue] = values[index];
        let swatch = container(space())
            .width(22)
            .height(22)
            .style(move |_| container::background(Color::from_rgb8(red, green, blue)));
        body = body.push(
            row![text(*label).width(112), field, swatch]
                .spacing(style.spacing.s)
                .align_y(Alignment::Center)
                .height(40),
        );
    }
    if let Some(problem) = &form.problem {
        body = body.push(text(problem).size(12));
    }
    let actions = row![
        button("Reset colours").on_press_maybe((!busy).then_some(Message::Reset)),
        space::horizontal(),
        button("Cancel").on_press_maybe((!busy).then_some(Message::Cancel)),
        button(if busy { "Saving…" } else { "Apply" })
            .on_press_maybe((!busy).then_some(Message::Apply)),
    ]
    .spacing(style.spacing.s)
    .align_y(Alignment::Center)
    .height(40);

    container(
        column![
            crate::widget::panels::vertical_scroll(style, body)
                .width(Fill)
                .height(Fill),
            actions,
        ]
        .spacing(style.spacing.s),
    )
    .width(480)
    .max_height(600)
    .padding(style.spacing.page_padding)
    .style(container::bordered_box)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{
        Event, Point, Rectangle, Size,
        advanced::{
            Layout, layout,
            widget::{Operation, Tree},
        },
        mouse,
    };

    #[derive(Default)]
    struct Labels(Vec<(String, Rectangle)>);
    impl Operation for Labels {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn text(&mut self, _: Option<&iced::advanced::widget::Id>, bounds: Rectangle, value: &str) {
            self.0.push((value.to_owned(), bounds));
        }
    }

    #[test]
    fn dialog_stays_bounded_and_exposes_theme_palette_and_colour_fields() {
        let mut form = Form::default();
        form.update(Message::Open);
        let mut element = view(&form, &UiStyle::DEFAULT);
        let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
        let mut tree = Tree::new(&element);
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(480.0, 320.0)),
        );
        let bounds = Layout::new(&node).bounds();
        assert!(
            bounds.width <= 480.0 && bounds.height <= 320.0,
            "{bounds:?}"
        );
        let mut labels = Labels::default();
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut labels);
        for expected in ["Appearance", "Theme", "Colour palette", "Canvas", "Focus"] {
            assert!(
                labels.0.iter().any(|(value, _)| value == expected),
                "{expected}: {:?}",
                labels.0
            );
        }
        let apply = labels
            .0
            .iter()
            .find(|(value, _)| value == "Apply")
            .unwrap()
            .1;
        assert!(
            apply.y >= 0.0 && apply.y + apply.height <= 320.0,
            "{apply:?}"
        );
        let cursor = mouse::Cursor::Available(Point::new(
            apply.x + apply.width / 2.0,
            apply.y + apply.height / 2.0,
        ));
        let viewport = Rectangle::with_size(Size::new(480.0, 320.0));
        let mut messages = Vec::new();
        for event in [
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ] {
            element.as_widget_mut().update(
                &mut tree,
                &event,
                Layout::new(&node),
                cursor,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &viewport,
            );
        }
        assert!(matches!(messages.as_slice(), [Message::Apply]));
    }
}
