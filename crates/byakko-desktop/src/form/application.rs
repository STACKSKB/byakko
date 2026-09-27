//! Window-level intents, navigation and close presentation state.
use super::{files, host, keymap, lighting, macros, picture, recording, settings};
use iced::Event;
use std::time::Instant;
#[derive(Clone, Debug)]
pub enum Message {
    Files(files::Message),
    FileComplete(crate::controller::files::Completion),
    Keys(keymap::Message),
    Macros(macros::Message),
    Lighting(lighting::Message),
    Host(host::Message),
    HostFocusLost,
    Picture(picture::Message),
    Settings(settings::Message),
    Record(recording::Message),
    RecordingInput(Event, Instant),
    Page(Page),
    Read,
    Save,
    Revert,
    Poll(Instant),
    Scan,
    ObserveLighting,
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
    Archive,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Closing {
    Open,
    Waiting,
    ConfirmDiscard,
}
