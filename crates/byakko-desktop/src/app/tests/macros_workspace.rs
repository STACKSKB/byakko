use super::*;
use iced::{
    Event, Point, Rectangle, Size,
    advanced::{
        Layout, layout,
        widget::{Operation, Tree},
    },
    mouse,
};

#[derive(Default)]
struct MacroText(Vec<(String, Rectangle)>);

impl Operation for MacroText {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn text(&mut self, _: Option<&iced::advanced::widget::Id>, bounds: Rectangle, value: &str) {
        self.0.push((value.to_owned(), bounds));
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
    let mut texts = MacroText::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut texts);
    texts.0.into_iter().map(|(value, _)| value).collect()
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
    let mut texts = MacroText::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut texts);
    let bounds = texts
        .0
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("{label:?} absent from rendered macro view"))
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

#[derive(Debug, Default)]
struct TimelineBounds {
    viewport: Option<Rectangle>,
    content: Option<Rectangle>,
    save: Option<Rectangle>,
}

impl Operation for TimelineBounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn text(&mut self, _: Option<&iced::advanced::widget::Id>, bounds: Rectangle, value: &str) {
        if value == "Save macro" {
            self.save = Some(bounds);
        }
    }
    fn scrollable(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        content: Rectangle,
        _: iced::Vector,
        _: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        if id == Some(&iced::advanced::widget::Id::new("macro-timeline")) {
            self.viewport = Some(bounds);
            self.content = Some(content);
        }
    }
}

#[test]
fn library_edit_and_direct_assign_reuse_the_loaded_macro() {
    let (mut app, reads) = ready_to_record();
    let slot = app.session.macros().unwrap().slot().to_owned();
    let loaded = reads.load(Ordering::SeqCst);
    assert_eq!(app.macros.screen, macros::Screen::Edit);

    let _ = app.update(Message::Macros(macros::Message::Back));
    assert_eq!(app.macros.screen, macros::Screen::Library);
    let _ = app.update(Message::Macros(macros::Message::AssignExisting(
        slot.clone(),
    )));
    assert_eq!(app.macros.screen, macros::Screen::Assign);
    assert_eq!(reads.load(Ordering::SeqCst), loaded);
    let _ = app.update(Message::Macros(macros::Message::Back));
    let _ = app.update(Message::Macros(macros::Message::Select(slot)));
    assert_eq!(app.macros.screen, macros::Screen::Edit);
    assert_eq!(reads.load(Ordering::SeqCst), loaded);
}

#[test]
fn macro_view_exposes_library_editor_timeline_and_assign_actions() {
    let (mut app, _) = ready_to_record();
    let _ = app.update(Message::Page(Page::Macros));
    let slot = app.session.macros().unwrap().slot().to_owned();
    let edit = rendered_text(&app);
    assert!(edit.iter().any(|text| text == "Macro editor"), "{edit:?}");
    assert!(
        edit.iter().any(|text| text.starts_with("Sequence")),
        "{edit:?}"
    );
    assert!(edit.iter().any(|text| text == "Record"), "{edit:?}");
    assert!(edit.iter().any(|text| text == "Save macro"), "{edit:?}");
    assert!(
        click_text(&app, "Add event")
            .iter()
            .any(|message| matches!(message, Message::Macros(macros::Message::NewEvent)))
    );
    assert!(
        click_text(&app, "Library")
            .iter()
            .any(|message| matches!(message, Message::Macros(macros::Message::Back)))
    );

    let _ = app.update(Message::Macros(macros::Message::Back));
    let library = rendered_text(&app);
    assert!(library.iter().any(|text| text == "Macros"), "{library:?}");
    assert!(library.iter().any(|text| text == "Edit"), "{library:?}");
    assert!(library.iter().any(|text| text == "Assign"), "{library:?}");
    assert!(
        click_text(&app, "Edit")
            .iter()
            .any(|message| matches!(message, Message::Macros(macros::Message::Select(_))))
    );
    assert!(
        click_text(&app, "Assign")
            .iter()
            .any(|message| matches!(message, Message::Macros(macros::Message::AssignExisting(_))))
    );

    let _ = app.update(Message::Macros(macros::Message::AssignExisting(slot)));
    let assign = rendered_text(&app);
    assert!(
        assign.iter().any(|text| text == "Assign macro"),
        "{assign:?}"
    );
    assert!(assign.iter().any(|text| text == "Trigger"), "{assign:?}");
}

#[test]
fn library_keeps_a_macro_draft_and_blocks_switching_slots() {
    let (mut app, reads) = ready_to_record();
    let current = app.session.macros().unwrap().slot().to_owned();
    let other = app
        .session
        .macros()
        .unwrap()
        .capabilities()
        .slots
        .iter()
        .find(|slot| slot.id != current)
        .unwrap()
        .id
        .clone();
    let _ = app.update(Message::Macros(macros::Message::Repeat("3".into())));
    assert!(app.session.macros().unwrap().dirty());
    let draft = app.session.macros().unwrap().draft().cloned();
    let loaded = reads.load(Ordering::SeqCst);

    let _ = app.update(Message::Macros(macros::Message::Back));
    assert_eq!(app.macros.screen, macros::Screen::Library);
    let _ = app.update(Message::Macros(macros::Message::Select(other)));
    assert_eq!(app.macros.screen, macros::Screen::Library);
    assert_eq!(app.session.macros().unwrap().slot(), current);
    assert_eq!(app.session.macros().unwrap().draft(), draft.as_ref());
    assert_eq!(reads.load(Ordering::SeqCst), loaded);
}

#[test]
fn empty_sequence_cannot_be_saved_from_the_editor() {
    let (mut app, _) = ready_to_record();
    let _ = app.update(Message::Macros(macros::Message::Clear));
    assert!(
        app.session
            .macros()
            .unwrap()
            .draft()
            .unwrap()
            .events
            .is_empty()
    );
    let _ = app.update(Message::Macros(macros::Message::Save));
    assert!(!app.session.busy());
    assert!(app.notice.contains("Add or record an event"));
    assert!(app.session.macros().unwrap().dirty());
}

#[test]
fn reverting_a_new_empty_macro_returns_to_the_library() {
    let (mut app, _) = macro_app(false);
    let _ = app.update(Message::Read);
    drain(&mut app);
    let _ = app.update(Message::Macros(macros::Message::Add));
    drain(&mut app);
    assert_eq!(app.macros.screen, macros::Screen::Edit);
    assert!(app.session.macros().unwrap().dirty());
    assert!(
        app.session
            .macros()
            .unwrap()
            .draft()
            .unwrap()
            .events
            .is_empty()
    );

    let _ = app.update(Message::Macros(macros::Message::Revert));
    assert_eq!(app.macros.screen, macros::Screen::Library);
    assert!(!app.session.macros().unwrap().dirty());
}

#[test]
fn compact_nine_event_timeline_scrolls_under_visible_save_action() {
    let (mut app, _) = ready_to_record();
    let _ = app.update(Message::Page(Page::Macros));
    for usage in 6..=12 {
        let _ = app.update(Message::Macros(macros::Message::NewEvent));
        let _ = app.update(Message::Macros(macros::Message::Value(usage.to_string())));
        let _ = app.update(Message::Macros(macros::Message::StageEvent));
    }
    assert_eq!(
        app.session.macros().unwrap().draft().unwrap().events.len(),
        9
    );
    let mut element = app.view();
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let mut tree = Tree::new(&element);
    let viewport = Rectangle::with_size(Size::new(960.0, 540.0));
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, viewport.size()),
    );
    let mut observed = TimelineBounds::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut observed);
    let scroll = observed
        .viewport
        .expect("macro timeline is independently scrollable");
    let content = observed
        .content
        .expect("macro timeline contains the event list");
    let save = observed.save.expect("save action remains rendered");
    assert!(
        content.height > scroll.height,
        "long sequence must scroll: {observed:?}"
    );
    assert!(
        scroll.height > 200.0,
        "timeline needs usable height: {observed:?}"
    );
    assert!(
        save.y >= 0.0 && save.y + save.height <= 540.0,
        "save must remain in view: {observed:?}"
    );
}

#[test]
fn recording_view_offers_stop_and_no_save_action() {
    let (mut app, _) = ready_to_record();
    let _ = app.update(Message::Page(Page::Macros));
    let _ = app.update(Message::Record(recording::Message::Start));
    assert!(app.session.recording());
    let stop = click_text(&app, "Stop recording");
    assert!(
        stop.iter()
            .any(|message| matches!(message, Message::Record(recording::Message::Stop)))
    );
    assert!(click_text(&app, "Save macro").is_empty());
}
