//! Real Iced layout and software-rendered frames with an in-memory Nia87 session.
use super::*;
use byakko_core::{
    contract::{CompletionPayload, FeatureResult},
    model::{lighting as light, macros as sequence, picture as rgb},
};
use byakko_devices::nia87::{application, looks::Look};
use iced::{
    Color, Rectangle, Size,
    advanced::{
        Layout, layout, renderer,
        widget::{Id, Operation, Tree},
    },
    mouse,
};
use std::collections::{BTreeMap, HashMap};

fn fixture() -> App {
    let mut session = application::session().unwrap();
    session.connect().unwrap();
    let descriptor = session.descriptor();
    let state = State {
        revision: vec![1],
        bindings: descriptor
            .layers
            .iter()
            .map(|layer| {
                (
                    layer.id.clone(),
                    descriptor
                        .keys
                        .iter()
                        .map(|key| {
                            let action = descriptor
                                .actions
                                .iter()
                                .find(|c| c.label == key.label)
                                .map_or(Action::Disabled, |c| c.action.clone());
                            (key.id.clone(), action)
                        })
                        .collect(),
                )
            })
            .collect(),
    };
    let cmd = session.read().unwrap();
    session.accept(cmd.map(|_| CompletionPayload::Keymap(FeatureResult::Read(Ok(state)))));
    let cmd = session.read_lighting().unwrap();
    let schema = session
        .lighting()
        .unwrap()
        .capabilities()
        .effects
        .iter()
        .find(|e| e.id == "13")
        .unwrap();
    let lighting = light::Snapshot {
        backend_id: "nia87".into(),
        revision: vec![2],
        picture_context: vec![13, 1],
        evidence: light::Evidence::Readback,
        content: light::Content::Editable(light::Setting {
            effect: schema.id.clone(),
            option: Some(schema.options[0].id.clone()),
            brightness: schema.brightness.as_ref().map(|r| *r.start()),
            speed: schema.speed.as_ref().map(|r| *r.start()),
            color: None,
        }),
    };
    session.accept(cmd.map(|_| CompletionPayload::Lighting(FeatureResult::Read(Ok(lighting)))));
    let cmd = session.read_picture().unwrap();
    let colors = session
        .picture()
        .unwrap()
        .capabilities()
        .keys
        .iter()
        .map(|id| (id.clone(), [32, 40, 50]))
        .collect();
    let picture = rgb::Snapshot {
        backend_id: "nia87".into(),
        revision: vec![3],
        context_revision: vec![13, 1],
        evidence: rgb::Evidence::Readback,
        content: rgb::Content::Editable(colors),
    };
    session.accept(cmd.map(|_| CompletionPayload::Picture(FeatureResult::Read(Ok(picture)))));
    let slot = session.macros().unwrap().slot().to_owned();
    let cmd = session.read_macro().unwrap();
    let program = sequence::Program {
        repeat_count: 1,
        events: (0..9)
            .map(|n| sequence::Event {
                action: sequence::Action::Key {
                    usage: 4 + n / 2,
                    pressed: n % 2 == 0,
                },
                delay_ms: 25,
            })
            .collect(),
    };
    let snapshot = sequence::Snapshot {
        backend_id: "nia87".into(),
        slot: slot.clone(),
        revision: vec![4],
        content: sequence::Content::Editable(program),
    };
    session.accept(cmd.map(|_| CompletionPayload::Macro {
        slot,
        result: FeatureResult::Read(Ok(snapshot)),
    }));
    let mut app = App::new(
        session,
        Box::new(|_| panic!("render fixture accessed transport")),
    );
    app.style.theme = appearance::theme(&app.appearance.saved);
    app.notice = "Fixture keyboard".into();
    app.keys.selected = app
        .session
        .descriptor()
        .keys
        .iter()
        .find(|key| key.label == "Caps")
        .map(|key| key.id.clone());
    app
}

#[derive(Default)]
struct Bounds {
    cap_ids: HashMap<Id, String>,
    caps: BTreeMap<String, Rectangle>,
    text: Vec<String>,
    timeline: Option<(Rectangle, Rectangle, f32)>,
}
impl Operation for Bounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }
    fn container(&mut self, id: Option<&Id>, rect: Rectangle) {
        if let Some(key) = id.and_then(|id| self.cap_ids.get(id)) {
            assert!(
                self.caps.insert(key.clone(), rect).is_none(),
                "duplicate key {key}"
            );
        }
    }
    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: iced::Vector,
        _: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        if id == Some(&Id::new("macro-timeline")) {
            self.timeline = Some((bounds, content, translation.y));
        }
    }
    fn text(&mut self, _: Option<&Id>, _: Rectangle, label: &str) {
        self.text.push(label.to_owned());
    }
}

fn frame(app: &App, width: u32, height: u32, name: &str) -> BTreeMap<String, Rectangle> {
    let size = Size::new(width as f32, height as f32);
    let viewport = Rectangle::with_size(size);
    let mut scene = app.view();
    let mut painter = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(14.0));
    let mut tree = Tree::new(&scene);
    let node =
        scene
            .as_widget_mut()
            .layout(&mut tree, &painter, &layout::Limits::new(Size::ZERO, size));
    let layout = Layout::new(&node);
    scene.as_widget_mut().update(
        &mut tree,
        &Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
        layout,
        mouse::Cursor::Unavailable,
        &painter,
        &mut iced::advanced::clipboard::Null,
        &mut iced::advanced::Shell::new(&mut Vec::new()),
        &viewport,
    );
    let mut probe = Bounds {
        cap_ids: app
            .session
            .descriptor()
            .keys
            .iter()
            .filter(|k| k.visible)
            .map(|k| (Id::from(format!("cap-{}", k.id)), k.id.clone()))
            .collect(),
        ..Default::default()
    };
    scene
        .as_widget_mut()
        .operate(&mut tree, layout, &painter, &mut probe);
    assert_eq!(probe.caps.len(), 87, "{width}x{height} {name}");
    for (key, r) in &probe.caps {
        assert!(
            r.width > 0.0
                && r.height > 0.0
                && r.x >= 0.0
                && r.y >= 0.0
                && r.x + r.width <= size.width + 0.1
                && r.y + r.height <= size.height + 0.1,
            "{key} outside {name}: {r:?}"
        );
    }
    assert!(
        !probe
            .text
            .iter()
            .any(|s| s.contains("MDA Lavender") || s.contains("Click Inc OF"))
    );
    if name == "macro-edit"
        && let Some((list, content, _)) = probe.timeline
        && content.height > list.height + 1.0
    {
        scene.as_widget_mut().update(
            &mut tree,
            &Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: -5.0 },
            }),
            layout,
            mouse::Cursor::Available(list.center()),
            &painter,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut Vec::new()),
            &viewport,
        );
        let mut scrolled = Bounds {
            cap_ids: probe.cap_ids.clone(),
            ..Default::default()
        };
        scene
            .as_widget_mut()
            .operate(&mut tree, layout, &painter, &mut scrolled);
        assert!(
            scrolled.timeline.unwrap().2 > 0.0,
            "macro timeline should scroll at {width}x{height}"
        );
        assert_eq!(scrolled.caps, probe.caps, "scroll moved the keyboard");
    }
    if let Some(dir) = std::env::var_os("BYAKKO_WORKSPACE_PNG_DIR") {
        scene.as_widget().draw(
            &tree,
            &mut painter,
            &app.style.theme,
            &renderer::Style {
                text_color: Color::BLACK,
            },
            layout,
            mouse::Cursor::Unavailable,
            &viewport,
        );
        let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
        let mut mask = tiny_skia::Mask::new(width, height).unwrap();
        painter.draw(
            &mut pixmap.as_mut(),
            &mut mask,
            &iced_renderer::graphics::Viewport::with_physical_size(Size::new(width, height), 1.0),
            &[viewport],
            Color::WHITE,
        );
        for p in pixmap.data_mut().as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        pixmap
            .save_png(dir.join(format!("{width}x{height}-{name}.png")))
            .unwrap();
    }
    probe.caps
}

#[test]
fn nia87_stage_stays_fixed_across_workflows() {
    for (w, h) in [(1440, 900), (1040, 676), (960, 540), (800, 600), (390, 844)] {
        let mut app = fixture();
        let base = frame(&app, w, h, "keys");
        let target = app
            .session
            .descriptor()
            .keys
            .iter()
            .find(|k| k.label == "K")
            .unwrap()
            .id
            .clone();
        let _ = app.update(Message::Keys(keymap::Message::Key(target)));
        let _ = app.update(Message::Keys(keymap::Message::Layer("fn".into())));
        assert_eq!(frame(&app, w, h, "fn-key"), base);
        let output = app
            .session
            .descriptor()
            .actions
            .iter()
            .position(|c| c.action == Action::Key(4))
            .unwrap();
        let _ = app.update(Message::Keys(keymap::Message::Assign(output)));
        assert!(app.session.keymap().dirty());
        assert_eq!(frame(&app, w, h, "pending-key"), base);
        app.session.revert().unwrap();
        app.page = Page::Macros;
        assert_eq!(frame(&app, w, h, "library"), base);
        app.macros.open(macros::Screen::Edit);
        assert_eq!(frame(&app, w, h, "macro-edit"), base);
        app.page = Page::Picture;
        let _ = app.update(Message::Picture(picture::Message::Preview(Look::Esdf)));
        assert!(app.picture.preview_look().is_some());
        assert!(!app.session.picture().unwrap().dirty());
        assert_eq!(frame(&app, w, h, "look-preview"), base);
        let _ = app.update(Message::Picture(picture::Message::RevertPreview));
        assert_eq!(frame(&app, w, h, "look-reverted"), base);
        for (page, name) in [
            (Page::Settings, "settings"),
            (Page::Archive, "diagnostics"),
            (Page::Tester, "tester"),
        ] {
            app.page = page;
            assert_eq!(frame(&app, w, h, name), base);
        }
    }
}
