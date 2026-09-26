//! Navigation measured from rendered action sections, including wrapped layouts.
use crate::form::catalog::Message;
use byakko_core::model::keymap::ActionCategory;

pub(crate) fn jump(category: ActionCategory) -> iced::Task<Message> {
    iced::advanced::widget::operate(JumpToSection::new(category))
}

pub(crate) fn top() -> iced::Task<Message> {
    iced::widget::operation::scroll_to(
        "action-catalog",
        iced::widget::operation::AbsoluteOffset { x: 0.0, y: 0.0 },
    )
}

pub(crate) fn visible() -> iced::Task<Message> {
    iced::advanced::widget::operate(VisibleSection::default())
}

pub(crate) const GROUPS: &[ActionCategory] = &[
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

pub(crate) fn section_id(group: ActionCategory) -> iced::advanced::widget::Id {
    format!("action-section-{group:?}").into()
}

// Measure the rendered section, so jumps remain accurate after wrapping/resizing.
struct JumpToSection {
    target: iced::advanced::widget::Id,
    content_y: Option<f32>,
    section_y: Option<f32>,
}

impl JumpToSection {
    fn new(group: ActionCategory) -> Self {
        Self {
            target: section_id(group),
            content_y: None,
            section_y: None,
        }
    }
}

impl iced::advanced::widget::Operation<Message> for JumpToSection {
    fn traverse(
        &mut self,
        operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<Message>),
    ) {
        operate(self);
    }

    fn container(&mut self, id: Option<&iced::advanced::widget::Id>, bounds: iced::Rectangle) {
        if id == Some(&self.target) {
            self.section_y = Some(bounds.y);
        }
    }

    fn scrollable(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        _: iced::Rectangle,
        content: iced::Rectangle,
        _: iced::Vector,
        _: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        if id == Some(&iced::advanced::widget::Id::new("action-catalog")) {
            self.content_y = Some(content.y);
        }
    }

    fn finish(&self) -> iced::advanced::widget::operation::Outcome<Message> {
        use iced::advanced::widget::operation::{Outcome, scrollable};
        match (self.content_y, self.section_y) {
            (Some(origin), Some(section)) => Outcome::Chain(Box::new(scrollable::scroll_to(
                iced::advanced::widget::Id::new("action-catalog"),
                scrollable::AbsoluteOffset {
                    x: None,
                    y: Some((section - origin).max(0.0)),
                },
            ))),
            _ => Outcome::None,
        }
    }
}

#[derive(Default)]
struct VisibleSection {
    viewport: Option<(f32, bool)>,
    sections: Vec<(ActionCategory, f32)>,
}

fn current_section(
    sections: &[(ActionCategory, f32)],
    top: f32,
    at_end: bool,
) -> Option<ActionCategory> {
    sections
        .iter()
        .rev()
        .find(|(_, y)| at_end || *y <= top + 1.0)
        .or_else(|| sections.first())
        .map(|(group, _)| *group)
}

impl iced::advanced::widget::Operation<Message> for VisibleSection {
    fn traverse(
        &mut self,
        operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<Message>),
    ) {
        operate(self);
    }

    fn container(&mut self, id: Option<&iced::advanced::widget::Id>, bounds: iced::Rectangle) {
        if let Some(group) = GROUPS.iter().find(|group| id == Some(&section_id(**group))) {
            self.sections.push((*group, bounds.y));
        }
    }

    fn scrollable(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        bounds: iced::Rectangle,
        content: iced::Rectangle,
        translation: iced::Vector,
        _: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        if id == Some(&iced::advanced::widget::Id::new("action-catalog")) {
            self.viewport = Some((
                content.y + translation.y,
                content.height > bounds.height
                    && translation.y >= content.height - bounds.height - 1.0,
            ));
        }
    }

    fn finish(&self) -> iced::advanced::widget::operation::Outcome<Message> {
        use iced::advanced::widget::operation::Outcome;
        self.viewport
            .and_then(|(top, end)| current_section(&self.sections, top, end))
            .map_or(Outcome::None, |group| {
                Outcome::Some(Message::VisibleSection(group))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn highlight_follows_the_top_section_and_the_end_of_the_list() {
        let sections = [
            (ActionCategory::Alphanumeric, 100.0),
            (ActionCategory::Modifiers, 300.0),
            (ActionCategory::Media, 600.0),
        ];
        assert_eq!(
            current_section(&sections, 100.0, false),
            Some(ActionCategory::Alphanumeric)
        );
        assert_eq!(
            current_section(&sections, 450.0, false),
            Some(ActionCategory::Modifiers)
        );
        assert_eq!(
            current_section(&sections, 620.0, false),
            Some(ActionCategory::Media)
        );
        assert_eq!(
            current_section(&sections, 120.0, false),
            Some(ActionCategory::Alphanumeric)
        );
        assert_eq!(
            current_section(&sections, 450.0, true),
            Some(ActionCategory::Media)
        );
        assert_eq!(current_section(&[], 0.0, false), None);
    }
    #[test]
    fn section_jump_uses_rendered_coordinates_not_estimated_row_counts() {
        use iced::advanced::widget::{
            Operation,
            operation::{
                Outcome, Scrollable,
                scrollable::{AbsoluteOffset, RelativeOffset},
            },
        };
        #[derive(Default)]
        struct ScrollState(Option<f32>);
        impl Scrollable for ScrollState {
            fn snap_to(&mut self, _: RelativeOffset<Option<f32>>) {}
            fn scroll_to(&mut self, offset: AbsoluteOffset<Option<f32>>) {
                self.0 = offset.y;
            }
            fn scroll_by(&mut self, _: AbsoluteOffset, _: iced::Rectangle, _: iced::Rectangle) {}
        }
        for section_y in [250.0, 610.0] {
            let mut jump = JumpToSection::new(ActionCategory::Numpad);
            let mut state = ScrollState::default();
            let bounds = iced::Rectangle {
                y: 50.0,
                ..Default::default()
            };
            let id = iced::advanced::widget::Id::new("action-catalog");
            jump.scrollable(
                Some(&id),
                bounds,
                bounds,
                iced::Vector::new(0.0, 100.0),
                &mut state,
            );
            jump.container(
                Some(&section_id(ActionCategory::Numpad)),
                iced::Rectangle {
                    y: section_y,
                    ..Default::default()
                },
            );
            let Outcome::Chain(mut scroll) = jump.finish() else {
                panic!("missing jump");
            };
            scroll.scrollable(Some(&id), bounds, bounds, iced::Vector::ZERO, &mut state);
            assert_eq!(state.0, Some(section_y - bounds.y));
        }
    }
}
