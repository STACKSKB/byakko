//! Compact native HSV picker. Its only persistent widget state is drag/hue intent.
use crate::{control_widgets, panels::UiStyle};
use iced::{
    Background, Border, Color, Element, Event, Length, Rectangle, Size, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, renderer,
        widget::{Tree, tree},
    },
    gradient, mouse,
    widget::{column, container, row},
};
use std::rc::Rc;

pub(crate) fn view<Message: Clone + 'static>(
    style: &UiStyle,
    rgb: [u8; 3],
    identity: String,
    on_change: Option<impl Fn([u8; 3]) -> Message + 'static>,
) -> Element<'static, Message> {
    let on_change = on_change.map(|f| Rc::new(f) as Rc<dyn Fn([u8; 3]) -> Message>);
    let swatch_change = on_change.clone();
    let picker = Element::new(Picker {
        rgb,
        identity,
        on_change,
        size: Size::new(style.color_picker_size.0, style.color_picker_size.1),
        hue_width: style.color_hue_width,
        gap: style.spacing.s as f32,
    });
    // A small preview, with no repeated key name or hexadecimal readout.
    let preview = container(iced::widget::space())
        .width(style.fields.compact)
        .height(style.color_hue_width)
        .style(move |theme: &Theme| container::Style {
            background: Some(Color::from_rgb8(rgb[0], rgb[1], rgb[2]).into()),
            border: Border {
                width: 1.0,
                color: theme.extended_palette().background.strong.color,
                ..Default::default()
            },
            ..Default::default()
        });
    row![
        picker,
        column![
            preview,
            control_widgets::color_presets(style, rgb, swatch_change.map(|f| move |rgb| f(rgb)))
        ]
        .spacing(style.spacing.s)
        .width(style.fields.compact),
    ]
    .spacing(style.spacing.s)
    .width(Length::Shrink)
    .into()
}

#[derive(Clone, Copy)]
enum Area {
    Color,
    Hue,
}

#[derive(Default)]
struct State {
    identity: Option<String>,
    dragging: Option<Area>,
    preview: Option<[u8; 3]>,
    hue: f32,
}

impl State {
    fn select(&mut self, identity: &str) {
        if self.identity.as_deref() != Some(identity) {
            self.identity = Some(identity.to_owned());
            self.dragging = None;
            self.preview = None;
            self.hue = 0.0;
        }
    }

    fn begin(&mut self, area: Option<Area>) {
        self.dragging = area;
        self.preview = None;
    }

    fn finish(&mut self) -> Option<[u8; 3]> {
        self.dragging.take().and_then(|_| self.preview.take())
    }
}

struct Picker<Message> {
    rgb: [u8; 3],
    identity: String,
    on_change: Option<Rc<dyn Fn([u8; 3]) -> Message>>,
    size: Size,
    hue_width: f32,
    gap: f32,
}

impl<Message> Picker<Message> {
    fn areas(&self, bounds: Rectangle) -> (Rectangle, Rectangle) {
        let color = Rectangle {
            width: (bounds.width - self.gap - self.hue_width).max(1.0),
            ..bounds
        };
        let hue = Rectangle {
            x: color.x + color.width + self.gap,
            width: self.hue_width,
            ..bounds
        };
        (color, hue)
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Picker<Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn diff(&self, tree: &mut Tree) {
        tree.state.downcast_mut::<State>().select(&self.identity);
    }
    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fixed(self.size.width),
            Length::Fixed(self.size.height),
        )
    }
    fn layout(
        &mut self,
        _: &mut Tree,
        _: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.size.width, self.size.height)
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &iced::Renderer,
        _: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        state.select(&self.identity);
        let Some(on_change) = &self.on_change else {
            state.dragging = None;
            state.preview = None;
            return;
        };
        let (color, hue) = self.areas(layout.bounds());
        match event {
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Window(iced::window::Event::Unfocused) => {
                let was_dragging = state.dragging.is_some();
                if let Some(rgb) = state.finish() {
                    shell.publish(on_change(rgb));
                }
                if was_dragging {
                    shell.capture_event();
                    shell.request_redraw();
                }
                return;
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let area = if cursor.is_over(color) {
                    Some(Area::Color)
                } else if cursor.is_over(hue) {
                    Some(Area::Hue)
                } else {
                    None
                };
                state.begin(area);
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging.is_some() => {}
            _ => return,
        }
        let (Some(area), Some(point)) = (state.dragging, cursor.position()) else {
            return;
        };
        let (mut h, mut s, mut v) = to_hsv(state.preview.unwrap_or(self.rgb), state.hue);
        match area {
            Area::Color => {
                s = ((point.x - color.x) / color.width).clamp(0.0, 1.0);
                v = 1.0 - ((point.y - color.y) / color.height).clamp(0.0, 1.0);
            }
            Area::Hue => h = ((point.y - hue.y) / hue.height).clamp(0.0, 1.0),
        }
        state.hue = h;
        state.preview = Some(from_hsv(h, s, v));
        shell.capture_event();
        shell.request_redraw();
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
        use iced::advanced::Renderer;
        let state = tree.state.downcast_ref::<State>();
        let (h, s, v) = to_hsv(state.preview.unwrap_or(self.rgb), state.hue);
        let (color, hue) = self.areas(layout.bounds());
        let pure = from_hsv(h, 1.0, 1.0);
        let saturation = gradient::Linear::new(std::f32::consts::FRAC_PI_2)
            .add_stop(0.0, Color::WHITE)
            .add_stop(1.0, Color::from_rgb8(pure[0], pure[1], pure[2]));
        renderer.fill_quad(
            renderer::Quad {
                bounds: color,
                ..Default::default()
            },
            Background::Gradient(saturation.into()),
        );
        let brightness = gradient::Linear::new(std::f32::consts::PI)
            .add_stop(0.0, Color::TRANSPARENT)
            .add_stop(1.0, Color::BLACK);
        renderer.fill_quad(
            renderer::Quad {
                bounds: color,
                ..Default::default()
            },
            Background::Gradient(brightness.into()),
        );
        let mut spectrum = gradient::Linear::new(std::f32::consts::PI);
        for step in 0..=6 {
            let at = step as f32 / 6.0;
            let rgb = from_hsv(at, 1.0, 1.0);
            spectrum = spectrum.add_stop(at, Color::from_rgb8(rgb[0], rgb[1], rgb[2]));
        }
        renderer.fill_quad(
            renderer::Quad {
                bounds: hue,
                ..Default::default()
            },
            Background::Gradient(spectrum.into()),
        );
        let marker = Rectangle {
            x: color.x + s * color.width - 4.0,
            y: color.y + (1.0 - v) * color.height - 4.0,
            width: 8.0,
            height: 8.0,
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: marker,
                border: Border {
                    width: 2.0,
                    radius: 4.0.into(),
                    color: Color::WHITE,
                },
                ..Default::default()
            },
            Color::TRANSPARENT,
        );
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    y: hue.y + h * hue.height - 2.0,
                    height: 4.0,
                    ..hue
                },
                border: Border {
                    width: 1.0,
                    color: Color::BLACK,
                    ..Default::default()
                },
                ..Default::default()
            },
            Color::WHITE,
        );
    }
    fn mouse_interaction(
        &self,
        _: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Rectangle,
        _: &iced::Renderer,
    ) -> mouse::Interaction {
        if self.on_change.is_some() && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

fn to_hsv(rgb: [u8; 3], hue_if_gray: f32) -> (f32, f32, f32) {
    let [r, g, b] = rgb.map(|channel| f32::from(channel) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if delta == 0.0 {
        return (hue_if_gray, 0.0, max);
    }
    let h = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } / 6.0;
    (h, delta / max, max)
}

fn from_hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let hue = h.rem_euclid(1.0) * 6.0;
    let chroma = v * s;
    let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let rgb = match hue as u8 {
        0 => [chroma, x, 0.0],
        1 => [x, chroma, 0.0],
        2 => [0.0, chroma, x],
        3 => [0.0, x, chroma],
        4 => [x, 0.0, chroma],
        _ => [chroma, 0.0, x],
    };
    rgb.map(|channel| ((channel + v - chroma) * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_publishes_only_on_finish_and_never_twice() {
        let mut state = State::default();
        state.select("first-key");
        state.begin(Some(Area::Color));
        state.preview = Some([10, 20, 30]);
        state.preview = Some([40, 50, 60]);
        // A release outside the widget or loss of window focus uses the same
        // finalization path and does not need a cursor position.
        assert_eq!(state.finish(), Some([40, 50, 60]));
        assert_eq!(state.finish(), None);

        state.begin(Some(Area::Hue));
        state.preview = Some([70, 80, 90]);
        assert_eq!(state.finish(), Some([70, 80, 90]));
        assert_eq!(state.finish(), None);
    }

    #[test]
    fn selecting_another_key_discards_an_active_drag() {
        let mut state = State::default();
        state.select("first-key");
        state.begin(Some(Area::Color));
        state.preview = Some([1, 2, 3]);
        state.hue = 0.6;
        state.select("second-key");
        assert_eq!(state.finish(), None);
        assert_eq!(state.preview, None);
        assert_eq!(state.hue, 0.0);

        state.begin(Some(Area::Color));
        state.preview = Some([4, 5, 6]);
        state.select("second-key");
        assert_eq!(state.finish(), Some([4, 5, 6]));
    }

    #[test]
    fn picker_roundtrips_colors_and_preserves_hue_at_black() {
        for rgb in [
            [255, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
            [0, 0, 0],
            [255, 255, 255],
            [12, 73, 211],
        ] {
            let (h, s, v) = to_hsv(rgb, 0.5);
            assert_eq!(from_hsv(h, s, v), rgb);
        }
        assert_eq!(to_hsv([0, 0, 0], 0.6), (0.6, 0.0, 0.0));
        assert_eq!(from_hsv(1.0, 1.0, 1.0), [255, 0, 0]);
    }
}
