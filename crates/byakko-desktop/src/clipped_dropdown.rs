//! Bound dropdown text painting to the control and its independently laid-out menu.
use iced::{
    Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Overlay, Renderer as _, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{Operation, Tree, tree},
    },
};

pub(crate) fn clipped<'a, Message: 'a>(element: Element<'a, Message>) -> Element<'a, Message> {
    Element::new(Clipped(element))
}

struct Clipped<'a, Message>(Element<'a, Message>);

impl<Message> Widget<Message, Theme, Renderer> for Clipped<'_, Message> {
    fn size(&self) -> Size<Length> {
        self.0.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.0.as_widget().size_hint()
    }

    fn tag(&self) -> tree::Tag {
        self.0.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.0.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.0.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.0.as_widget().diff(tree);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.0.as_widget_mut().layout(tree, renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if let Some(bounds) = layout.bounds().intersection(viewport) {
            renderer.with_layer(bounds, |renderer| {
                self.0
                    .as_widget()
                    .draw(tree, renderer, theme, style, layout, cursor, &bounds);
            });
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.0
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.0.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.0
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        self.0
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
            .map(clip_overlay)
    }
}

fn clip_overlay<'a, Message: 'a>(
    element: overlay::Element<'a, Message, Theme, Renderer>,
) -> overlay::Element<'a, Message, Theme, Renderer> {
    overlay::Element::new(Box::new(ClippedOverlay(element)))
}

struct ClippedOverlay<'a, Message>(overlay::Element<'a, Message, Theme, Renderer>);

impl<Message> Overlay<Message, Theme, Renderer> for ClippedOverlay<'_, Message> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        self.0.as_overlay_mut().layout(renderer, bounds)
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        renderer.with_layer(layout.bounds(), |renderer| {
            self.0
                .as_overlay()
                .draw(renderer, theme, style, layout, cursor);
        });
    }

    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.0.as_overlay_mut().operate(layout, renderer, operation);
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        self.0
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.0
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }

    fn overlay<'a>(
        &'a mut self,
        layout: Layout<'a>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        self.0
            .as_overlay_mut()
            .overlay(layout, renderer)
            .map(clip_overlay)
    }

    fn index(&self) -> f32 {
        self.0.as_overlay().index()
    }
}
