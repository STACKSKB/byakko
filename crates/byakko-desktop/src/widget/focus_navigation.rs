// SPDX-License-Identifier: GPL-3.0-or-later
//! Tab traversal through Iced's focusable controls in visual tree order.
use iced::{
    Element, Event, Length, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, overlay, renderer,
        widget::{
            Id, Operation, Tree,
            operation::{
                self, Outcome,
                focusable::Focusable,
                scrollable::{AbsoluteOffset, Scrollable},
            },
        },
    },
    keyboard, mouse,
};

pub fn navigate<'a, Message: Clone + 'a>(
    content: Element<'a, Message>,
    enabled: bool,
    capture_keys: bool,
) -> Element<'a, Message> {
    Element::new(Navigation {
        content,
        enabled,
        capture_keys,
    })
}

struct Navigation<'a, Message> {
    content: Element<'a, Message>,
    enabled: bool,
    capture_keys: bool,
}

impl<Message: Clone> Widget<Message, Theme, iced::Renderer> for Navigation<'_, Message> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
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
        self.content
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
        if self.capture_keys && matches!(event, Event::Keyboard(_)) {
            shell.capture_event();
            return;
        }
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) {
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                &mut operation::focusable::unfocus::<()>(),
            );
            shell.request_redraw();
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if !self.enabled || shell.is_event_captured() {
            return;
        }
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Tab),
            modifiers,
            repeat: false,
            ..
        }) = event
        {
            let mut operation: Box<dyn Operation> = if modifiers.shift() {
                Box::new(operation::focusable::focus_previous())
            } else {
                Box::new(operation::focusable::focus_next())
            };
            loop {
                self.content.as_widget_mut().operate(
                    &mut tree.children[0],
                    layout,
                    renderer,
                    operation.as_mut(),
                );
                match operation.finish() {
                    Outcome::Chain(next) => operation = next,
                    Outcome::None | Outcome::Some(()) => break,
                }
            }
            let mut reveal = RevealFocused::default();
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                &mut reveal,
            );
            if let Some((index, x, y)) = reveal.target {
                self.content.as_widget_mut().operate(
                    &mut tree.children[0],
                    layout,
                    renderer,
                    &mut ScrollByIndex {
                        index,
                        current: 0,
                        x,
                        y,
                    },
                );
            }
            shell.capture_event();
            shell.request_redraw();
        }
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
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
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
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[derive(Clone, Copy)]
struct ScrollContext {
    index: usize,
    bounds: Rectangle,
    translation: Vector,
}

#[derive(Default)]
struct RevealFocused {
    next: usize,
    pending: Option<ScrollContext>,
    active: Option<ScrollContext>,
    target: Option<(usize, f32, f32)>,
}

impl Operation for RevealFocused {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        let outer = self.active;
        if let Some(context) = self.pending.take() {
            self.active = Some(context);
        }
        visit(self);
        self.active = outer;
    }
    fn scrollable(
        &mut self,
        _: Option<&Id>,
        bounds: Rectangle,
        _: Rectangle,
        translation: Vector,
        _: &mut dyn Scrollable,
    ) {
        self.pending = Some(ScrollContext {
            index: self.next,
            bounds,
            translation,
        });
        self.next += 1;
    }
    fn focusable(&mut self, _: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        let Some(context) = self.active.filter(|_| state.is_focused()) else {
            return;
        };
        let visible_x = bounds.x - context.translation.x;
        let visible_y = bounds.y - context.translation.y;
        let dx = if visible_x < context.bounds.x {
            visible_x - context.bounds.x
        } else {
            (visible_x + bounds.width - (context.bounds.x + context.bounds.width)).max(0.0)
        };
        let dy = if visible_y < context.bounds.y {
            visible_y - context.bounds.y
        } else {
            (visible_y + bounds.height - (context.bounds.y + context.bounds.height)).max(0.0)
        };
        if dx != 0.0 || dy != 0.0 {
            self.target = Some((context.index, dx, dy));
        }
    }
}

struct ScrollByIndex {
    index: usize,
    current: usize,
    x: f32,
    y: f32,
}
impl Operation for ScrollByIndex {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }
    fn scrollable(
        &mut self,
        _: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        _: Vector,
        state: &mut dyn Scrollable,
    ) {
        if self.current == self.index {
            state.scroll_by(
                AbsoluteOffset {
                    x: self.x,
                    y: self.y,
                },
                bounds,
                content_bounds,
            );
        }
        self.current += 1;
    }
}
