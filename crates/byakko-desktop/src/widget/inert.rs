// SPDX-License-Identifier: GPL-3.0-or-later
//! Renders the modal backdrop without exposing its controls while a dialog is open.
use iced::{
    Element, Length, Rectangle, Size, Theme, Vector,
    advanced::{
        Layout, Widget, layout, overlay, renderer,
        widget::{Operation, Tree},
    },
    mouse,
};

pub fn guard<'a, Message: 'a>(
    content: Element<'a, Message>,
    blocked: bool,
) -> Element<'a, Message> {
    Element::new(Inert { content, blocked })
}

struct Inert<'a, Message> {
    content: Element<'a, Message>,
    blocked: bool,
}
impl<Message> Widget<Message, Theme, iced::Renderer> for Inert<'_, Message> {
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
        if !self.blocked {
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                operation,
            );
        }
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut iced::advanced::Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if !self.blocked || matches!(event, iced::Event::Window(_)) {
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                layout,
                if self.blocked {
                    mouse::Cursor::Unavailable
                } else {
                    cursor
                },
                renderer,
                clipboard,
                shell,
                viewport,
            );
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
            if self.blocked {
                mouse::Cursor::Unavailable
            } else {
                cursor
            },
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
        if self.blocked {
            mouse::Interaction::default()
        } else {
            self.content.as_widget().mouse_interaction(
                &tree.children[0],
                layout,
                cursor,
                viewport,
                renderer,
            )
        }
    }
    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        if self.blocked {
            None
        } else {
            self.content.as_widget_mut().overlay(
                &mut tree.children[0],
                layout,
                renderer,
                viewport,
                translation,
            )
        }
    }
}
