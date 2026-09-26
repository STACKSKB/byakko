//! Window-level intents, navigation and close presentation state.
use super::{keymap, lighting, macros, picture, recording, settings};
use iced::Event;
use std::time::Instant;
#[derive(Clone, Debug)]
pub enum Message {
    Keys(keymap::Message),
    Macros(macros::Message),
    Lighting(lighting::Message),
    Picture(picture::Message),
    Settings(settings::Message),
    Record(recording::Message),
    RecordingInput(Event, Instant),
    Page(Page),
    Read,
    Save,
    Revert,
    Poll(Instant),
    Close,
    Discard,
    KeepEditing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Keys,
    Macros,
    Lighting,
    Picture,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Closing {
    Open,
    Waiting,
    ConfirmDiscard,
}
