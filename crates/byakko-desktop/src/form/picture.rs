//! Per-key selection is local; colors are edited only in the core draft.
use crate::widget::color_picker::{Gesture, Interaction};
use byakko_core::{
    editor::{Editor, picture::PictureRules},
    model::picture::{Channel, Edit},
};

#[derive(Clone, Debug)]
pub enum Message {
    Select(String),
    Color([u8; 3]),
    Channel(Channel, u8),
    Picker(Interaction),
    Read,
    Revert,
    Save,
}

#[derive(Default)]
pub struct Form {
    pub(crate) selected: Option<String>,
    brush: Option<[u8; 3]>,
    gesture: Gesture,
}

impl Form {
    pub fn dragging(&self) -> bool {
        self.gesture == Gesture::Dragging
    }

    pub fn color(&self, editor: &Editor<PictureRules>) -> Option<[u8; 3]> {
        self.brush
            .or_else(|| editor.draft()?.get(self.selected.as_ref()?).copied())
    }

    /// Brush choice follows accepted edits, never a rejected or pending intent.
    pub fn accepted(&mut self, key: &str, editor: &Editor<PictureRules>) {
        if let Some(color) = editor.draft().and_then(|colors| colors.get(key)) {
            self.brush = Some(*color);
        }
    }

    pub fn update(&mut self, message: Message, editor: &Editor<PictureRules>) -> Option<Edit> {
        match message {
            Message::Select(key) => {
                if !editor.capabilities().keys.contains(&key) {
                    return None;
                }
                self.selected = Some(key.clone());
                self.gesture = Gesture::Idle;
                return Some(Edit::Color {
                    key,
                    color: self.color(editor)?,
                });
            }
            Message::Color(color) => {
                return Some(Edit::Color {
                    key: self.selected.clone()?,
                    color,
                });
            }
            Message::Channel(channel, value) => {
                let mut color = self.color(editor)?;
                color[match channel {
                    Channel::Red => 0,
                    Channel::Green => 1,
                    Channel::Blue => 2,
                }] = value;
                return Some(Edit::Color {
                    key: self.selected.clone()?,
                    color,
                });
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::model::picture::{Capabilities, Content, Evidence, Snapshot};
    use std::collections::BTreeMap;

    #[test]
    fn accepted_brush_paints_selected_keys_but_rejected_color_does_not_replace_it() {
        let mut editor = Editor::new(PictureRules::new(Capabilities {
            backend_id: "test".into(),
            keys: vec!["a".into(), "b".into()],
            lighting_effect: None,
        }));
        editor.accept_read(Ok(Snapshot {
            backend_id: "test".into(),
            revision: vec![1],
            context_revision: vec![],
            evidence: Evidence::Readback,
            content: Content::Editable(BTreeMap::from([
                ("a".into(), [1, 2, 3]),
                ("b".into(), [4, 5, 6]),
            ])),
        }));
        let mut form = Form::default();
        let initial = form.update(Message::Select("a".into()), &editor).unwrap();
        editor.edit(initial).unwrap();
        form.accepted("a", &editor);
        let rejected = form.update(Message::Color([9, 9, 9]), &editor).unwrap();
        editor.invalidate();
        assert!(editor.edit(rejected).is_err());
        assert_eq!(form.color(&editor), Some([1, 2, 3]));
        let paint = form.update(Message::Select("b".into()), &editor).unwrap();
        assert_eq!(
            paint,
            Edit::Color {
                key: "b".into(),
                color: [1, 2, 3]
            }
        );
        assert_eq!(editor.draft().unwrap()["b"], [4, 5, 6]);
        let edit = form
            .update(Message::Channel(Channel::Green, 22), &editor)
            .unwrap();
        assert_eq!(
            edit,
            Edit::Color {
                key: "b".into(),
                color: [1, 22, 3]
            }
        );
    }
}
