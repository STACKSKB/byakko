//! Local picker interaction; the core editor owns every lighting value.
use crate::widget::color_picker::{Gesture, Interaction};
use byakko_core::model::lighting::Edit;

#[derive(Clone, Debug)]
pub enum Message {
    Edit(Edit),
    Picker(Interaction),
    Read,
    Revert,
    Save,
}

#[derive(Default)]
pub struct Form {
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
            | Message::Read
            | Message::Revert
            | Message::Save => {}
        }
        None
    }
}
