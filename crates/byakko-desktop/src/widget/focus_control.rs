// SPDX-License-Identifier: GPL-3.0-or-later
//! Keyboard focus for native controls while their pointer widgets keep drawing and handling clicks.
use iced::{
    Element, Event, Length, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, overlay, renderer,
        widget::{Operation, Tree, operation::focusable::Focusable, tree},
    },
    keyboard, mouse,
};

enum Keys<Message> {
    Select {
        options: Vec<Message>,
        selected: Option<usize>,
    },
    Number {
        previous: Message,
        next: Message,
        first: Message,
        last: Message,
    },
    Toggle(Message),
}

pub fn select<'a, Message: Clone + 'a>(
    inner: impl Into<Element<'a, Message>>,
    options: Vec<Message>,
    selected: Option<usize>,
    enabled: bool,
) -> Element<'a, Message> {
    Element::new(Control {
        inner: inner.into(),
        keys: Keys::Select { options, selected },
        enabled,
    })
}

pub fn number<'a, Message: Clone + 'a>(
    inner: impl Into<Element<'a, Message>>,
    current: u32,
    min: u32,
    max: u32,
    step: u32,
    enabled: bool,
    on_change: impl Fn(u32) -> Message,
) -> Element<'a, Message> {
    let step = step.max(1);
    let previous = current.saturating_sub(step).max(min);
    let next = current.saturating_add(step).min(max);
    Element::new(Control {
        inner: inner.into(),
        keys: Keys::Number {
            previous: on_change(previous),
            next: on_change(next),
            first: on_change(min),
            last: on_change(max),
        },
        enabled,
    })
}

pub fn toggle<'a, Message: Clone + 'a>(
    inner: impl Into<Element<'a, Message>>,
    message: Message,
    enabled: bool,
) -> Element<'a, Message> {
    Element::new(Control {
        inner: inner.into(),
        keys: Keys::Toggle(message),
        enabled,
    })
}

struct Control<'a, Message> {
    inner: Element<'a, Message>,
    keys: Keys<Message>,
    enabled: bool,
}

#[derive(Default)]
struct State {
    focused: bool,
}
impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn focus(&mut self) {
        self.focused = true;
    }
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl<Message: Clone> Keys<Message> {
    fn message(&self, key: keyboard::key::Named) -> Option<Message> {
        use keyboard::key::Named;
        match self {
            Self::Select { options, selected } => {
                let last = options.len().checked_sub(1)?;
                let index = match key {
                    Named::ArrowDown => selected.map_or(0, |index| (index + 1).min(last)),
                    Named::ArrowUp => selected.map_or(last, |index| index.saturating_sub(1)),
                    Named::Home => 0,
                    Named::End => last,
                    _ => return None,
                };
                options.get(index).cloned()
            }
            Self::Number {
                previous,
                next,
                first,
                last,
            } => match key {
                Named::ArrowLeft | Named::ArrowDown => Some(previous.clone()),
                Named::ArrowRight | Named::ArrowUp => Some(next.clone()),
                Named::Home => Some(first.clone()),
                Named::End => Some(last.clone()),
                _ => None,
            },
            Self::Toggle(message) if matches!(key, Named::Enter | Named::Space) => {
                Some(message.clone())
            }
            Self::Toggle(_) => None,
        }
    }
}

impl<'a, Message: Clone + 'a> Widget<Message, Theme, iced::Renderer> for Control<'a, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.inner)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.inner]);
    }
    fn size(&self) -> Size<Length> {
        self.inner.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.inner
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        if self.enabled {
            operation.focusable(None, layout.bounds(), tree.state.downcast_mut::<State>());
        }
        self.inner
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if self.enabled
            && tree.state.downcast_ref::<State>().focused
            && matches!(self.keys, Keys::Select { .. })
            && let Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(key),
                repeat: false,
                ..
            }) = event
            && matches!(
                key,
                keyboard::key::Named::Escape | keyboard::key::Named::Tab
            )
        {
            let menu_open = self
                .inner
                .as_widget_mut()
                .overlay(
                    &mut tree.children[0],
                    layout,
                    renderer,
                    viewport,
                    Vector::ZERO,
                )
                .is_some();
            if menu_open {
                // PickList keeps its open menu in private widget state. Recreate
                // only that child; focus and the application's value remain.
                tree.children[0] = Tree::new(&self.inner);
                shell.invalidate_layout();
                shell.request_redraw();
                if *key == keyboard::key::Named::Escape {
                    shell.capture_event();
                    return;
                }
            }
        }
        if self.enabled
            && cursor.is_over(layout.bounds())
            && matches!(
                event,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            )
        {
            tree.state.downcast_mut::<State>().focus();
            shell.request_redraw();
        }
        if self.enabled
            && tree.state.downcast_ref::<State>().focused
            && let Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(key),
                modifiers,
                repeat,
                ..
            }) = event
            && matches!(
                key,
                keyboard::key::Named::ArrowDown
                    | keyboard::key::Named::ArrowUp
                    | keyboard::key::Named::ArrowLeft
                    | keyboard::key::Named::ArrowRight
                    | keyboard::key::Named::Home
                    | keyboard::key::Named::End
                    | keyboard::key::Named::Enter
                    | keyboard::key::Named::Space
            )
        {
            if !modifiers.control()
                && !modifiers.alt()
                && !modifiers.logo()
                && (!*repeat || !matches!(self.keys, Keys::Toggle(_)))
                && let Some(message) = self.keys.message(*key)
            {
                shell.publish(message);
                shell.capture_event();
            }
            return;
        }
        self.inner.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.inner.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        if self.enabled && tree.state.downcast_ref::<State>().focused {
            use iced::advanced::Renderer as _;
            renderer.fill_quad(
                renderer::Quad {
                    bounds: layout.bounds(),
                    border: iced::Border {
                        color: theme.extended_palette().primary.strong.color,
                        width: 2.0,
                        radius: 4.0.into(),
                    },
                    shadow: iced::Shadow::default(),
                    snap: false,
                },
                iced::Background::Color(iced::Color::TRANSPARENT),
            );
        }
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.inner.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.inner.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{
        Pixels, Point,
        keyboard::key::{Code, Named, Physical},
    };

    #[derive(Default)]
    struct Focused {
        count: usize,
        index: Option<usize>,
    }

    impl Operation for Focused {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }

        fn focusable(
            &mut self,
            _: Option<&iced::advanced::widget::Id>,
            _: Rectangle,
            state: &mut dyn Focusable,
        ) {
            if state.is_focused() {
                self.index = Some(self.count);
            }
            self.count += 1;
        }
    }

    fn key_event(key: Named, code: Code) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key),
            modified_key: keyboard::Key::Named(key),
            physical_key: Physical::Code(code),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        })
    }

    fn send_event(
        element: &mut Element<'_, i32>,
        tree: &mut Tree,
        node: &layout::Node,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        event: Event,
    ) -> (Vec<i32>, bool) {
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            tree,
            &event,
            Layout::new(node),
            mouse::Cursor::Available(Point::new(10.0, 10.0)),
            renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            viewport,
        );
        let relayout = shell.is_layout_invalid();
        (messages, relayout)
    }

    #[test]
    fn real_pick_list_escape_dismisses_and_tab_moves_to_next_control() {
        use crate::widget::{focus_button::button, focus_navigation::navigate};

        let pick = iced::widget::pick_list(vec!["One", "Two"], Some("One"), |value| {
            if value == "One" { 1 } else { 2 }
        });
        let mut element = navigate(
            iced::widget::column![
                select(pick, vec![1, 2], Some(0), true),
                button("Next").on_press(3)
            ]
            .into(),
            true,
            false,
        );
        let renderer = iced::Renderer::new(iced::Font::DEFAULT, Pixels(16.0));
        let mut tree = Tree::new(&element);
        let viewport = Rectangle::with_size(Size::new(400.0, 200.0));
        let mut node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
        let (messages, _) = send_event(
            &mut element,
            &mut tree,
            &node,
            &renderer,
            &viewport,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        );
        assert!(messages.is_empty());
        assert!(
            element
                .as_widget_mut()
                .overlay(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &viewport,
                    Vector::ZERO
                )
                .is_some()
        );
        let (messages, relayout) = send_event(
            &mut element,
            &mut tree,
            &node,
            &renderer,
            &viewport,
            key_event(Named::Escape, Code::Escape),
        );
        assert!(messages.is_empty());
        assert!(relayout);
        node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
        assert!(
            element
                .as_widget_mut()
                .overlay(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &viewport,
                    Vector::ZERO
                )
                .is_none()
        );
        let mut focus = Focused::default();
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut focus);
        assert_eq!((focus.index, focus.count), (Some(0), 2));

        let (messages, relayout) = send_event(
            &mut element,
            &mut tree,
            &node,
            &renderer,
            &viewport,
            key_event(Named::Escape, Code::Escape),
        );
        assert!(messages.is_empty());
        assert!(!relayout, "closed pick list needs no state reset");

        let (messages, _) = send_event(
            &mut element,
            &mut tree,
            &node,
            &renderer,
            &viewport,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        );
        assert!(messages.is_empty());
        assert!(
            element
                .as_widget_mut()
                .overlay(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &viewport,
                    Vector::ZERO
                )
                .is_some()
        );
        let (messages, relayout) = send_event(
            &mut element,
            &mut tree,
            &node,
            &renderer,
            &viewport,
            key_event(Named::Tab, Code::Tab),
        );
        assert!(messages.is_empty());
        assert!(relayout);
        node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
        assert!(
            element
                .as_widget_mut()
                .overlay(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &viewport,
                    Vector::ZERO
                )
                .is_none()
        );
        let mut focus = Focused::default();
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut focus);
        assert_eq!((focus.index, focus.count), (Some(1), 2));
    }

    fn press(
        control: &mut Control<'_, i32>,
        key: Named,
        modifiers: keyboard::Modifiers,
        repeat: bool,
    ) -> Vec<i32> {
        let mut tree = Tree::new(control as &dyn Widget<i32, Theme, iced::Renderer>);
        tree.state.downcast_mut::<State>().focus();
        let node = layout::Node::new(Size::new(100.0, 24.0));
        let renderer = iced::Renderer::new(iced::Font::DEFAULT, Pixels(16.0));
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        control.update(
            &mut tree,
            &Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(key),
                modified_key: keyboard::Key::Named(key),
                physical_key: Physical::Unidentified(keyboard::key::NativeCode::Windows(0)),
                location: keyboard::Location::Standard,
                modifiers,
                text: None,
                repeat,
            }),
            Layout::new(&node),
            mouse::Cursor::Available(Point::new(10.0, 10.0)),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &Rectangle::new(Point::ORIGIN, Size::new(100.0, 24.0)),
        );
        messages
    }

    fn control(keys: Keys<i32>, enabled: bool) -> Control<'static, i32> {
        Control {
            inner: iced::widget::Space::new().width(100).height(24).into(),
            keys,
            enabled,
        }
    }

    #[test]
    fn selects_visible_options_and_honors_modifiers() {
        let mut dropdown = control(
            Keys::Select {
                options: vec![10, 20, 30],
                selected: Some(1),
            },
            true,
        );
        assert_eq!(
            press(
                &mut dropdown,
                Named::ArrowDown,
                keyboard::Modifiers::empty(),
                true
            ),
            vec![30]
        );
        assert_eq!(
            press(
                &mut dropdown,
                Named::Home,
                keyboard::Modifiers::empty(),
                false
            ),
            vec![10]
        );
        assert!(
            press(
                &mut dropdown,
                Named::ArrowDown,
                keyboard::Modifiers::CTRL,
                false
            )
            .is_empty()
        );
        let mut disabled = control(
            Keys::Select {
                options: vec![10, 20],
                selected: Some(0),
            },
            false,
        );
        assert!(
            press(
                &mut disabled,
                Named::ArrowDown,
                keyboard::Modifiers::empty(),
                false
            )
            .is_empty()
        );
    }

    #[test]
    fn numeric_and_checkbox_keys_emit_existing_values() {
        let mut slider = control(
            Keys::Number {
                previous: 1,
                next: 3,
                first: 0,
                last: 9,
            },
            true,
        );
        assert_eq!(
            press(
                &mut slider,
                Named::ArrowRight,
                keyboard::Modifiers::empty(),
                true
            ),
            vec![3]
        );
        assert_eq!(
            press(&mut slider, Named::End, keyboard::Modifiers::empty(), false),
            vec![9]
        );
        let mut checkbox = control(Keys::Toggle(42), true);
        assert_eq!(
            press(
                &mut checkbox,
                Named::Space,
                keyboard::Modifiers::empty(),
                false
            ),
            vec![42]
        );
        assert_eq!(
            press(
                &mut checkbox,
                Named::Enter,
                keyboard::Modifiers::empty(),
                false
            ),
            vec![42]
        );
        assert!(
            press(
                &mut checkbox,
                Named::Space,
                keyboard::Modifiers::empty(),
                true
            )
            .is_empty()
        );
    }
}
