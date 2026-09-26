//! One physical key press selects an advertised assignment; capture stays local.
use crate::form::catalog::Message;
use byakko_core::model::macros::Action;
use iced::Event;

pub fn capture(event: Event) -> Option<Message> {
    if matches!(event, Event::Window(iced::window::Event::Unfocused)) {
        return Some(Message::CancelCapture);
    }
    match super::recording::action(&event) {
        Some(Action::Key {
            usage: 0x29,
            pressed: true,
        }) => Some(Message::CancelCapture),
        Some(Action::Key {
            usage,
            pressed: true,
        }) => Some(Message::Captured(usage)),
        _ => None,
    }
}
