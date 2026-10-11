// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use iced::{
    Point, Rectangle, Size,
    advanced::{
        Layout, layout,
        widget::{self, Operation, Tree, operation::focusable},
    },
    keyboard, mouse,
};

#[derive(Default)]
struct Focused {
    total: usize,
    focused: Option<usize>,
}
impl Operation for Focused {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }
    fn focusable(
        &mut self,
        _: Option<&widget::Id>,
        _: Rectangle,
        state: &mut dyn focusable::Focusable,
    ) {
        if state.is_focused() {
            self.focused = Some(self.total);
        }
        self.total += 1;
    }
}
#[derive(Default)]
struct Buttons(Vec<Rectangle>);
impl Operation for Buttons {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }
    fn focusable(
        &mut self,
        _: Option<&widget::Id>,
        bounds: Rectangle,
        _: &mut dyn focusable::Focusable,
    ) {
        self.0.push(bounds);
    }
}

fn key(code: keyboard::key::Code, named: keyboard::key::Named, shift: bool) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(named),
        modified_key: keyboard::Key::Named(named),
        physical_key: keyboard::key::Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers: if shift {
            keyboard::Modifiers::SHIFT
        } else {
            keyboard::Modifiers::default()
        },
        text: None,
        repeat: false,
    })
}

#[test]
fn tab_skips_disabled_button_and_enter_space_follow_real_page_messages() {
    use crate::widget::{focus_button::button, focus_navigation::navigate};
    let controls = iced::widget::column![
        button("Macros").on_press(Message::Page(Page::Macros)),
        button("Unavailable").on_press_maybe(None),
        button("Settings").on_press(Message::Page(Page::Settings)),
    ];
    let mut element = navigate(controls.into(), true, false);
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let mut tree = Tree::new(&element);
    let viewport = Rectangle::with_size(Size::new(400.0, 200.0));
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, viewport.size()),
    );
    {
        let mut press = |event| {
            let mut messages = Vec::new();
            element.as_widget_mut().update(
                &mut tree,
                &event,
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &viewport,
            );
            messages
        };
        assert!(
            press(key(
                keyboard::key::Code::Tab,
                keyboard::key::Named::Tab,
                false
            ))
            .is_empty()
        );
        assert!(matches!(
            press(key(
                keyboard::key::Code::Enter,
                keyboard::key::Named::Enter,
                false
            ))
            .as_slice(),
            [Message::Page(Page::Macros)]
        ));
        assert!(
            press(key(
                keyboard::key::Code::Tab,
                keyboard::key::Named::Tab,
                false
            ))
            .is_empty()
        );
        assert!(matches!(
            press(key(
                keyboard::key::Code::Space,
                keyboard::key::Named::Space,
                false
            ))
            .as_slice(),
            [Message::Page(Page::Settings)]
        ));
        assert!(
            press(key(
                keyboard::key::Code::Tab,
                keyboard::key::Named::Tab,
                true
            ))
            .is_empty()
        );
        assert!(matches!(
            press(key(
                keyboard::key::Code::Enter,
                keyboard::key::Named::Enter,
                false
            ))
            .as_slice(),
            [Message::Page(Page::Macros)]
        ));
    }
    let mut count = Focused::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut count);
    assert_eq!((count.focused, count.total), (Some(0), 2));
}

#[test]
fn catalog_capture_does_not_activate_focused_page_button() {
    use crate::widget::{focus_button::button, focus_navigation::navigate};
    let mut element = navigate(
        button("Settings")
            .on_press(Message::Page(Page::Settings))
            .into(),
        true,
        true,
    );
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let mut tree = Tree::new(&element);
    let viewport = Rectangle::with_size(Size::new(400.0, 100.0));
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, viewport.size()),
    );
    let mut operation = focusable::focus_next::<()>();
    // The operation is chained; the first pass counts, then the second focuses.
    use iced::advanced::widget::Operation as _;
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut operation);
    if let widget::operation::Outcome::Chain(mut next) = operation.finish() {
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, next.as_mut());
    }
    let mut messages = Vec::new();
    let mut shell = iced::advanced::Shell::new(&mut messages);
    element.as_widget_mut().update(
        &mut tree,
        &key(
            keyboard::key::Code::Enter,
            keyboard::key::Named::Enter,
            false,
        ),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut shell,
        &viewport,
    );
    assert!(shell.is_event_captured());
    assert!(messages.is_empty());
}

#[test]
fn clicking_another_button_moves_focus_before_enter() {
    use crate::widget::{focus_button::button, focus_navigation::navigate};
    let mut element = navigate(
        iced::widget::column![
            button("Macros").on_press(Message::Page(Page::Macros)),
            button("Settings").on_press(Message::Page(Page::Settings)),
        ]
        .into(),
        true,
        false,
    );
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let mut tree = Tree::new(&element);
    let viewport = Rectangle::with_size(Size::new(400.0, 120.0));
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, viewport.size()),
    );
    let mut bounds = Buttons::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
    assert_eq!(bounds.0.len(), 2);
    let mut send = |event: Event, cursor| {
        let mut messages = Vec::new();
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
        messages
    };
    let first = bounds.0[0];
    let second = bounds.0[1];
    let at = |r: Rectangle| {
        mouse::Cursor::Available(Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0))
    };
    send(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        at(first),
    );
    assert!(matches!(
        send(
            key(
                keyboard::key::Code::Enter,
                keyboard::key::Named::Enter,
                false
            ),
            mouse::Cursor::Unavailable
        )
        .as_slice(),
        [Message::Page(Page::Macros)]
    ));
    send(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        at(second),
    );
    assert!(matches!(
        send(
            key(
                keyboard::key::Code::Enter,
                keyboard::key::Named::Enter,
                false
            ),
            mouse::Cursor::Unavailable
        )
        .as_slice(),
        [Message::Page(Page::Settings)]
    ));
}

#[test]
fn tab_reveals_focused_control_in_scrollable_panel() {
    use crate::widget::{focus_button::button, focus_navigation::navigate};
    #[derive(Default)]
    struct ScrollOffset(f32);
    impl Operation for ScrollOffset {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
            visit(self);
        }
        fn scrollable(
            &mut self,
            _: Option<&widget::Id>,
            _: Rectangle,
            _: Rectangle,
            translation: iced::Vector,
            _: &mut dyn widget::operation::Scrollable,
        ) {
            self.0 = translation.y;
        }
    }
    let content = iced::widget::scrollable(iced::widget::column![
        button("One").on_press(Message::Page(Page::Keys)),
        button("Two").on_press(Message::Page(Page::Macros)),
        button("Three").on_press(Message::Page(Page::Lighting)),
        button("Four").on_press(Message::Page(Page::Settings)),
    ])
    .height(45);
    let mut element = navigate(content.into(), true, false);
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let mut tree = Tree::new(&element);
    let viewport = Rectangle::with_size(Size::new(300.0, 45.0));
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, viewport.size()),
    );
    for _ in 0..4 {
        element.as_widget_mut().update(
            &mut tree,
            &key(keyboard::key::Code::Tab, keyboard::key::Named::Tab, false),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut Vec::new()),
            &viewport,
        );
    }
    let mut scroll = ScrollOffset::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut scroll);
    assert!(
        scroll.0 > 0.0,
        "keyboard focus must reveal the fourth offscreen control"
    );
    let mut messages = Vec::new();
    element.as_widget_mut().update(
        &mut tree,
        &key(
            keyboard::key::Code::Enter,
            keyboard::key::Named::Enter,
            false,
        ),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut iced::advanced::Shell::new(&mut messages),
        &viewport,
    );
    assert!(matches!(
        messages.as_slice(),
        [Message::Page(Page::Settings)]
    ));
}

#[test]
fn modal_backdrop_is_visible_but_excluded_from_focus_and_activation() {
    use crate::widget::{focus_button::button, focus_navigation::navigate, inert};
    let backdrop = inert::backdrop(button("Read").on_press(Message::Read).into());
    let dialog = button("Keep editing").on_press(Message::KeepEditing);
    let mut element = navigate(iced::widget::stack![backdrop, dialog].into(), true, false);
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let mut tree = Tree::new(&element);
    let viewport = Rectangle::with_size(Size::new(400.0, 100.0));
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, viewport.size()),
    );
    let mut focused = Focused::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut focused);
    assert_eq!(focused.total, 1);
    let mut messages = Vec::new();
    for event in [
        key(keyboard::key::Code::Tab, keyboard::key::Named::Tab, false),
        key(
            keyboard::key::Code::Enter,
            keyboard::key::Named::Enter,
            false,
        ),
    ] {
        element.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &viewport,
        );
    }
    assert!(matches!(messages.as_slice(), [Message::KeepEditing]));
}

#[test]
fn tab_moves_from_text_input_to_action_button() {
    use crate::widget::{focus_button::button, focus_navigation::navigate};
    let mut element = navigate(
        iced::widget::column![
            iced::widget::text_input("Macro name", "").on_input(|_| Message::Read),
            button("Save").on_press(Message::Save),
        ]
        .into(),
        true,
        false,
    );
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let mut tree = Tree::new(&element);
    let viewport = Rectangle::with_size(Size::new(400.0, 100.0));
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, viewport.size()),
    );
    for _ in 0..2 {
        element.as_widget_mut().update(
            &mut tree,
            &key(keyboard::key::Code::Tab, keyboard::key::Named::Tab, false),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut Vec::new()),
            &viewport,
        );
    }
    let mut messages = Vec::new();
    element.as_widget_mut().update(
        &mut tree,
        &key(
            keyboard::key::Code::Enter,
            keyboard::key::Named::Enter,
            false,
        ),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut iced::advanced::Shell::new(&mut messages),
        &viewport,
    );
    assert!(matches!(messages.as_slice(), [Message::Save]));
}
