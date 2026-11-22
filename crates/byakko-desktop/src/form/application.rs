//! Window-level intents, navigation and close presentation state.
use super::{
    appearance, files, host, keymap, lighting, macros, picture, recording, settings, tester,
};
use iced::Event;
use std::time::Instant;
#[derive(Clone, Debug)]
pub enum Message {
    Appearance(appearance::Message),
    AppearanceSaved(Result<(), String>),
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
    ToggleMenu,
    DismissMenu,
    RefreshTransports,
    #[cfg(target_os = "linux")]
    EnableKeyboardAccess,
    #[cfg(target_os = "linux")]
    ReviewReceiverAccess,
    #[cfg(target_os = "linux")]
    CancelReceiverAccess,
    #[cfg(target_os = "linux")]
    AllowReceiverAccess,
    #[cfg(target_os = "linux")]
    KeyboardAccessFinished(Result<String, String>),
    ChooseTransport(String),
    ConfirmTransportSwitch,
    CancelTransportSwitch,
    ObserveDevice(Instant),
    DeviceEvent(byakko_devices::notifications::Event),
    Close,
    Discard,
    KeepEditing,
}

#[cfg(target_os = "linux")]
#[derive(Default, Debug, PartialEq)]
pub enum KeyboardAccess {
    #[default]
    Idle,
    ConfirmReceiver,
    Pending,
    Finished(String),
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
