//! Search and category navigation for advertised actions; no device operations.
use crate::{Desktop, Message as AppMessage, panels};
use byakko_core::{Action, ActionCategory, ActionChoice};
use iced::{
    Element, Event, Fill,
    widget::{button, column, container, row, scrollable, text, text_input, tooltip},
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum InputMode {
    #[default]
    Browse,
    Capture,
}

#[derive(Clone, Debug)]
pub enum Message {
    Category(ActionCategory),
    Search(String),
    SubmitSearch,
    Capture,
    CancelCapture,
    Captured(u16),
}

#[derive(Default)]
pub struct Browser {
    pub category: Option<ActionCategory>,
    pub query: String,
    pub input: InputMode,
}

const GROUPS: &[ActionCategory] = &[
    ActionCategory::Alphanumeric,
    ActionCategory::Modifiers,
    ActionCategory::Navigation,
    ActionCategory::Function,
    ActionCategory::Numpad,
    ActionCategory::Media,
    ActionCategory::Mouse,
    ActionCategory::System,
    ActionCategory::Shortcuts,
    ActionCategory::Other,
];

pub fn group_label(group: ActionCategory) -> &'static str {
    match group {
        ActionCategory::Alphanumeric => "Alphanumeric",
        ActionCategory::Modifiers => "Modifiers",
        ActionCategory::Navigation => "Navigation",
        ActionCategory::Function => "Function keys",
        ActionCategory::Numpad => "Numpad",
        ActionCategory::Media => "Media keys",
        ActionCategory::Mouse => "Mouse",
        ActionCategory::System => "System",
        ActionCategory::Shortcuts => "Shortcuts",
        ActionCategory::Other => "Other",
    }
}

pub fn compact_label(choice: &ActionChoice) -> String {
    if let Action::Key(usage) = choice.action {
        match usage {
            0x59..=0x61 => return format!("Num {}", usage - 0x58),
            0x62 => return "Num 0".into(),
            0x63 => return "Num .".into(),
            0x54 => return "Num /".into(),
            0x55 => return "Num *".into(),
            0x56 => return "Num −".into(),
            0x57 => return "Num +".into(),
            0x58 => return "Num Enter".into(),
            _ => {}
        }
    }
    choice.label.clone()
}

fn matches(choice: &ActionChoice, query: &str) -> bool {
    let haystack = format!(
        "{} {} {}",
        choice.label,
        compact_label(choice),
        group_label(choice.category)
    )
    .to_lowercase();
    query
        .split_whitespace()
        .all(|word| haystack.contains(&word.to_lowercase()))
}

pub fn results<'a>(
    actions: &'a [ActionChoice],
    browser: &Browser,
) -> Vec<(usize, &'a ActionChoice)> {
    let category = browser.category.or_else(|| {
        GROUPS
            .iter()
            .copied()
            .find(|group| actions.iter().any(|choice| choice.category == *group))
    });
    actions
        .iter()
        .enumerate()
        .filter(|(_, choice)| {
            if browser.query.trim().is_empty() {
                Some(choice.category) == category
            } else {
                matches(choice, &browser.query)
            }
        })
        .collect()
}

fn exact_match(actions: &[ActionChoice], browser: &Browser) -> Option<usize> {
    let query = browser.query.trim();
    if query.is_empty() {
        return None;
    }
    let found = results(actions, browser);
    let exact: Vec<_> = found
        .iter()
        .filter(|(_, choice)| {
            choice.label.eq_ignore_ascii_case(query)
                || compact_label(choice).eq_ignore_ascii_case(query)
        })
        .collect();
    if exact.len() == 1 {
        Some(exact[0].0)
    } else if found.len() == 1 {
        Some(found[0].0)
    } else {
        None
    }
}

impl Desktop {
    pub fn update_catalog(&mut self, message: Message) {
        match message {
            Message::Category(category) => {
                self.action_browser.category = Some(category);
                self.action_browser.query.clear();
                self.action_browser.input = InputMode::Browse;
            }
            Message::Search(query) => {
                self.action_browser.query = query;
                self.action_browser.input = InputMode::Browse;
            }
            Message::SubmitSearch => {
                if let Some(index) =
                    exact_match(&self.session.descriptor().actions, &self.action_browser)
                {
                    self.stage(index);
                }
            }
            Message::Capture => self.action_browser.input = InputMode::Capture,
            Message::CancelCapture => self.action_browser.input = InputMode::Browse,
            Message::Captured(usage) => {
                if self.action_browser.input != InputMode::Capture || self.page != crate::Page::Keys
                {
                    return;
                }
                self.action_browser.input = InputMode::Browse;
                if let Some(index) = self
                    .session
                    .descriptor()
                    .actions
                    .iter()
                    .position(|choice| choice.action == Action::Key(usage))
                {
                    self.stage(index);
                } else {
                    self.notice = Some(
                        "This key is not supported as an assignment. Choose an advertised action."
                            .into(),
                    );
                }
            }
        }
    }
}

pub fn capture_event(
    event: Event,
    _: iced::event::Status,
    _: iced::window::Id,
) -> Option<AppMessage> {
    if matches!(event, Event::Window(iced::window::Event::Unfocused)) {
        return Some(AppMessage::Catalog(Message::CancelCapture));
    }
    if let Some(byakko_core::macros::Action::Key {
        usage,
        pressed: true,
    }) = crate::recording_input::action(&event)
    {
        return Some(AppMessage::Catalog(if usage == 0x29 {
            Message::CancelCapture
        } else {
            Message::Captured(usage)
        }));
    }
    None
}

pub fn view(app: &Desktop) -> Element<'_, AppMessage> {
    let style = &app.ui;
    let actions = &app.session.descriptor().actions;
    let editable = !app.busy()
        && app.session.status() == &byakko_core::session::Status::Ready
        && app
            .session
            .descriptor()
            .keys
            .iter()
            .any(|key| key.writable && Some(&key.id) == app.selected.as_ref());
    let selected_group = app.action_browser.category.or_else(|| {
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
                    group_label(*group),
                    app.action_browser.query.is_empty() && selected_group == Some(*group),
                    Some(AppMessage::Catalog(Message::Category(*group))),
                )
            }),
    )
    .spacing(style.spacing.xs);
    let choices = results(actions, &app.action_browser);
    let heading = if app.action_browser.query.trim().is_empty() {
        selected_group
            .map(group_label)
            .unwrap_or("Actions")
            .to_string()
    } else {
        format!("Search results · {}", choices.len())
    };
    let selected_action = app
        .selected
        .as_ref()
        .and_then(|key| app.session.draft()?.get(&app.layer)?.get(key));
    let tiles = row(choices.iter().map(|(index, choice)| {
        let label = compact_label(choice);
        let width = (label.chars().count() as f32 * style.action_character_width
            + style.spacing.control_padding as f32 * 2.0)
            .clamp(style.board.min_unit, style.fields.regular as f32);
        let tile = panels::selectable_button_with_size(
            style,
            label,
            selected_action == Some(&choice.action),
            editable.then_some(AppMessage::Stage(*index)),
            Some((width, style.board.min_unit)),
        );
        tooltip(
            tile,
            text(&choice.label),
            iced::widget::tooltip::Position::Top,
        )
        .into()
    }))
    .spacing(style.spacing.xs)
    .wrap();
    let results: Element<'_, AppMessage> = if choices.is_empty() {
        text("No matching actions").into()
    } else {
        tiles.into()
    };
    let capture = if app.action_browser.input == InputMode::Capture {
        button("Press a key… Esc cancels").on_press(AppMessage::Catalog(Message::CancelCapture))
    } else {
        button("Type a key")
            .on_press_maybe(editable.then_some(AppMessage::Catalog(Message::Capture)))
    };
    column![
        text("Assign action").size(style.type_scale.section_title),
        text_input("Type A, Num 9, calculator…", &app.action_browser.query)
            .on_input(|value| AppMessage::Catalog(Message::Search(value)))
            .on_submit_maybe(editable.then_some(AppMessage::Catalog(Message::SubmitSearch))),
        capture,
        row![
            container(scrollable(groups))
                .width(style.action_group_width)
                .height(Fill),
            container(scrollable(
                column![text(heading), results].spacing(style.spacing.s)
            ))
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

#[cfg(test)]
mod tests {
    use super::*;
    fn choices() -> Vec<ActionChoice> {
        vec![
            ActionChoice {
                label: "A".into(),
                action: Action::Key(4),
                category: ActionCategory::Alphanumeric,
            },
            ActionChoice {
                label: "Numpad 9".into(),
                action: Action::Key(0x61),
                category: ActionCategory::Numpad,
            },
        ]
    }
    #[test]
    fn browsing_is_grouped_but_search_crosses_groups_and_accepts_num_alias() {
        let actions = choices();
        let mut browser = Browser::default();
        assert_eq!(results(&actions, &browser).len(), 1);
        browser.query = "num 9".into();
        assert_eq!(exact_match(&actions, &browser), Some(1));
        browser.query.clear();
        browser.category = Some(ActionCategory::Numpad);
        assert_eq!(results(&actions, &browser)[0].0, 1);
        browser.query = "a".into();
        assert_eq!(exact_match(&actions, &browser), Some(0));
    }
}
