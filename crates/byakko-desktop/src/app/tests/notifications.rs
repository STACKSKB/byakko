// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use byakko_core::contract::FeatureResult;
use iced::{
    Point, Rectangle, Size,
    advanced::{
        Layout, layout,
        widget::{Operation, Tree},
    },
    keyboard, mouse,
};

#[derive(Default)]
struct RenderedText(Vec<(String, Rectangle)>);

impl Operation for RenderedText {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn text(&mut self, _: Option<&iced::advanced::widget::Id>, bounds: Rectangle, text: &str) {
        self.0.push((text.to_owned(), bounds));
    }
}

fn rendered_text(app: &App) -> Vec<String> {
    let mut element = app.view();
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let mut tree = Tree::new(&element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1360.0, 900.0)),
    );
    let mut texts = RenderedText::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut texts);
    texts.0.into_iter().map(|(text, _)| text).collect()
}

fn click_text(app: &App, label: &str) -> Vec<Message> {
    let mut element = app.view();
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let mut tree = Tree::new(&element);
    let viewport = Rectangle::with_size(Size::new(1360.0, 900.0));
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, viewport.size()),
    );
    let mut texts = RenderedText::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut texts);
    let bounds = texts
        .0
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("{label:?} absent from rendered view"))
        .1;
    let cursor = mouse::Cursor::Available(Point::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    ));
    let mut messages = Vec::new();
    for event in [
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ] {
        element.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            cursor,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &viewport,
        );
    }
    messages
}

fn escape_key(repeat: bool) -> keyboard::Event {
    keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(keyboard::key::Named::Escape),
        modified_key: keyboard::Key::Named(keyboard::key::Named::Escape),
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Escape),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::empty(),
        text: None,
        repeat,
    }
}

fn rendered_escape_status(app: &App) -> iced::event::Status {
    let mut element = app.view();
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let mut tree = Tree::new(&element);
    let viewport = Rectangle::with_size(Size::new(1360.0, 900.0));
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, viewport.size()),
    );
    let mut messages = Vec::new();
    let mut shell = iced::advanced::Shell::new(&mut messages);
    element.as_widget_mut().update(
        &mut tree,
        &Event::Keyboard(escape_key(false)),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut shell,
        &viewport,
    );
    let status = shell.event_status();
    assert!(
        messages.is_empty(),
        "view must leave Escape to the subscription"
    );
    status
}

#[test]
fn discard_modal_escape_keeps_draft_and_other_screens_have_no_modal_listener() {
    use iced::advanced::subscription::into_recipes;

    let (mut app, _) = app(false);
    let _ = app.update(Message::Read);
    settle(&mut app);
    edit(&mut app);
    let draft = app.session.keymap().draft().cloned();
    let ordinary_listeners = into_recipes(app.subscription()).len();
    assert_eq!(rendered_escape_status(&app), iced::event::Status::Ignored);

    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::ConfirmDiscard);
    assert_eq!(
        into_recipes(app.subscription()).len(),
        ordinary_listeners + 1,
        "only the open modal subscribes to ignored keyboard events"
    );
    assert_eq!(rendered_escape_status(&app), iced::event::Status::Ignored);
    assert!(modal_escape(escape_key(true)).is_none());
    let message = modal_escape(escape_key(false)).unwrap();
    assert!(matches!(message, Message::KeepEditing));
    let _ = app.update(message);
    assert_eq!(app.closing, Closing::Open);
    assert_eq!(app.session.keymap().draft(), draft.as_ref());
    assert_eq!(into_recipes(app.subscription()).len(), ordinary_listeners);
}

#[test]
fn rendered_appearance_controls_preview_cancel_and_apply_without_device_reads() {
    use byakko_devices::storage::appearance::{Mode, Palette};
    let (mut app, reads) = app(false);
    let _ = app.update(Message::Read);
    settle(&mut app);
    edit(&mut app);
    let draft = app.session.keymap().draft().cloned();
    let before = reads.load(Ordering::SeqCst);
    app.tester.update(crate::form::tester::Message::Start);
    let messages = click_text(&app, "Appearance");
    assert_eq!(messages.len(), 1);
    let _ = app.update(messages.into_iter().next().unwrap());
    assert!(app.appearance.open);
    assert!(!app.tester.active());
    let _ = app.update(Message::Appearance(appearance::Message::Mode(Mode::Light)));
    let _ = app.update(Message::Appearance(appearance::Message::Palette(
        Palette::StacksKB,
    )));
    assert_eq!(
        app.style.theme.palette().background,
        iced::Color::from_rgb8(247, 247, 247)
    );
    assert!(
        !rendered_text(&app).iter().any(|label| label == "Revert"),
        "modal backdrop must be inert"
    );
    assert!(appearance_escape(escape_key(true)).is_none());
    let _ = app.update(appearance_escape(escape_key(false)).unwrap());
    assert!(!app.appearance.open);
    assert_eq!(app.style.theme, iced::Theme::Dark);

    let _ = app.update(Message::Appearance(appearance::Message::Open));
    let _ = app.update(Message::Appearance(appearance::Message::Mode(Mode::Light)));
    let _ = app.update(Message::Appearance(appearance::Message::Palette(
        Palette::StacksKB,
    )));
    let messages = click_text(&app, "Apply");
    assert_eq!(messages.len(), 1);
    let _ = app.update(messages.into_iter().next().unwrap());
    assert!(!app.appearance.open);
    assert_eq!(app.appearance.saved.palette, Palette::StacksKB);
    assert_eq!(app.session.keymap().draft(), draft.as_ref());
    assert_eq!(reads.load(Ordering::SeqCst), before);
    assert!(!app.session.busy());
}

#[test]
fn failed_appearance_save_keeps_the_modal_and_draft_when_close_was_requested() {
    use byakko_devices::storage::appearance::Mode;
    let (mut app, _) = app(false);
    let _ = app.update(Message::Appearance(appearance::Message::Open));
    let _ = app.update(Message::Appearance(appearance::Message::Mode(Mode::Light)));
    let target = app.appearance.begin_apply().unwrap();
    let _ = app.update(Message::Close);
    assert_eq!(app.closing, Closing::Waiting);
    let _ = app.update(Message::AppearanceSaved(Err("disk full".into())));
    assert_eq!(app.closing, Closing::Open);
    assert!(app.appearance.open);
    assert_eq!(app.appearance.draft, target);
    assert_eq!(app.appearance.saved.mode, Mode::Dark);
    assert!(
        app.appearance
            .problem
            .as_deref()
            .unwrap()
            .contains("disk full")
    );
}

#[test]
fn closing_appearance_preserves_the_workspace_through_the_existing_widget_cache() {
    use byakko_devices::storage::appearance::{Mode, Palette};
    use iced_renderer::graphics::{Viewport, damage};
    use iced_runtime::user_interface::Cache;
    use iced_tiny_skia::Layer;

    fn frame(
        app: &App,
        cache: Cache,
        renderer: &mut iced::Renderer,
        mask: &mut tiny_skia::Mask,
    ) -> (Cache, tiny_skia::Pixmap) {
        let mut ui = iced_runtime::UserInterface::build(
            app.view(),
            Size::new(1360.0, 800.0),
            cache,
            renderer,
        );
        let mut messages = Vec::new();
        let _ = ui.update(
            &[Event::Window(iced::window::Event::RedrawRequested(
                Instant::now(),
            ))],
            mouse::Cursor::Unavailable,
            renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
        ui.draw(
            renderer,
            &app.style.theme,
            &iced::advanced::renderer::Style {
                text_color: app.style.theme.palette().text,
            },
            mouse::Cursor::Unavailable,
        );
        let mut pixels = tiny_skia::Pixmap::new(1360, 800).unwrap();
        renderer.draw(
            &mut pixels.as_mut(),
            mask,
            &Viewport::with_physical_size(Size::new(1360, 800), 1.0),
            &[Rectangle::with_size(Size::new(1360.0, 800.0))],
            app.style.theme.palette().background,
        );
        (ui.into_cache(), pixels)
    }

    let mut app = App::new(
        byakko_devices::nia87::application::session().unwrap(),
        Box::new(|_| Err("No device effects in this test".into())),
    );
    app.session.connect().unwrap();
    let command = app.session.read().unwrap();
    app.session.accept(
        command.map(|_| CompletionPayload::Keymap(FeatureResult::Read(Err("read failed".into())))),
    );
    let mut renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(16.0));
    let mut mask = tiny_skia::Mask::new(1360, 800).unwrap();
    let (cache, _) = frame(&app, Cache::default(), &mut renderer, &mut mask);
    let _ = app.update(Message::Appearance(appearance::Message::Open));
    let _ = app.update(Message::Appearance(appearance::Message::Mode(Mode::Light)));
    let _ = app.update(Message::Appearance(appearance::Message::Palette(
        Palette::StacksKB,
    )));
    let (cache, mut retained) = frame(&app, cache, &mut renderer, &mut mask);
    let previous = renderer.layers().to_vec();
    let _ = app.update(Message::Appearance(appearance::Message::Apply));
    let (_cache, fresh) = frame(&app, cache, &mut renderer, &mut mask);
    let full = Rectangle::with_size(Size::new(1360.0, 800.0));
    let regions = damage::group(
        damage::diff(
            &previous,
            renderer.layers(),
            |layer| vec![layer.bounds],
            Layer::damage,
        ),
        full,
    );
    renderer.draw(
        &mut retained.as_mut(),
        &mut mask,
        &Viewport::with_physical_size(Size::new(1360, 800), 1.0),
        &regions,
        app.style.theme.palette().background,
    );
    let differences = retained
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(fresh.data().as_chunks::<4>().0.iter())
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 8))
        .count();
    assert_eq!(
        differences, 0,
        "closing the modal must repaint the workspace controls"
    );
}

#[test]
fn failed_keymap_read_has_one_short_rendered_diagnostic() {
    let (mut app, _) = app(false);
    let _ = app.update(Message::Read);
    let mut completion = app
        .link
        .executor()
        .unwrap()
        .receive(Some(Duration::from_secs(2)))
        .unwrap();
    completion.payload = CompletionPayload::Keymap(FeatureResult::Read(Err(
        "Long transport detail from the keyboard read".into(),
    )));
    let _ = app.complete(completion);
    let texts = rendered_text(&app);
    assert_eq!(
        texts
            .iter()
            .filter(|text| text.contains("Read failed"))
            .count(),
        1,
        "{texts:?}"
    );
    assert!(
        texts
            .iter()
            .all(|text| !text.contains("Long transport detail"))
    );
}

#[test]
fn lighting_failure_outweighs_unrelated_success_and_navigation_clears_success() {
    use byakko_core::model::lighting::Edit;
    let (mut app, calls) = feature_app(true);
    let _ = app.update(Message::Page(Page::Lighting));
    let _ = app.update(Message::Lighting(lighting::Message::Edit(
        Edit::Brightness(2),
    )));
    elapsed(&mut app);
    drain(&mut app);
    assert_eq!(*calls.lock().unwrap(), ["apply-lighting"]);
    app.notice = "Key colors applied.".into();
    let texts = rendered_text(&app);
    assert_eq!(
        texts
            .iter()
            .filter(|text| text.contains("Save failed"))
            .count(),
        1,
        "{texts:?}"
    );
    assert!(
        texts
            .iter()
            .all(|text| !text.contains("Key colors applied"))
    );
    assert!(
        texts
            .iter()
            .all(|text| !text.contains("Injected lighting failure"))
    );

    let _ = app.update(Message::Page(Page::Settings));
    let texts = rendered_text(&app);
    assert!(
        texts
            .iter()
            .all(|text| !text.contains("Key colors applied"))
    );
    assert!(texts.iter().all(|text| !text.contains("Save failed")));
}

#[test]
fn dirty_failed_settings_read_keeps_a_local_retry_action() {
    use byakko_core::model::settings::{Edit, Value};
    let (mut app, _) = feature_app(false);
    let _ = app.update(Message::Page(Page::Settings));
    let _ = app.update(Message::Settings(settings::Message::Edit(Edit {
        id: "sleep".into(),
        value: Value::Number(8),
    })));
    let request = app.session.read_settings().unwrap();
    let _ = app.complete(request.map(|_| {
        CompletionPayload::Settings(FeatureResult::Read(Err("Settings read failure".into())))
    }));
    assert!(app.session.settings().unwrap().dirty());
    let texts = rendered_text(&app);
    assert!(
        texts.iter().any(|text| text == "Reload & retry"),
        "{texts:?}"
    );
    assert!(
        texts.iter().all(|text| text != "Read / reconnect"),
        "{texts:?}"
    );
    assert!(matches!(
        click_text(&app, "Reload & retry").as_slice(),
        [Message::Settings(settings::Message::Read)]
    ));
}

#[test]
fn disconnected_and_unknown_macro_library_do_not_claim_stale_success() {
    let mut disconnected = DiscoveryApp::new(false);
    disconnected.observe(Availability::Ready { id: "first".into() });
    disconnected.app.notice = "Assignments saved and read back.".into();
    disconnected.observe(Availability::Missing);
    let texts = rendered_text(&disconnected.app);
    assert_eq!(
        texts
            .iter()
            .filter(|text| text.contains("Keyboard disconnected"))
            .count(),
        1,
        "{texts:?}"
    );
    assert!(texts.iter().all(|text| !text.contains("Assignments saved")));

    let (mut app, _) = macro_app(false);
    let _ = app.update(Message::Page(Page::Macros));
    let texts = rendered_text(&app);
    assert!(
        texts
            .iter()
            .all(|text| !text.starts_with("0 of ") || !text.ends_with(" slots used")),
        "{texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "Library not fully read"),
        "{texts:?}"
    );
    assert!(
        texts
            .iter()
            .all(|text| text != "Read macro" && !text.starts_with("Macro: Read / reconnect")),
        "{texts:?}"
    );
}
