//! The complete advertised catalog, with section navigation and search.
use crate::widget::controls::{pick_list, text_input};
use crate::widget::focus_button::button;
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
use byakko_core::model::keymap::{Action, ActionCategory, ActionChoice};
use iced::{
    Element, Fill,
    widget::{column, container, responsive, row, text},
};

#[derive(Clone, Copy, PartialEq)]
struct Category(ActionCategory);

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(catalog::group_label(self.0))
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Layout {
    Sidebar,
    Selector,
    WholePaneScroll,
}

pub fn view<'a>(
    form: &'a Form,
    actions: &'a [ActionChoice],
    selected: Option<&'a Action>,
    editable: bool,
    can_capture: bool,
    style: &'a UiStyle,
    whole_pane_scroll: bool,
) -> Element<'a, keymap::Message> {
    if whole_pane_scroll {
        return catalog_layout(
            form,
            actions,
            selected,
            editable,
            can_capture,
            style,
            Layout::WholePaneScroll,
        );
    }
    responsive(move |size| {
        let compact = size.width < (style.action_group_width + style.fields.regular) as f32;
        catalog_layout(
            form,
            actions,
            selected,
            editable,
            can_capture,
            style,
            if compact {
                Layout::Selector
            } else {
                Layout::Sidebar
            },
        )
    })
    .into()
}

fn catalog_layout<'a>(
    form: &'a Form,
    actions: &'a [ActionChoice],
    selected: Option<&Action>,
    editable: bool,
    can_capture: bool,
    style: &'a UiStyle,
    layout: Layout,
) -> Element<'a, keymap::Message> {
    let selected_group = form.category.or_else(|| {
        GROUPS
            .iter()
            .copied()
            .find(|group| actions.iter().any(|choice| choice.category == *group))
    });
    let groups = GROUPS
        .iter()
        .copied()
        .filter(|group| actions.iter().any(|choice| choice.category == *group));
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
        button("Type a key").on_press_maybe(
            (editable && can_capture).then_some(keymap::Message::Catalog(Message::Capture)),
        )
    };
    let title = text("Assign action").size(style.type_scale.section_title);
    let search = text_input("Type A, Num 9, calculator…", &form.query)
        .on_input(|value| keymap::Message::Catalog(Message::Search(value)))
        .on_submit_maybe(editable.then_some(keymap::Message::Catalog(Message::SubmitSearch)));
    let selector = || {
        pick_list(
            groups.clone().map(Category).collect::<Vec<_>>(),
            selected_group.map(Category),
            |Category(group)| keymap::Message::Catalog(Message::Category(group)),
        )
        .placeholder("Category")
    };
    if layout == Layout::WholePaneScroll {
        return column![
            row![title, selector()]
                .spacing(style.spacing.s)
                .align_y(iced::Alignment::Center)
                .wrap(),
            row![search, capture].spacing(style.spacing.s),
            results,
        ]
        .spacing(style.spacing.s)
        .into();
    }
    let results = panels::vertical_scroll(style, results)
        .id("action-catalog")
        .on_scroll(|_| keymap::Message::Catalog(Message::Scrolled))
        .height(Fill);
    let body: Element<'_, keymap::Message> = if layout != Layout::Sidebar {
        column![selector(), results]
            .spacing(style.spacing.s)
            .height(Fill)
            .into()
    } else {
        let index = column(groups.map(|group| {
            panels::selectable_button_fill_width(
                style,
                catalog::group_label(group),
                form.query.is_empty() && selected_group == Some(group),
                Some(keymap::Message::Catalog(Message::Category(group))),
            )
        }))
        .spacing(style.spacing.xs);
        row![
            container(panels::vertical_scroll(style, index))
                .width(style.action_group_width)
                .height(Fill),
            container(results).width(Fill).height(Fill),
        ]
        .spacing(style.spacing.m)
        .height(Fill)
        .into()
    };
    column![title, search, capture, body]
        .spacing(style.spacing.s)
        .height(Fill)
        .into()
}
