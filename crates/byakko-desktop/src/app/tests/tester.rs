// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::form::tester as tester_form;
use byakko_core::model::tester::{Input, InputKind};
use iced::{
    Color, Point, Rectangle, Size,
    advanced::{
        Layout, layout, renderer,
        widget::{Operation, Tree},
    },
    keyboard, mouse,
};

#[derive(Default)]
struct TextBounds(Vec<(String, Rectangle)>);
impl Operation for TextBounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn text(&mut self, _: Option<&iced::advanced::widget::Id>, bounds: Rectangle, text: &str) {
        self.0.push((text.to_owned(), bounds));
    }
}
fn key(code: keyboard::key::Code, repeat: bool, modifiers: keyboard::Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Unidentified,
        modified_key: keyboard::Key::Unidentified,
        physical_key: keyboard::key::Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat,
    })
}

#[test]
fn tester_messages_keep_device_configuration_and_pause_at_boundaries() {
    let (mut app, reads) = app(false);
    let baseline_reads = reads.load(Ordering::SeqCst);
    let before = app.session.keymap().draft().cloned();
    let _ = app.update(Message::Page(Page::Tester));
    assert!(!app.tester.active());
    let _ = app.update(Message::Tester(tester_form::Message::Start));
    let _ = app.update(Message::Tester(tester_form::Message::Input(
        key(
            keyboard::key::Code::KeyA,
            false,
            keyboard::Modifiers::empty(),
        ),
        Instant::now(),
    )));
    assert_eq!(app.tester.test.held().collect::<Vec<_>>(), vec![4]);
    let _ = app.update(Message::Tester(tester_form::Message::Input(
        key(
            keyboard::key::Code::KeyA,
            true,
            keyboard::Modifiers::empty(),
        ),
        Instant::now(),
    )));
    assert_eq!(app.tester.test.total_presses(), 1);
    assert_eq!(app.tester.test.total_repeats(), 1);
    let _ = app.update(Message::Tester(tester_form::Message::Input(
        key(keyboard::key::Code::Tab, false, keyboard::Modifiers::SHIFT),
        Instant::now(),
    )));
    assert!(!app.tester.active());
    assert_eq!(app.tester.test.held().count(), 0);
    let _ = app.update(Message::Tester(tester_form::Message::Start));
    let _ = app.update(Message::Tester(tester_form::Message::Input(
        Event::Window(window::Event::Unfocused),
        Instant::now(),
    )));
    assert!(!app.tester.active());
    let _ = app.update(Message::Tester(tester_form::Message::Start));
    let _ = app.update(Message::Page(Page::Keys));
    assert!(!app.tester.active());
    assert_eq!(app.session.keymap().draft(), before.as_ref());
    assert_eq!(reads.load(Ordering::SeqCst), baseline_reads);
}

#[test]
fn rendered_tester_controls_emit_real_app_messages_and_layouts_fit() {
    let (mut app, _) = app(false);
    let _ = app.update(Message::Page(Page::Tester));
    for (name, width, height, standard, board_size) in [
        (
            "tester-wide",
            1360_u32,
            1200_u32,
            byakko_core::projection::tester::Standard::Ansi,
            byakko_core::projection::tester::Size::Full,
        ),
        (
            "tester-narrow",
            900,
            800,
            byakko_core::projection::tester::Standard::Iso,
            byakko_core::projection::tester::Size::Tkl,
        ),
    ] {
        app.tester.layout.standard = standard;
        app.tester.layout.size = board_size;
        app.tester.test.resume();
        app.tester.test.accept(Input {
            usage: 4,
            kind: InputKind::Press,
            at_us: 1_000,
        });
        app.tester.test.accept(Input {
            usage: 4,
            kind: InputKind::Release,
            at_us: 5_000,
        });
        app.tester.test.accept(Input {
            usage: 4,
            kind: InputKind::Press,
            at_us: 7_000,
        });
        app.tester.test.pause();
        let mut element = app.view();
        let mut renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
        let mut tree = Tree::new(&element);
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(width as f32, height as f32)),
        );
        let layout = Layout::new(&node);
        let viewport = Rectangle::with_size(Size::new(width as f32, height as f32));
        let mut texts = TextBounds::default();
        element
            .as_widget_mut()
            .operate(&mut tree, layout, &renderer, &mut texts);
        let start = texts
            .0
            .iter()
            .find(|(text, _)| text == "Start test")
            .expect("actual Start button label")
            .1;
        assert!(start.x >= 0.0 && start.x + start.width <= width as f32);
        assert!(start.y >= 0.0 && start.y + start.height <= height as f32);
        let mut messages = Vec::new();
        for event in [
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ] {
            let mut shell = iced::advanced::Shell::new(&mut messages);
            element.as_widget_mut().update(
                &mut tree,
                &event,
                layout,
                mouse::Cursor::Available(Point::new(start.center_x(), start.center_y())),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &viewport,
            );
        }
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, Message::Tester(tester_form::Message::Start)))
        );
        element.as_widget_mut().update(
            &mut tree,
            &Event::Window(window::Event::RedrawRequested(Instant::now())),
            layout,
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut Vec::new()),
            &viewport,
        );
        element.as_widget().draw(
            &tree,
            &mut renderer,
            &app.style.theme,
            &renderer::Style {
                text_color: Color::WHITE,
            },
            layout,
            mouse::Cursor::Unavailable,
            &viewport,
        );
        let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
        let mut mask = tiny_skia::Mask::new(width, height).unwrap();
        renderer.draw(
            &mut pixmap.as_mut(),
            &mut mask,
            &iced_renderer::graphics::Viewport::with_physical_size(Size::new(width, height), 1.0),
            &[viewport],
            Color::from_rgb8(32, 34, 38),
        );
        // The software renderer targets the native BGRA surface.
        // PNG expects RGBA, so convert channels after rendering.
        for pixel in pixmap.data_mut().as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tester-ui");
        std::fs::create_dir_all(&path).unwrap();
        pixmap.save_png(path.join(format!("{name}.png"))).unwrap();
        drop(element);
        let _ = app.update(messages.remove(0));
        assert!(app.tester.active());
        app.tester.pause();
    }
}

#[test]
fn active_capture_consumes_keyboard_navigation_without_triggering_controls() {
    let (mut app, _) = app(false);
    let _ = app.update(Message::Page(Page::Tester));
    let _ = app.update(Message::Tester(tester_form::Message::Start));
    let mut element = app.view();
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let mut tree = Tree::new(&element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1360.0, 800.0)),
    );
    let mut messages = Vec::new();
    let mut shell = iced::advanced::Shell::new(&mut messages);
    element.as_widget_mut().update(
        &mut tree,
        &key(
            keyboard::key::Code::Space,
            false,
            keyboard::Modifiers::empty(),
        ),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut shell,
        &Rectangle::with_size(Size::new(1360.0, 800.0)),
    );
    assert!(shell.is_event_captured());
    assert!(messages.is_empty());
}

#[test]
fn opening_a_layout_menu_pauses_capture_and_exposes_the_menu() {
    let (mut app, _) = app(false);
    let _ = app.update(Message::Page(Page::Tester));
    let _ = app.update(Message::Tester(tester_form::Message::Start));
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let viewport = Rectangle::with_size(Size::new(1360.0, 800.0));
    let limits = layout::Limits::new(Size::ZERO, viewport.size());
    let mut element = app.view();
    let mut tree = Tree::new(&element);
    let node = element
        .as_widget_mut()
        .layout(&mut tree, &renderer, &limits);
    let mut texts = TextBounds::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut texts);
    let label = texts
        .0
        .iter()
        .find(|(label, _)| label == "Physical layout")
        .unwrap()
        .1;
    let cursor =
        mouse::Cursor::Available(Point::new(label.x + 15.0, label.y + label.height + 12.0));
    let mut messages = Vec::new();
    element.as_widget_mut().update(
        &mut tree,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Layout::new(&node),
        cursor,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut iced::advanced::Shell::new(&mut messages),
        &viewport,
    );
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::Tester(tester_form::Message::Pause)))
    );
    drop(element);
    let _ = app.update(Message::Tester(tester_form::Message::Pause));
    assert!(!app.tester.active());
    let mut element = app.view();
    tree.diff(element.as_widget());
    let node = element
        .as_widget_mut()
        .layout(&mut tree, &renderer, &limits);
    assert!(
        element
            .as_widget_mut()
            .overlay(
                &mut tree,
                Layout::new(&node),
                &renderer,
                &viewport,
                iced::Vector::ZERO
            )
            .is_some()
    );
}
