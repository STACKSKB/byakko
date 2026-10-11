use crate::Primitive;
use crate::core::renderer::Quad;
use crate::core::{self, Background, Color, Point, Rectangle, Svg, Transformation};
use crate::graphics::damage;
use crate::graphics::layer;
use crate::graphics::text::{Editor, Paragraph, Text};
use crate::graphics::{self, Image};

use std::sync::Arc;

pub type Stack = layer::Stack<Layer>;

#[derive(Debug, Clone)]
pub struct Layer {
    pub bounds: Rectangle,
    pub quads: Vec<(Quad, Background)>,
    pub primitives: Vec<Item<Primitive>>,
    pub images: Vec<Image>,
    pub text: Vec<Item<Text>>,
}

impl Layer {
    pub fn draw_quad(
        &mut self,
        mut quad: Quad,
        background: Background,
        transformation: Transformation,
    ) {
        quad.bounds = quad.bounds * transformation;
        self.quads.push((quad, background));
    }

    pub fn draw_paragraph(
        &mut self,
        paragraph: &Paragraph,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let paragraph = Text::Paragraph {
            paragraph: paragraph.downgrade(),
            position,
            color,
            clip_bounds,
            transformation,
        };

        self.text.push(Item::Live(paragraph));
    }

    pub fn draw_editor(
        &mut self,
        editor: &Editor,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let editor = Text::Editor {
            editor: editor.downgrade(),
            position,
            color,
            clip_bounds,
            transformation,
        };

        self.text.push(Item::Live(editor));
    }

    pub fn draw_text(
        &mut self,
        text: core::Text,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let text = Text::Cached {
            content: text.content,
            bounds: Rectangle::new(position, text.bounds) * transformation,
            color,
            size: text.size * transformation.scale_factor(),
            line_height: text.line_height.to_absolute(text.size) * transformation.scale_factor(),
            font: text.font,
            align_x: text.align_x,
            align_y: text.align_y,
            shaping: text.shaping,
            clip_bounds: clip_bounds * transformation,
        };

        self.text.push(Item::Live(text));
    }

    pub fn draw_text_raw(&mut self, raw: graphics::text::Raw, transformation: Transformation) {
        let raw = Text::Raw {
            raw,
            transformation,
        };

        self.text.push(Item::Live(raw));
    }

    pub fn draw_text_group(
        &mut self,
        text: Vec<Text>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        self.text
            .push(Item::Group(text, clip_bounds, transformation));
    }

    pub fn draw_text_cache(
        &mut self,
        text: Arc<[Text]>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        self.text
            .push(Item::Cached(text, clip_bounds, transformation));
    }

    pub fn draw_image(&mut self, image: Image, transformation: Transformation) {
        match image {
            Image::Raster {
                image,
                bounds,
                clip_bounds,
            } => {
                self.draw_raster(image, bounds, clip_bounds, transformation);
            }
            Image::Vector {
                svg,
                bounds,
                clip_bounds,
            } => {
                self.draw_svg(svg, bounds, clip_bounds, transformation);
            }
        }
    }

    pub fn draw_raster(
        &mut self,
        image: core::Image,
        bounds: Rectangle,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let image = Image::Raster {
            image: core::Image {
                border_radius: image.border_radius * transformation.scale_factor(),
                ..image
            },
            bounds: bounds * transformation,
            clip_bounds: clip_bounds * transformation,
        };

        self.images.push(image);
    }

    pub fn draw_svg(
        &mut self,
        svg: Svg,
        bounds: Rectangle,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let svg = Image::Vector {
            svg,
            bounds: bounds * transformation,
            clip_bounds: clip_bounds * transformation,
        };

        self.images.push(svg);
    }

    pub fn draw_primitive_group(
        &mut self,
        primitives: Vec<Primitive>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        self.primitives.push(Item::Group(
            primitives,
            clip_bounds * transformation,
            transformation,
        ));
    }

    pub fn draw_primitive_cache(
        &mut self,
        primitives: Arc<[Primitive]>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        self.primitives.push(Item::Cached(
            primitives,
            clip_bounds * transformation,
            transformation,
        ));
    }

    pub fn damage(previous: &Self, current: &Self) -> Vec<Rectangle> {
        if previous.bounds != current.bounds {
            // Antialiased fills and borders can touch the pixel just outside
            // a layer's geometric clip. Include that fringe when a scroll
            // clip moves, just as changed quads below already do.
            return vec![previous.bounds.expand(1.0), current.bounds.expand(1.0)];
        }

        let mut damage = damage::list(
            &previous.quads,
            &current.quads,
            |(quad, _)| {
                quad.bounds
                    .expand(1.0)
                    .intersection(&current.bounds.expand(1.0))
                    .into_iter()
                    .collect()
            },
            |(quad_a, background_a), (quad_b, background_b)| {
                quad_a == quad_b && background_a == background_b
            },
        );

        let text = damage::diff(
            &previous.text,
            &current.text,
            |item| {
                item.as_slice()
                    .iter()
                    .filter_map(text_damage_bounds)
                    .map(|bounds| bounds * item.transformation())
                    .collect()
            },
            |text_a, text_b| {
                damage::list(
                    text_a.as_slice(),
                    text_b.as_slice(),
                    |text| {
                        text_damage_bounds(text)
                            .into_iter()
                            .map(|bounds| bounds * text_a.transformation())
                            .collect()
                    },
                    |text_a, text_b| text_a == text_b,
                )
            },
        );

        let primitives = damage::list(
            &previous.primitives,
            &current.primitives,
            |item| match item {
                Item::Live(primitive) => vec![primitive.visible_bounds()],
                Item::Group(primitives, group_bounds, transformation) => primitives
                    .as_slice()
                    .iter()
                    .map(Primitive::visible_bounds)
                    .map(|bounds| bounds * *transformation)
                    .filter_map(|bounds| bounds.intersection(group_bounds))
                    .collect(),
                Item::Cached(_primitives, bounds, _transformation) => {
                    vec![*bounds]
                }
            },
            |primitive_a, primitive_b| match (primitive_a, primitive_b) {
                (
                    Item::Cached(cache_a, bounds_a, transformation_a),
                    Item::Cached(cache_b, bounds_b, transformation_b),
                ) => {
                    Arc::ptr_eq(cache_a, cache_b)
                        && bounds_a == bounds_b
                        && transformation_a == transformation_b
                }
                _ => false,
            },
        );

        let images = damage::list(
            &previous.images,
            &current.images,
            |image| vec![image.bounds().expand(1.0)],
            Image::eq,
        );

        damage.extend(text);
        damage.extend(primitives);
        damage.extend(images);
        damage
    }
}

fn text_damage_bounds(text: &Text) -> Option<Rectangle> {
    match text {
        Text::Cached {
            bounds,
            align_x,
            align_y,
            clip_bounds,
            ..
        } => {
            // Cached text positions are alignment anchors. In particular, a
            // pick list anchors its label at the vertical center of the field.
            // Treating that anchor as the top-left leaves old glyphs above it.
            let mut bounds = *bounds;
            bounds.x -= match align_x {
                core::text::Alignment::Center => bounds.width / 2.0,
                core::text::Alignment::Right => bounds.width,
                _ => 0.0,
            };
            bounds.y -= match align_y {
                core::alignment::Vertical::Center => bounds.height / 2.0,
                core::alignment::Vertical::Bottom => bounds.height,
                core::alignment::Vertical::Top => 0.0,
            };
            bounds.intersection(clip_bounds)
        }
        _ => text.visible_bounds(),
    }
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            bounds: Rectangle::INFINITE,
            quads: Vec::new(),
            primitives: Vec::new(),
            text: Vec::new(),
            images: Vec::new(),
        }
    }
}

impl graphics::Layer for Layer {
    fn with_bounds(bounds: Rectangle) -> Self {
        Self {
            bounds,
            ..Self::default()
        }
    }

    fn bounds(&self) -> Rectangle {
        self.bounds
    }

    fn flush(&mut self) {}

    fn resize(&mut self, bounds: Rectangle) {
        self.bounds = bounds;
    }

    fn reset(&mut self) {
        self.bounds = Rectangle::INFINITE;

        self.quads.clear();
        self.primitives.clear();
        self.text.clear();
        self.images.clear();
    }

    fn start(&self) -> usize {
        if !self.quads.is_empty() {
            return 1;
        }

        if !self.primitives.is_empty() {
            return 2;
        }

        if !self.images.is_empty() {
            return 3;
        }

        if !self.text.is_empty() {
            return 4;
        }

        usize::MAX
    }

    fn end(&self) -> usize {
        if !self.text.is_empty() {
            return 4;
        }

        if !self.images.is_empty() {
            return 3;
        }

        if !self.primitives.is_empty() {
            return 2;
        }

        if !self.quads.is_empty() {
            return 1;
        }

        0
    }

    fn merge(&mut self, layer: &mut Self) {
        self.quads.append(&mut layer.quads);
        self.primitives.append(&mut layer.primitives);
        self.text.append(&mut layer.text);
        self.images.append(&mut layer.images);
    }
}

#[derive(Debug, Clone)]
pub enum Item<T> {
    Live(T),
    Group(Vec<T>, Rectangle, Transformation),
    Cached(Arc<[T]>, Rectangle, Transformation),
}

impl<T> Item<T> {
    pub fn transformation(&self) -> Transformation {
        match self {
            Item::Live(_) => Transformation::IDENTITY,
            Item::Group(_, _, transformation) | Item::Cached(_, _, transformation) => {
                *transformation
            }
        }
    }

    pub fn clip_bounds(&self) -> Rectangle {
        match self {
            Item::Live(_) => Rectangle::INFINITE,
            Item::Group(_, clip_bounds, _) | Item::Cached(_, clip_bounds, _) => *clip_bounds,
        }
    }

    pub fn as_slice(&self) -> &[T] {
        match self {
            Item::Live(item) => std::slice::from_ref(item),
            Item::Group(group, _, _) => group.as_slice(),
            Item::Cached(cache, _, _) => cache,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::text::Renderer as _;
    use crate::core::{Font, Pixels, Size, alignment, text};
    use crate::graphics::Viewport;

    fn label(content: &str) -> crate::Renderer {
        let mut renderer = crate::Renderer::new(Font::DEFAULT, Pixels(16.0));
        renderer.fill_text(
            core::Text {
                content: content.into(),
                bounds: Size::new(220.0, 20.8),
                size: Pixels(16.0),
                line_height: text::LineHeight::Absolute(Pixels(20.8)),
                font: Font::DEFAULT,
                align_x: text::Alignment::Default,
                align_y: alignment::Vertical::Center,
                shaping: text::Shaping::Basic,
                wrapping: text::Wrapping::None,
            },
            Point::new(10.0, 30.0),
            Color::WHITE,
            Rectangle::with_size(Size::new(260.0, 60.0)),
        );
        renderer
    }

    #[test]
    fn changing_centered_picker_label_matches_a_fresh_frame() {
        // A recognized lighting mode becomes opaque, so its name is replaced
        // by the picker placeholder. Exercise incremental window rendering,
        // which a fresh headless screenshot alone cannot verify.
        for scale in [1.0, 1.25, 2.0] {
            for (before, after) in [
                ("Breathing", "Select lighting mode"),
                ("Select lighting mode", "Rainbow wave"),
                ("Rainbow wave", "Breathing"),
            ] {
                let size = Size::new((260.0 * scale) as u32, (60.0 * scale) as u32);
                let viewport = Viewport::with_physical_size(size, scale);
                let full = Rectangle::with_size(Size::new(260.0, 60.0));
                let mut pixels = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
                let mut mask = tiny_skia::Mask::new(size.width, size.height).unwrap();
                let mut previous = label(before);
                previous.draw(
                    &mut pixels.as_mut(),
                    &mut mask,
                    &viewport,
                    &[full],
                    Color::BLACK,
                );
                let mut current = label(after);
                let damage = Layer::damage(&previous.layers()[0], &current.layers()[0]);
                let damage = graphics::damage::group(damage, full);
                current.draw(
                    &mut pixels.as_mut(),
                    &mut mask,
                    &viewport,
                    &damage,
                    Color::BLACK,
                );
                let mut fresh = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
                current.draw(
                    &mut fresh.as_mut(),
                    &mut mask,
                    &viewport,
                    &[full],
                    Color::BLACK,
                );
                assert!(
                    pixels.data() == fresh.data(),
                    "stale label: {before} -> {after} at {scale}x"
                );
            }
        }
    }

    fn bordered_clip(x: f32, y: f32) -> crate::Renderer {
        use crate::core::Renderer as _;

        let mut renderer = crate::Renderer::new(Font::DEFAULT, Pixels(16.0));
        let bounds = Rectangle {
            x,
            y,
            width: 44.0,
            height: 44.0,
        };
        renderer.start_layer(bounds);
        renderer.fill_quad(
            core::renderer::Quad {
                bounds,
                border: core::Border {
                    color: Color::from_rgb8(102, 104, 109),
                    width: 1.0,
                    radius: 2.0.into(),
                },
                ..Default::default()
            },
            Color::from_rgb8(136, 138, 144),
        );
        renderer.end_layer();
        renderer
    }

    fn empty_clip(x: f32, y: f32) -> crate::Renderer {
        use crate::core::Renderer as _;

        let mut renderer = crate::Renderer::new(Font::DEFAULT, Pixels(16.0));
        renderer.start_layer(Rectangle {
            x,
            y,
            width: 44.0,
            height: 44.0,
        });
        renderer.end_layer();
        renderer
    }

    #[test]
    fn moving_bordered_scroll_clip_matches_a_fresh_frame() {
        // Switching between the tester and configurator moves bordered keys
        // between scroll clips. Compare the incremental window buffer with a
        // fresh frame; antialiasing can paint a pixel outside a clip boundary.
        for scale in [1.0, 1.25] {
            for (before, after) in [((40.5, 70.5), (70.5, 30.5)), ((70.5, 30.5), (40.5, 70.5))] {
                let logical_size = Size::new(180.0, 160.0);
                let size = Size::new(
                    (logical_size.width * scale) as u32,
                    (logical_size.height * scale) as u32,
                );
                let viewport = Viewport::with_physical_size(size, scale);
                let full = Rectangle::with_size(logical_size);
                let mut pixels = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
                let mut mask = tiny_skia::Mask::new(size.width, size.height).unwrap();
                let mut previous = bordered_clip(before.0, before.1);
                previous.draw(
                    &mut pixels.as_mut(),
                    &mut mask,
                    &viewport,
                    &[full],
                    Color::BLACK,
                );

                let mut current = bordered_clip(after.0, after.1);
                let damage = graphics::damage::diff(
                    previous.layers(),
                    current.layers(),
                    |layer| vec![layer.bounds],
                    Layer::damage,
                );
                let damage = graphics::damage::group(damage, full);
                current.draw(
                    &mut pixels.as_mut(),
                    &mut mask,
                    &viewport,
                    &damage,
                    Color::BLACK,
                );

                let mut fresh = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
                current.draw(
                    &mut fresh.as_mut(),
                    &mut mask,
                    &viewport,
                    &[full],
                    Color::BLACK,
                );
                let differing_pixels = pixels
                    .data()
                    .chunks_exact(4)
                    .zip(fresh.data().chunks_exact(4))
                    .filter(|(incremental, full)| incremental != full)
                    .count();
                assert_eq!(differing_pixels, 0, "stale bordered key at {scale}x");
            }
        }
    }

    #[test]
    fn removed_border_at_unchanged_clip_matches_a_fresh_frame() {
        for scale in [1.0, 1.25] {
            for offset in [0.1, 0.25, 0.5, 0.75, 0.9] {
                let logical_size = Size::new(180.0, 160.0);
                let size = Size::new(
                    (logical_size.width * scale) as u32,
                    (logical_size.height * scale) as u32,
                );
                let viewport = Viewport::with_physical_size(size, scale);
                let full = Rectangle::with_size(logical_size);
                let mut pixels = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
                let mut mask = tiny_skia::Mask::new(size.width, size.height).unwrap();
                let mut previous = bordered_clip(40.0 + offset, 70.0 + offset);
                previous.draw(
                    &mut pixels.as_mut(),
                    &mut mask,
                    &viewport,
                    &[full],
                    Color::BLACK,
                );

                let mut current = empty_clip(40.0 + offset, 70.0 + offset);
                let damage = graphics::damage::diff(
                    previous.layers(),
                    current.layers(),
                    |layer| vec![layer.bounds],
                    Layer::damage,
                );
                let damage = graphics::damage::group(damage, full);
                current.draw(
                    &mut pixels.as_mut(),
                    &mut mask,
                    &viewport,
                    &damage,
                    Color::BLACK,
                );

                let mut fresh = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
                current.draw(
                    &mut fresh.as_mut(),
                    &mut mask,
                    &viewport,
                    &[full],
                    Color::BLACK,
                );
                let differing_pixels = pixels
                    .data()
                    .chunks_exact(4)
                    .zip(fresh.data().chunks_exact(4))
                    .filter(|(incremental, full)| incremental != full)
                    .count();
                assert_eq!(
                    differing_pixels, 0,
                    "stale removed key at {scale}x offset {offset}"
                );
            }
        }
    }

    #[test]
    fn border_touching_clip_matches_with_partial_damage() {
        for scale in [1.0, 1.25] {
            let logical_size = Size::new(180.0, 160.0);
            let size = Size::new(
                (logical_size.width * scale) as u32,
                (logical_size.height * scale) as u32,
            );
            let viewport = Viewport::with_physical_size(size, scale);
            let full = Rectangle::with_size(logical_size);
            let damage = Rectangle {
                x: 45.0,
                y: 60.0,
                width: 40.0,
                height: 60.0,
            };
            let mut mask = tiny_skia::Mask::new(size.width, size.height).unwrap();
            let mut renderer = bordered_clip(40.6, 70.6);
            let mut incremental = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
            incremental.fill(tiny_skia::Color::BLACK);
            renderer.draw(
                &mut incremental.as_mut(),
                &mut mask,
                &viewport,
                &[damage],
                Color::BLACK,
            );
            let mut fresh = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
            renderer.draw(
                &mut fresh.as_mut(),
                &mut mask,
                &viewport,
                &[full],
                Color::BLACK,
            );
            let min_x = (damage.x * scale) as usize;
            let max_x = ((damage.x + damage.width) * scale) as usize;
            let min_y = (damage.y * scale) as usize;
            let max_y = ((damage.y + damage.height) * scale) as usize;
            let mut differing_pixels = 0;
            for y in min_y..max_y {
                for x in min_x..max_x {
                    let offset = (y * size.width as usize + x) * 4;
                    if incremental.data()[offset..offset + 4] != fresh.data()[offset..offset + 4] {
                        differing_pixels += 1;
                    }
                }
            }
            assert_eq!(differing_pixels, 0, "inconsistent clip at {scale}x");
        }
    }

    #[test]
    fn fractional_damage_replays_complete_pixels() {
        for scale in [1.0, 1.25] {
            let logical_size = Size::new(180.0, 160.0);
            let size = Size::new(
                (logical_size.width * scale) as u32,
                (logical_size.height * scale) as u32,
            );
            let viewport = Viewport::with_physical_size(size, scale);
            let full = Rectangle::with_size(logical_size);
            let damage = Rectangle {
                x: 45.8,
                y: 75.8,
                width: 29.3,
                height: 20.3,
            };
            let mut mask = tiny_skia::Mask::new(size.width, size.height).unwrap();
            let mut renderer = bordered_clip(40.6, 70.6);
            let mut incremental = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
            incremental.fill(tiny_skia::Color::BLACK);
            renderer.draw(
                &mut incremental.as_mut(),
                &mut mask,
                &viewport,
                &[damage],
                Color::BLACK,
            );
            let mut fresh = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
            renderer.draw(
                &mut fresh.as_mut(),
                &mut mask,
                &viewport,
                &[full],
                Color::BLACK,
            );
            let left = (damage.x * scale).floor() as usize;
            let top = (damage.y * scale).floor() as usize;
            let right = ((damage.x + damage.width) * scale).ceil() as usize;
            let bottom = ((damage.y + damage.height) * scale).ceil() as usize;
            let mut differing_pixels = 0;
            for y in top..bottom {
                for x in left..right {
                    let offset = (y * size.width as usize + x) * 4;
                    if incremental.data()[offset..offset + 4] != fresh.data()[offset..offset + 4] {
                        differing_pixels += 1;
                    }
                }
            }
            assert_eq!(differing_pixels, 0, "fractional damage at {scale}x");
        }
    }
}
