//! The complete advertised catalog, with section navigation and search.
use crate::{
    form::{
        catalog::{self, Form, InputMode, Message},
        keymap,
    },
    widget::{
        catalog::{GROUPS, section_id},
        panels::{self, UiStyle},
    },
};
use byakko_core::model::keymap::{Action, ActionChoice};
use iced::{
    Element, Fill,
    widget::{button, column, container, row, scrollable, text, text_input},
};

pub fn view<'a>(
    form: &'a Form,
    actions: &'a [ActionChoice],
    selected: Option<&Action>,
    editable: bool,
    style: &'a UiStyle,
) -> Element<'a, keymap::Message> {
    let selected_group = form.category.or_else(|| {
        GROUPS
            .iter()
            .copied()
            .find(|group| actions.iter().any(|choice| choice.category == *group))
    });
    let groups = column(
        GROUPS
            .iter()
            .filter(|group| actions.iter().any(|choice| choice.category == **group))
            .map(|group| {
                panels::selectable_button_fill_width(
                    style,
                    catalog::group_label(*group),
                    form.query.is_empty() && selected_group == Some(*group),
                    Some(keymap::Message::Catalog(Message::Category(*group))),
                )
            }),
    )
    .spacing(style.spacing.xs);
    let choices = catalog::results(actions, &form.query);
    let sections = column(GROUPS.iter().filter_map(|group| {
        let group_choices: Vec<_> = choices
            .iter()
            .filter(|(_, choice)| choice.category == *group)
            .collect();
        if group_choices.is_empty() {
            return None;
        }
        let tiles = row(group_choices.into_iter().map(|(index, choice)| {
            let label = catalog::compact_label(choice);
            let width = (label.chars().count() as f32 * style.action_character_width
                + style.spacing.control_padding as f32 * 2.0)
                .clamp(style.board.min_unit, style.fields.regular as f32);
            panels::selectable_button_with_size(
                style,
                label,
                selected == Some(&choice.action),
                editable.then_some(keymap::Message::Assign(*index)),
                Some((width, style.board.min_unit)),
            )
        }))
        .spacing(style.spacing.xs)
        .wrap();
        Some(
            container(
                column![
                    text(catalog::group_label(*group)).size(style.type_scale.section_title),
                    tiles
                ]
                .spacing(style.spacing.s),
            )
            .id(section_id(*group))
            .into(),
        )
    }))
    .spacing(style.spacing.l);
    let results: Element<'_, keymap::Message> = if choices.is_empty() {
        text("No matching actions").into()
    } else {
        sections.into()
    };
    let capture = if form.input == InputMode::Capture {
        button("Press a key… Esc cancels")
            .on_press(keymap::Message::Catalog(Message::CancelCapture))
    } else {
        button("Type a key")
            .on_press_maybe(editable.then_some(keymap::Message::Catalog(Message::Capture)))
    };
    column![
        text("Assign action").size(style.type_scale.section_title),
        text_input("Type A, Num 9, calculator…", &form.query)
            .on_input(|value| keymap::Message::Catalog(Message::Search(value)))
            .on_submit_maybe(editable.then_some(keymap::Message::Catalog(Message::SubmitSearch))),
        capture,
        row![
            container(scrollable(groups))
                .width(style.action_group_width)
                .height(Fill),
            container(
                scrollable(container(results).width(Fill).padding(iced::Padding {
                    right: f32::from(style.scrollbar_width + style.scrollbar_inset),
                    ..Default::default()
                }))
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new()
                        .width(u32::from(style.scrollbar_width))
                        .scroller_width(u32::from(style.scrollbar_width))
                ))
                .id("action-catalog")
                .on_scroll(|_| keymap::Message::Catalog(Message::Scrolled))
            )
            .width(Fill)
            .height(Fill),
        ]
        .spacing(style.spacing.m)
        .height(Fill),
    ]
    .spacing(style.spacing.s)
    .height(Fill)
    .into()
}
