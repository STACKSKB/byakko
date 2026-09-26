//! Only unsubmitted form state lives here. Assignments belong to the core editor.
use byakko_core::model::keymap::{Change, Descriptor};

#[derive(Clone, Debug)]
pub enum Message {
    Layer(String),
    Key(String),
    Search(String),
    Assign(usize),
}

pub struct Form {
    pub(crate) layer: String,
    pub(crate) selected: Option<String>,
    pub(crate) search: String,
}

impl Form {
    pub fn target(&self) -> Option<(&str, &str)> {
        Some((&self.layer, self.selected.as_deref()?))
    }

    pub fn new(descriptor: &Descriptor) -> Self {
        Self {
            layer: descriptor.layers[0].id.clone(),
            selected: None,
            search: String::new(),
        }
    }

    pub fn update(&mut self, message: Message, descriptor: &Descriptor) -> Option<Change> {
        match message {
            Message::Layer(layer) => self.layer = layer,
            Message::Key(key) => self.selected = Some(key),
            Message::Search(search) => self.search = search,
            Message::Assign(index) => {
                return Some(Change {
                    layer: self.layer.clone(),
                    key: self.selected.clone()?,
                    action: descriptor.actions.get(index)?.action.clone(),
                });
            }
        }
        None
    }
}
