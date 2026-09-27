//! Local picker interaction; the core editor owns every lighting value.
use crate::widget::color_picker::{Gesture, Interaction};
use byakko_core::model::lighting::Edit;

/// The single lighting selector covers device effects, per-key painting and host modes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Mode {
    PerKey,
    Onboard(String),
    Host(String),
}

#[derive(Clone, Debug)]
pub enum Message {
    Mode(Mode),
    Edit(Edit),
    Picker(Interaction),
    Read,
    Revert,
}

#[derive(Default)]
pub struct Form {
    pub mode: Option<Mode>,
    gesture: Gesture,
}

impl Form {
    pub fn dragging(&self) -> bool {
        self.gesture == Gesture::Dragging
    }

    pub fn update(&mut self, message: Message) -> Option<Edit> {
        match message {
            Message::Edit(edit) => return Some(edit),
            Message::Picker(Interaction::Started) => self.gesture = Gesture::Dragging,
            Message::Picker(Interaction::Finished) => self.gesture = Gesture::Idle,
            Message::Picker(Interaction::Moved)
            | Message::Mode(_)
            | Message::Read
            | Message::Revert => {}
        }
        None
    }
}
