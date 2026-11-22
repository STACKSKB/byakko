//! Export offscreen desktop frames using BYAKKO_WORKSPACE_PNG_DIR and
//! cargo test -p byakko-desktop export_screen_matrix -- --ignored
use super::*;
use byakko_core::model::settings as scalar;
use iced_runtime::{UserInterface, user_interface::Cache};

fn snapshot(app: &App, name: &str, dir: &Path, w: u32, h: u32) {
    let mut painter = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(16.0));
    let viewport = iced_renderer::graphics::Viewport::with_physical_size(Size::new(w, h), 1.0);
    let mut ui = UserInterface::build(
        app.view(),
        viewport.logical_size(),
        Cache::default(),
        &mut painter,
    );
    let cursor = mouse::Cursor::Unavailable;
    let _ = ui.update(
        &[Event::Window(iced::window::Event::RedrawRequested(
            Instant::now(),
        ))],
        cursor,
        &mut painter,
        &mut iced::advanced::clipboard::Null,
        &mut Vec::new(),
    );
    ui.draw(
        &mut painter,
        &app.style.theme,
        &renderer::Style {
            text_color: app.style.theme.palette().text,
        },
        cursor,
    );
    let mut pixels = tiny_skia::Pixmap::new(w, h).unwrap();
    painter.draw(
        &mut pixels.as_mut(),
        &mut tiny_skia::Mask::new(w, h).unwrap(),
        &viewport,
        &[Rectangle::with_size(viewport.logical_size())],
        app.style.theme.palette().background,
    );
    for pixel in pixels.data_mut().as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    pixels
        .save_png(dir.join(format!("{w}x{h}-{name}.png")))
        .unwrap();
}

#[test]
#[ignore = "writes visual audit PNGs to BYAKKO_WORKSPACE_PNG_DIR"]
fn export_screen_matrix() {
    let dir = std::path::PathBuf::from(
        std::env::var_os("BYAKKO_WORKSPACE_PNG_DIR").expect("choose an output directory"),
    );
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = fixture();
    app.config.data_directory = Some(dir.clone());
    app.link
        .monitor(Discovery::spawn(|| ready("receiver")).unwrap());
    let fields = app
        .session
        .settings()
        .unwrap()
        .capabilities()
        .fields
        .iter()
        .map(|f| {
            let value = match f.kind {
                scalar::Kind::Toggle => scalar::Value::Toggle(false),
                scalar::Kind::Number { min, .. } => scalar::Value::Number(min),
            };
            (f.id.clone(), value)
        })
        .collect();
    let cmd = app.session.read_settings().unwrap();
    app.session.accept(cmd.map(|_| {
        CompletionPayload::Settings(FeatureResult::Read(Ok(scalar::Snapshot {
            backend_id: "nia87".into(),
            revision: vec![1],
            content: scalar::Content::Editable(fields),
        })))
    }));
    let render = |app: &App, name: &str| {
        for (w, h) in [
            (360, 480),
            (390, 670),
            (576, 420),
            (800, 600),
            (960, 540),
            (1040, 676),
            (1280, 720),
            (1440, 900),
            (1920, 1080),
        ] {
            snapshot(app, name, &dir, w, h);
        }
    };
    for (pane, name) in [
        (keymap::Pane::Remap, "remap"),
        (keymap::Pane::Shortcut, "shortcut"),
        (keymap::Pane::TypingPreset, "preset"),
    ] {
        app.keys.pane = pane;
        render(&app, name);
    }
    app.page = Page::Macros;
    for (screen, name) in [
        (macros::Screen::Library, "library"),
        (macros::Screen::Edit, "macro-edit"),
        (macros::Screen::Assign, "macro-assign"),
    ] {
        app.macros.open(screen);
        render(&app, name);
    }
    app.macros.open(macros::Screen::Edit);
    let _ = app.update(Message::Macros(macros::Message::NewEvent));
    render(&app, "macro-composer");
    let _ = app.update(Message::Lighting(lighting::Message::Mode(
        lighting::Mode::PerKey,
    )));
    render(&app, "per-key");
    let modes = app
        .session
        .lighting()
        .unwrap()
        .capabilities()
        .host_modes
        .clone();
    for mode in modes {
        let _ = app.update(Message::Lighting(lighting::Message::Mode(
            lighting::Mode::Host(mode.id.clone()),
        )));
        render(&app, &format!("host-{}", mode.id));
    }
    let _ = app.update(Message::Lighting(lighting::Message::Mode(
        lighting::Mode::Onboard("5".into()),
    )));
    assert_eq!(app.page, Page::Lighting);
    render(&app, "ripple");
    app.session.revert_lighting().unwrap();
    app.autosave.clear();
    app.notice.clear();
    for (page, name) in [
        (Page::Settings, "settings"),
        (Page::Archive, "diagnostics"),
        (Page::Tester, "tester"),
    ] {
        app.page = page;
        render(&app, name);
    }
    app.menu_open = true;
    render(&app, "gear");
    #[cfg(target_os = "linux")]
    {
        let _ = app.update(Message::ReviewReceiverAccess);
        render(&app, "receiver-permission");
        let _ = app.update(Message::CancelReceiverAccess);
    }
    app.menu_open = false;
    let _ = app.update(Message::Appearance(appearance::Message::Open));
    render(&app, "appearance");
    app.appearance.open = false;
    app.closing = Closing::ConfirmDiscard;
    render(&app, "discard-dialog");
}
