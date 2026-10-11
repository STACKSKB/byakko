// SPDX-License-Identifier: GPL-3.0-or-later
//! Renders the modal backdrop without exposing its controls while a dialog is open.
use iced::{
    Element, Length, Rectangle, Size, Theme,
    advanced::{
        Layout, Widget, layout, renderer,
        widget::{Operation, Tree},
    },
    mouse,
};

pub fn backdrop<'a, Message: 'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    Element::new(Inert { content })
}

struct Inert<'a, Message> {
    content: Element<'a, Message>,
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
    fn operate(&mut self, _: &mut Tree, _: Layout<'_>, _: &iced::Renderer, _: &mut dyn Operation) {}
    fn update(
        &mut self,
        _: &mut Tree,
        _: &iced::Event,
        _: Layout<'_>,
        _: mouse::Cursor,
        _: &iced::Renderer,
        _: &mut dyn iced::advanced::Clipboard,
        _: &mut iced::advanced::Shell<'_, Message>,
        _: &Rectangle,
    ) {
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
        _: &Tree,
        _: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
        _: &iced::Renderer,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }
}
