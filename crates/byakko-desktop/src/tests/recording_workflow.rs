use super::*;
use crate::{macro_editor::Message as Macro, recording::Message as Record};
use byakko_core::macros::{Action as MacroAction, Edit};
use iced::{
    Event,
    keyboard::{
        self, Key, Location, Modifiers,
        key::{Code, Physical},
    },
};

fn send(app: &mut Desktop, message: Record) {
    let _ = app.update(Message::Record(message));
}
fn key(code: Code, pressed: bool) -> Event {
    if pressed {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Unidentified,
            modified_key: Key::Unidentified,
            physical_key: Physical::Code(code),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
            text: None,
            repeat: false,
        })
    } else {
        Event::Keyboard(keyboard::Event::KeyReleased {
            key: Key::Unidentified,
            modified_key: Key::Unidentified,
            physical_key: Physical::Code(code),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
        })
    }
}

#[test]
fn focused_recording_excludes_edits_and_io_then_releases_on_focus_loss() {
    let mut app = super::macro_workflow::loaded();
    let prefix = app
        .session
        .macros()
        .unwrap()
        .draft()
        .unwrap()
        .events
        .clone();
    send(&mut app, Record::Start);
    assert!(app.session.recording());
    for (code, pressed, ms) in [
        (Code::KeyA, true, 100),
        (Code::KeyB, true, 130),
        (Code::KeyB, false, 190),
    ] {
        let at = app.clock + Duration::from_millis(ms);
        send(&mut app, Record::Input(key(code, pressed), at));
    }
    assert!(app.session.request_macro_apply().is_err());
    assert!(app.session.request_read().is_err());
    assert!(app.session.select_macro("pointer").is_err());
    assert!(app.session.revert_macro().is_err());
    let _ = app.update(Message::Macro(Macro::Edit(Edit::Clear)));
    let _ = app.update(Message::Page(Page::Keys));
    assert_eq!(app.page, Page::Macros);
    let at = app.clock + Duration::from_millis(250);
    send(
        &mut app,
        Record::Input(Event::Window(window::Event::Unfocused), at),
    );
    assert!(!app.busy());
    let draft = app.session.macros().unwrap().draft().unwrap();
    assert_eq!(&draft.events[..prefix.len()], prefix.as_slice());
    assert_eq!(
        draft.events[prefix.len()..]
            .iter()
            .map(|event| event.delay_ms)
            .collect::<Vec<_>>(),
        vec![30, 60, 60, 50]
    );
    assert_eq!(
        draft.events.last().unwrap().action,
        MacroAction::Key {
            usage: 4,
            pressed: false
        }
    );
    assert!(app.session.macros().unwrap().dirty());
}

#[test]
fn closing_finalizes_recording_before_asking_to_discard_and_no_input_runs_after_stop() {
    let mut app = super::macro_workflow::loaded();
    send(&mut app, Record::Fixed(true));
    send(&mut app, Record::Delay("17".into()));
    send(&mut app, Record::Start);
    let at = app.clock + Duration::from_millis(100);
    send(
        &mut app,
        Record::Input(
            Event::Mouse(iced::mouse::Event::ButtonPressed(
                iced::mouse::Button::Right,
            )),
            at,
        ),
    );
    let _ = app.update(Message::Close);
    assert!(!app.session.recording());
    assert_eq!(app.closing, Closing::ConfirmDiscard);
    let before = app.session.macros().unwrap().draft().cloned();
    let draft = before.as_ref().unwrap();
    assert_eq!(
        draft.events.last().unwrap().action,
        MacroAction::Button {
            button: 2,
            pressed: false
        }
    );
    assert_eq!(draft.events.last().unwrap().delay_ms, 17);
    send(&mut app, Record::Input(key(Code::KeyA, true), at));
    assert_eq!(app.session.macros().unwrap().draft(), before.as_ref());
}

#[test]
fn unsupported_input_stops_recording_with_accepted_held_releases() {
    let mut app = super::macro_workflow::loaded();
    send(&mut app, Record::Start);
    let at = app.clock + Duration::from_millis(10);
    send(&mut app, Record::Input(key(Code::KeyA, true), at));
    // The memory device deliberately supports only keyboard usages 4..=40.
    send(&mut app, Record::Input(key(Code::F1, true), at));
    assert!(!app.busy());
    assert!(
        app.macro_notice
            .as_ref()
            .unwrap()
            .contains("Recording stopped")
    );
    assert_eq!(
        app.session
            .macros()
            .unwrap()
            .draft()
            .unwrap()
            .events
            .last()
            .unwrap()
            .action,
        MacroAction::Key {
            usage: 4,
            pressed: false
        }
    );
}
