// SPDX-License-Identifier: GPL-3.0-or-later
//! While testing, keyboard keys belong to the board instead of focused controls.
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, overlay, renderer,
    widget::{
        Operation, Tree,
        operation::{self, Outcome},
    },
};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector, mouse};

pub fn capture<'a, Message: Clone + 'a>(
    content: Element<'a, Message>,
    active: bool,
    on_escape: Message,
) -> Element<'a, Message> {
    Element::new(Capture {
        content,
        active,
        on_escape,
    })
}
struct Capture<'a, Message> {
    content: Element<'a, Message>,
    active: bool,
    on_escape: Message,
}
impl<Message: Clone> Widget<Message, Theme, iced::Renderer> for Capture<'_, Message> {
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
        if self.active && matches!(event, Event::Keyboard(_)) {
            if crate::input::tester::pauses(event) {
                shell.publish(self.on_escape.clone());
                // Run Iced's real focus operation on this first chord. Its
                // count pass chains into the pass that moves focus backward.
                let mut operation: Box<dyn Operation> =
                    Box::new(operation::focusable::focus_previous());
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
            }
            // The subscription still observes captured test keys; controls do
            // not activate while listening. Shift+Tab performs navigation here.
            shell.capture_event();
            return;
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
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        // Tester menus pause on open. Passive help tooltips remain usable
        // while listening; keyboard capture still belongs to update above.
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
