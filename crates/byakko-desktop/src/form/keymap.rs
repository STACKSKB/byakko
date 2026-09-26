//! Only unsubmitted form state lives here. Assignments belong to the core editor.
use super::{catalog, shortcut};
use byakko_core::{
    editor::{Editor, Status, keymap::KeymapRules},
    model::keymap::{Action, Change, Descriptor},
};

#[derive(Clone, Debug)]
pub enum Message {
    Layer(String),
    Key(String),
    Catalog(catalog::Message),
    Shortcut(shortcut::Message),
    Assign(usize),
}
pub enum Intent {
    Edit(Change),
    Scroll(catalog::Scroll),
}

pub struct Form {
    pub(crate) layer: String,
    pub(crate) selected: Option<String>,
    pub(crate) catalog: catalog::Form,
    pub(crate) shortcut: shortcut::Form,
}

impl Form {
    pub fn target(&self) -> Option<(&str, &str)> {
        Some((&self.layer, self.selected.as_deref()?))
    }

    pub fn new(descriptor: &Descriptor) -> Self {
        Self {
            layer: descriptor.layers[0].id.clone(),
            selected: None,
            catalog: catalog::Form::default(),
            shortcut: shortcut::Form::default(),
        }
    }

    pub fn selected_action<'a>(&self, editor: &'a Editor<KeymapRules>) -> Option<&'a Action> {
        editor
            .draft()?
            .get(&self.layer)?
            .get(self.selected.as_deref()?)
    }
    pub fn sync_shortcut(&mut self, editor: &Editor<KeymapRules>) {
        let action = editor
            .draft()
            .and_then(|draft| draft.get(&self.layer))
            .and_then(|bindings| self.selected.as_ref().and_then(|key| bindings.get(key)));
        self.shortcut
            .load(action, editor.rules().descriptor().shortcuts.as_ref());
    }
    pub fn can_assign(&self, editor: &Editor<KeymapRules>) -> bool {
        editor.status() == &Status::Ready
            && self.selected.as_ref().is_some_and(|id| {
                editor
                    .rules()
                    .descriptor()
                    .keys
                    .iter()
                    .any(|key| &key.id == id && key.writable)
            })
    }
    pub fn update(
        &mut self,
        message: Message,
        editor: &Editor<KeymapRules>,
    ) -> Result<Option<Intent>, String> {
        let descriptor = editor.rules().descriptor();
        match message {
            Message::Layer(layer) => {
                self.layer = layer;
                self.sync_shortcut(editor);
            }
            Message::Key(key) => {
                self.selected = Some(key);
                self.sync_shortcut(editor);
            }
            Message::Catalog(message) => {
                if matches!(message, catalog::Message::Capture) && !self.can_assign(editor) {
                    return Err("Select an editable key before capturing an assignment".into());
                }
                match self.catalog.update(message, &descriptor.actions)? {
                    Some(catalog::Intent::Assign(index)) => {
                        return self.update(Message::Assign(index), editor);
                    }
                    Some(catalog::Intent::Scroll(scroll)) => {
                        return Ok(Some(Intent::Scroll(scroll)));
                    }
                    None => {}
                }
            }
            Message::Shortcut(message) => {
                let caps = descriptor
                    .shortcuts
                    .as_ref()
                    .ok_or("Shortcuts are unavailable")?;
                if let Ok(Some(action)) = self.shortcut.update(message, caps) {
                    return self.change(action).map(|change| Some(Intent::Edit(change)));
                }
            }
            Message::Assign(index) => {
                let action = descriptor
                    .actions
                    .get(index)
                    .ok_or("Select an advertised assignment")?
                    .action
                    .clone();
                return self.change(action).map(|change| Some(Intent::Edit(change)));
            }
        }
        Ok(None)
    }
    fn change(&self, action: Action) -> Result<Change, String> {
        Ok(Change {
            layer: self.layer.clone(),
            key: self
                .selected
                .clone()
                .ok_or("Select a key before assigning")?,
            action,
        })
    }
}
