//! Full and retained-buffer rendering of the Keys workspace without a window.
use super::*;
use iced::{Rectangle, Size, mouse};
use iced_renderer::graphics::{Viewport, damage};
use iced_runtime::{UserInterface, user_interface::Cache};
use iced_tiny_skia::Layer;

struct ReadOnlyNia87(State);

impl Device for ReadOnlyNia87 {
    fn read(&mut self) -> Result<State, String> {
        Ok(self.0.clone())
    }

    fn apply(&mut self, _: &State, _: &[Change], _: &Path) -> Result<State, ApplyFailure> {
        panic!("rendering must not apply keyboard changes")
    }
}

fn keys_workspace() -> App {
    let session = byakko_devices::nia87::application::session().unwrap();
    let descriptor = session.descriptor();
    let state = State {
        revision: vec![1],
        bindings: descriptor
            .layers
            .iter()
            .map(|layer| {
                let bindings = descriptor
                    .keys
                    .iter()
                    .map(|key| (key.id.clone(), Action::Disabled))
                    .collect();
                (layer.id.clone(), bindings)
            })
            .collect(),
    };
    let mut app = App::new(
        session,
        Box::new(move |expected| {
            assert_eq!(expected, Some("wired"));
            let worker = Executor::spawn(ReadOnlyNia87(state.clone()), Default::default())
                .map_err(|error| error.to_string())?;
            Ok(("wired".into(), worker))
        }),
    );
    app.link
        .monitor(Discovery::spawn(|| ready("wired")).unwrap());
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.link.selected().is_none() {
        let _ = app.update(Message::Scan);
        assert!(Instant::now() < deadline, "fake device did not attach");
        std::thread::yield_now();
    }
    drain(&mut app);
    assert_eq!(app.session.keymap().status(), &Status::Ready);
    app
}

fn keyboard_case(layers: &[Layer]) -> Rectangle {
    layers
        .iter()
        .flat_map(|layer| &layer.quads)
        .find(|(quad, _)| quad.shadow.color.a > 0.0 && quad.bounds.width > 400.0)
        .expect("keyboard case has a shadow")
        .0
        .bounds
}

#[derive(Debug, Default)]
struct Differences {
    pixels: usize,
    over_eight: usize,
    max_channel: u8,
    bounds: Option<Rectangle>,
}

fn differences(a: &tiny_skia::Pixmap, b: &tiny_skia::Pixmap, bounds: Rectangle) -> Differences {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    let left = bounds.x.floor().max(0.0) as usize;
    let top = bounds.y.floor().max(0.0) as usize;
    let right = ((bounds.x + bounds.width).ceil() as usize).min(a.width() as usize);
    let bottom = ((bounds.y + bounds.height).ceil() as usize).min(a.height() as usize);
    let stride = a.width() as usize;
    let a = a.data().as_chunks::<4>().0;
    let b = b.data().as_chunks::<4>().0;
    let mut result = Differences::default();
    for y in top..bottom {
        for x in left..right {
            let i = y * stride + x;
            let delta = a[i]
                .into_iter()
                .zip(b[i])
                .map(|(x, y)| x.abs_diff(y))
                .max()
                .unwrap();
            if delta > 0 {
                result.pixels += 1;
                result.over_eight += usize::from(delta > 8);
                result.max_channel = result.max_channel.max(delta);
                let pixel = Rectangle::new(iced::Point::new(x as f32, y as f32), Size::UNIT);
                result.bounds = Some(result.bounds.map_or(pixel, |bounds| bounds.union(&pixel)));
            }
        }
    }
    result
}

struct Preview {
    viewport: Viewport,
    renderer: iced::Renderer,
    cache: Cache,
    previous: Vec<Layer>,
    retained: tiny_skia::Pixmap,
    mask: tiny_skia::Mask,
}

impl Preview {
    fn new(size: Size<u32>, scale: f32) -> Self {
        Self {
            viewport: Viewport::with_physical_size(size, scale),
            renderer: iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(16.0)),
            cache: Cache::default(),
            previous: Vec::new(),
            retained: tiny_skia::Pixmap::new(size.width, size.height).unwrap(),
            mask: tiny_skia::Mask::new(size.width, size.height).unwrap(),
        }
    }

    fn frame(
        &mut self,
        app: &App,
        cursor: mouse::Cursor,
        step: &str,
        max_channel_delta: u8,
    ) -> Rectangle {
        let mut ui = UserInterface::build(
            app.view(),
            self.viewport.logical_size(),
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let theme = &app.style.theme;
        let movement = cursor
            .position()
            .map_or(mouse::Event::CursorLeft, |position| {
                mouse::Event::CursorMoved { position }
            });
        let _ = ui.update(
            &[
                Event::Mouse(movement),
                Event::Window(window::Event::RedrawRequested(Instant::now())),
            ],
            cursor,
            &mut self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Vec::new(),
        );
        ui.draw(
            &mut self.renderer,
            theme,
            &iced::advanced::renderer::Style {
                text_color: theme.palette().text,
            },
            cursor,
        );
        self.cache = ui.into_cache();

        let current = self.renderer.layers();
        let case = keyboard_case(current);
        let full = Rectangle::with_size(self.viewport.logical_size());
        let has_previous = !self.previous.is_empty();
        let regions = if has_previous {
            damage::group(
                damage::diff(
                    &self.previous,
                    current,
                    |layer| vec![layer.bounds],
                    Layer::damage,
                ),
                full,
            )
        } else {
            // A resized window has a fresh surface and no retained layer history.
            vec![full]
        };
        assert!(!regions.is_empty(), "{step} must produce damage");
        let previous_case = has_previous.then(|| keyboard_case(&self.previous));
        self.previous = current.to_vec();
        let background = theme.palette().background;
        self.renderer.draw(
            &mut self.retained.as_mut(),
            &mut self.mask,
            &self.viewport,
            &regions,
            background,
        );

        if let Some(previous_case) = previous_case {
            let size = self.viewport.physical_size();
            let scale = self.viewport.scale_factor();
            let mut fresh = tiny_skia::Pixmap::new(size.width, size.height).unwrap();
            self.renderer.draw(
                &mut fresh.as_mut(),
                &mut self.mask,
                &self.viewport,
                &[full],
                background,
            );
            let bounds = (case.union(&previous_case) * scale).expand(16.0);
            let differences = differences(&self.retained, &fresh, bounds);
            eprintln!(
                "{step} at {scale}x, case {:?}, {differences:?}",
                case * scale
            );
            assert!(
                differences.max_channel <= max_channel_delta,
                "{step} at {scale}x differed: {differences:?}"
            );
        }
        case
    }
}

fn select(app: &mut App, label: &str) {
    let id = app
        .session
        .descriptor()
        .keys
        .iter()
        .find(|key| key.label == label)
        .unwrap()
        .id
        .clone();
    let _ = app.update(Message::Keys(keymap::Message::Key(id)));
}

#[test]
fn selecting_a_key_preserves_the_rest_of_the_keyboard_on_partial_redraw() {
    for (size, scale) in [
        (Size::new(1440, 900), 1.0),
        (Size::new(1726, 1021), 1.25),
        (Size::new(1726, 1021), 1.5),
    ] {
        let mut app = keys_workspace();
        let mut preview = Preview::new(size, scale);
        let cursor = mouse::Cursor::Unavailable;
        let case = preview.frame(&app, cursor, "initial frame", 0);
        select(&mut app, "Esc");
        assert_eq!(preview.frame(&app, cursor, "Esc selection", 0), case);
    }
}

#[test]
fn hover_and_preview_motion_keep_retained_keys_in_sync_at_fractional_scale() {
    let mut app = keys_workspace();
    let mut preview = Preview::new(Size::new(1726, 1021), 1.25);
    let case = preview.frame(&app, mouse::Cursor::Unavailable, "initial frame", 0);
    let hover = mouse::Cursor::Available(iced::Point::new(case.x + 26.0, case.y + 26.0));
    assert_eq!(preview.frame(&app, hover, "show key tooltip", 0), case);
    assert_eq!(
        preview.frame(&app, mouse::Cursor::Unavailable, "hide key tooltip", 0),
        case
    );

    let _ = app.update(Message::RefreshTransports);
    assert!(app.link.scan_requested());
    // Moving the stage changes which key edges intersect a damage clip. Keep
    // the tolerance at the observed edge rounding of two byte values.
    let moved = preview.frame(
        &app,
        mouse::Cursor::Unavailable,
        "show connection scan progress",
        2,
    );
    assert_ne!(
        moved, case,
        "the progress label must move the board fixture"
    );

    let mut resized = Preview::new(Size::new(1280, 800), 1.25);
    resized.cache = preview.cache;
    let case = resized.frame(
        &app,
        mouse::Cursor::Unavailable,
        "resize to a new surface",
        0,
    );
    assert_ne!(moved, case);
    select(&mut app, "Esc");
    assert_eq!(
        resized.frame(&app, mouse::Cursor::Unavailable, "select after resize", 0),
        case
    );
}

#[test]
fn inactive_header_labels_draw_light_glyphs_on_black() {
    let mut app = keys_workspace();
    for theme in [iced::Theme::Light, iced::Theme::Dark] {
        app.style.theme = theme;
        let mut preview = Preview::new(Size::new(1360, 900), 1.0);
        preview.frame(&app, mouse::Cursor::Unavailable, "header", 0);
        let texts = super::notifications::rendered_bounds(&app, 1360.0);
        for label in [
            "Macros",
            "Lighting",
            "Settings",
            "Diagnostics",
            "Tester",
            "Appearance",
        ] {
            let rect = texts.0.iter().find(|(name, _)| name == label).unwrap().1;
            let mut light = 0;
            for y in rect.y.ceil() as u32..(rect.y + rect.height).floor() as u32 {
                for x in rect.x.ceil() as u32..(rect.x + rect.width).floor() as u32 {
                    let pixel = preview.retained.pixel(x, y).unwrap();
                    if pixel.red().min(pixel.green()).min(pixel.blue()) > 100 {
                        light += 1;
                    }
                }
            }
            assert!(
                light > 10,
                "{label} rendered {light} light pixels in {rect:?} on {:?}",
                app.style.theme
            );
        }
    }
}
