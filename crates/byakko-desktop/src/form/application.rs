//! Window-level intents, navigation and close presentation state.
use super::{files, host, keymap, lighting, macros, picture, recording, settings, tester};
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
    Picture(picture::Message),
    Settings(settings::Message),
    Record(recording::Message),
    RecordingInput(Event, Instant),
    Tester(tester::Message),
    Page(Page),
    Read,
    Save,
    Revert,
    Poll(Instant),
    Scan,
    ObserveDevice(Instant),
    DeviceEvent(byakko_devices::notifications::Event),
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
    Tester,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Closing {
    Open,
    Waiting,
    ConfirmDiscard,
}
