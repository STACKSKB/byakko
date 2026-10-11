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
            "tester-compact-wide",
            1360_u32,
            1000_u32,
            byakko_core::projection::tester::Standard::Ansi,
            byakko_core::projection::tester::Size::Full,
        ),
        (
            "tester-compact-narrow",
            900,
            800,
            byakko_core::projection::tester::Standard::Iso,
            byakko_core::projection::tester::Size::Tkl,
        ),
        (
            "tester-compact-jis",
            1360,
            800,
            byakko_core::projection::tester::Standard::Jis,
            byakko_core::projection::tester::Size::Full,
        ),
        (
            "tester-compact-usb-fixture",
            1360,
            800,
            byakko_core::projection::tester::Standard::Ansi,
            byakko_core::projection::tester::Size::Tkl,
        ),
        (
            "tester-compact-active",
            1360,
            800,
            byakko_core::projection::tester::Standard::Ansi,
            byakko_core::projection::tester::Size::Full,
        ),
    ] {
        app.tester.update(tester_form::Message::Standard(standard));
        app.tester.layout.size = board_size;
        let usb_fixture = name == "tester-compact-usb-fixture";
        let listening = name == "tester-compact-active";
        if !usb_fixture {
            app.tester
                .update(tester_form::Message::Source(tester_form::Source::Window));
        }
        if usb_fixture {
            use byakko_devices::{
                tester_input::InputDevice,
                tester_usb::{Endpoint, Metadata, Speed},
            };
            let device = InputDevice {
                id: r"\\?\HID#VID_3151&PID_4011&MI_00#fixture".into(),
                identity: "fixture-identity".into(),
                label: "Fixture keyboard".into(),
            };
            app.tester
                .update(tester_form::Message::Source(tester_form::Source::Device(
                    device.clone(),
                )));
            app.tester.update(tester_form::Message::Usb {
                request: app.tester.usb_request(),
                device,
                result: Ok(Metadata {
                    source: "Descriptor fixture (no hardware observation)".into(),
                    speed: Speed::High,
                    endpoints: vec![Endpoint {
                        interface: 3,
                        alternate: 0,
                        address: 0x81,
                        b_interval: 4,
                        interval_us: 1_000,
                    }],
                }),
            });
        }
        app.tester.test.pause();
        app.tester.test.reset();
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
        if listening {
            app.tester.test.accept(Input {
                usage: 4,
                kind: InputKind::Release,
                at_us: 9_000,
            });
            app.tester.test.accept(Input {
                usage: 0x44,
                kind: InputKind::Press,
                at_us: 11_000,
            });
        } else {
            app.tester.test.pause();
        }
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
        assert!(
            texts
                .0
                .iter()
                .any(|(text, _)| text.starts_with("Observed "))
        );
        assert!(
            !texts
                .0
                .iter()
                .any(|(text, _)| text.contains("Firmware remapping")
                    || text.starts_with("Selected input:")
                    || text.starts_with("Choose an input source"))
        );
        for (label, bounds) in &texts.0 {
            if label == "Timing"
                || label.starts_with("Event spacing")
                || label.starts_with("USB declared")
                || label.starts_with("Chatter candidates:")
                || label.starts_with("Last ")
            {
                assert!(
                    bounds.y >= 0.0 && bounds.y + bounds.height <= height as f32,
                    "{name}: {label} below viewport: {bounds:?}"
                );
                assert!(
                    bounds.x >= 0.0 && bounds.x + bounds.width <= width as f32,
                    "{name}: {label} outside viewport: {bounds:?}"
                );
            }
        }
        if listening {
            assert!(texts.0.iter().any(|(text, _)| text == "Last F11 · press"));
        }
        if usb_fixture {
            assert!(
                texts
                    .0
                    .iter()
                    .any(|(text, _)| text == "USB declared: 1.000 ms")
            );
            assert!(
                !texts
                    .0
                    .iter()
                    .any(|(text, _)| text.contains("bInterval")
                        || text.contains("Descriptor fixture"))
            );
        }
        let start = texts
            .0
            .iter()
            .find(|(text, _)| text == if listening { "Pause" } else { "Start" })
            .expect("actual tester control label")
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
        assert!(messages.iter().any(|message| {
            if listening {
                matches!(message, Message::Tester(tester_form::Message::Pause))
            } else {
                matches!(message, Message::Tester(tester_form::Message::Start))
            }
        }));
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
        if usb_fixture {
            let details = texts
                .0
                .iter()
                .find(|(text, _)| text == "Details")
                .unwrap()
                .1;
            for event in [
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ] {
                element.as_widget_mut().update(
                    &mut tree,
                    &event,
                    layout,
                    mouse::Cursor::Available(Point::new(details.center_x(), details.center_y())),
                    &renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut iced::advanced::Shell::new(&mut messages),
                    &viewport,
                );
            }
        }
        drop(element);
        if usb_fixture {
            let message = messages
                .into_iter()
                .find(|message| {
                    matches!(
                        message,
                        Message::Tester(tester_form::Message::ToggleDetails)
                    )
                })
                .expect("actual Details button emits its message");
            let _ = app.update(message);
            assert!(app.tester.show_details);
            let mut element = app.view();
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, viewport.size()),
            );
            let mut texts = TextBounds::default();
            element
                .as_widget_mut()
                .operate(&mut tree, Layout::new(&node), &renderer, &mut texts);
            assert!(texts.0.iter().any(
                |(text, _)| text.contains("Descriptor fixture") && text.contains("bInterval 4")
            ));
            assert!(
                texts
                    .0
                    .iter()
                    .any(|(text, _)| text.contains("VID_3151&PID_4011&MI_00#fixture"))
            );
            drop(element);
            let _ = app.update(Message::Tester(tester_form::Message::ToggleDetails));
            assert!(!app.tester.show_details);
        } else {
            let _ = app.update(messages.remove(0));
            assert_eq!(app.tester.active(), !listening);
        }
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
    let viewport = Rectangle::with_size(Size::new(1360.0, 800.0));
    let mut texts = TextBounds::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut texts);
    let label = texts
        .0
        .iter()
        .find(|(label, _)| label == "Layout")
        .unwrap()
        .1;
    let cursor = mouse::Cursor::Available(Point::new(label.center_x(), label.center_y()));
    element.as_widget_mut().update(
        &mut tree,
        &Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(label.center_x(), label.center_y()),
        }),
        Layout::new(&node),
        cursor,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut iced::advanced::Shell::new(&mut Vec::new()),
        &viewport,
    );
    let mut messages = Vec::new();
    assert!(
        element
            .as_widget_mut()
            .overlay(
                &mut tree,
                Layout::new(&node),
                &renderer,
                &viewport,
                iced::Vector::ZERO,
            )
            .is_none(),
        "hovering Layout must not open a popup"
    );
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
        .find(|(label, _)| label == "Layout")
        .unwrap()
        .1;
    let cursor =
        mouse::Cursor::Available(Point::new(label.x + label.width + 20.0, label.center_y()));
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

#[test]
fn first_shift_tab_pauses_and_moves_real_iced_focus_backward() {
    use iced::advanced::widget::{
        Id,
        operation::{self, Focusable},
    };
    struct Focused(Vec<Id>);
    impl Operation for Focused {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
            visit(self);
        }
        fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
            if state.is_focused() {
                self.0.extend(id.cloned());
            }
        }
    }
    let (mut app, _) = app(false);
    let _ = app.update(Message::Page(Page::Tester));
    let _ = app.update(Message::Tester(tester_form::Message::Start));
    let first = Id::new("tester-first");
    let second = Id::new("tester-second");
    let controls = iced::widget::column![
        iced::widget::text_input("First", "")
            .id(first.clone())
            .on_input(|_| Message::Tester(tester_form::Message::Reset)),
        iced::widget::text_input("Second", "")
            .id(second.clone())
            .on_input(|_| Message::Tester(tester_form::Message::Reset)),
    ]
    .into();
    let mut element = crate::widget::tester_capture::capture(
        controls,
        true,
        Message::Tester(tester_form::Message::Pause),
    );
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let mut tree = Tree::new(&element);
    let viewport = Rectangle::with_size(Size::new(600.0, 300.0));
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, viewport.size()),
    );
    element.as_widget_mut().operate(
        &mut tree,
        Layout::new(&node),
        &renderer,
        &mut operation::focusable::focus::<()>(second.clone()),
    );
    let mut focused = Focused(Vec::new());
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut focused);
    assert_eq!(focused.0, vec![second]);
    let mut messages = Vec::new();
    let mut shell = iced::advanced::Shell::new(&mut messages);
    element.as_widget_mut().update(
        &mut tree,
        &key(keyboard::key::Code::Tab, false, keyboard::Modifiers::SHIFT),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut shell,
        &viewport,
    );
    assert!(shell.is_event_captured());
    let mut focused = Focused(Vec::new());
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut focused);
    assert_eq!(focused.0, vec![first]);
    assert_eq!(messages.len(), 1);
    assert!(matches!(
        messages[0],
        Message::Tester(tester_form::Message::Pause)
    ));
    let _ = app.update(messages.remove(0));
    assert!(!app.tester.active());
    assert_eq!(app.tester.test.total_presses(), 0);
}

#[test]
fn jis_choice_and_window_input_preserve_japanese_host_usages() {
    use byakko_core::projection::tester::{Legends, Standard};
    let (mut app, _) = app(false);
    let _ = app.update(Message::Page(Page::Tester));
    let _ = app.update(Message::Tester(tester_form::Message::Standard(
        Standard::Jis,
    )));
    assert_eq!(app.tester.layout.legends, Legends::Japanese);
    let _ = app.update(Message::Tester(tester_form::Message::Start));
    for (code, usage) in [
        (keyboard::key::Code::IntlRo, 0x87),
        (keyboard::key::Code::KanaMode, 0x88),
        (keyboard::key::Code::IntlYen, 0x89),
        (keyboard::key::Code::Convert, 0x8a),
        (keyboard::key::Code::NonConvert, 0x8b),
    ] {
        let _ = app.update(Message::Tester(tester_form::Message::Input(
            key(code, false, keyboard::Modifiers::empty()),
            Instant::now(),
        )));
        assert_eq!(app.tester.test.last_input().unwrap().usage, usage);
    }
    let _ = app.update(Message::Tester(tester_form::Message::Standard(
        Standard::Ansi,
    )));
    assert_eq!(app.tester.layout.legends, Legends::Qwerty);
}

#[test]
fn usb_snapshots_reject_old_duplicate_and_disappeared_source_completions() {
    use byakko_devices::{
        tester_input::InputDevice,
        tester_usb::{Endpoint, Metadata, Speed, Unavailable},
    };
    use tester_form::Source;
    let device = InputDevice {
        id: "fixture-input".into(),
        identity: "fixture-identity".into(),
        label: "Fixture keyboard".into(),
    };
    let metadata = Metadata {
        source: "Fixture descriptor".into(),
        speed: Speed::High,
        endpoints: vec![Endpoint {
            interface: 3,
            alternate: 0,
            address: 0x81,
            b_interval: 4,
            interval_us: 1000,
        }],
    };
    let mut form = tester_form::Form::default();
    form.update(tester_form::Message::Source(Source::Device(device.clone())));
    let old = form.usb_request();
    form.update(tester_form::Message::Source(Source::Window));
    form.update(tester_form::Message::Source(Source::Device(device.clone())));
    let current = form.usb_request();
    form.update(tester_form::Message::Usb {
        request: old,
        device: device.clone(),
        result: Ok(metadata.clone()),
    });
    assert!(form.usb.is_none());
    form.update(tester_form::Message::Usb {
        request: current,
        device: device.clone(),
        result: Ok(metadata.clone()),
    });
    assert_eq!(form.usb, Some(Ok(metadata.clone())));
    form.update(tester_form::Message::Usb {
        request: current,
        device: device.clone(),
        result: Err(Unavailable("Duplicate".into())),
    });
    assert_eq!(form.usb, Some(Ok(metadata.clone())));
    form.update(tester_form::Message::Sources(Ok(Vec::new())));
    assert!(form.usb.as_ref().unwrap().is_err());
    form.update(tester_form::Message::Usb {
        request: current,
        device: device.clone(),
        result: Ok(metadata.clone()),
    });
    assert!(form.usb.as_ref().unwrap().is_err());
    form.update(tester_form::Message::Source(Source::Device(device.clone())));
    let in_flight = form.usb_request();
    form.update(tester_form::Message::Sources(Ok(Vec::new())));
    form.update(tester_form::Message::Usb {
        request: in_flight,
        device,
        result: Ok(metadata),
    });
    assert!(form.usb.as_ref().unwrap().is_err());
}

#[test]
fn usb_completion_retains_snapshot_after_leaving_tester_page() {
    use byakko_devices::{tester_input::InputDevice, tester_usb::Unavailable};
    let (mut app, _) = app(false);
    let device = InputDevice {
        id: "fixture-input".into(),
        identity: "fixture-identity".into(),
        label: "Fixture keyboard".into(),
    };
    app.tester
        .update(tester_form::Message::Source(tester_form::Source::Device(
            device.clone(),
        )));
    let request = app.tester.usb_request();
    assert_ne!(app.page, Page::Tester);
    let _ = app.update(Message::Tester(tester_form::Message::Usb {
        request,
        device,
        result: Err(Unavailable("Fixture source has no USB endpoint".into())),
    }));
    assert!(app.tester.usb.as_ref().unwrap().is_err());
    assert!(!app.tester.active());
}
