//! Unsubmitted, capability-described shortcut choices for the selected key.
use byakko_core::model::keymap::{Action, ShortcutCapabilities};

#[derive(Clone, Debug)]
pub enum Message {
    ToggleModifier(u16),
    SelectKey(u16),
    Stage,
    Search(String),
}

#[derive(Default)]
pub struct Form {
    pub(crate) modifiers: Vec<u16>,
    pub(crate) key: Option<u16>,
    pub(crate) query: String,
    pub(crate) error: Option<String>,
}

impl Form {
    pub fn load(&mut self, action: Option<&Action>, caps: Option<&ShortcutCapabilities>) {
        self.modifiers.clear();
        self.key = None;
        self.query.clear();
        self.error = None;
        if let (Some(Action::Shortcut { modifiers, key }), Some(caps)) = (action, caps)
            && caps.compose(modifiers, *key).is_ok()
        {
            self.modifiers.clone_from(modifiers);
            self.key = Some(*key);
        }
    }

    pub fn update(
        &mut self,
        message: Message,
        caps: &ShortcutCapabilities,
    ) -> Result<Option<Action>, String> {
        let result = (|| {
            match message {
                Message::ToggleModifier(usage) => {
                    if !caps.modifiers.iter().any(|choice| choice.usage == usage) {
                        return Err("Modifier is not supported by this keyboard".into());
                    }
                    if let Some(index) = self.modifiers.iter().position(|current| *current == usage)
                    {
                        self.modifiers.remove(index);
                    } else if self.modifiers.len() < caps.max_modifiers {
                        self.modifiers.push(usage);
                    } else {
                        return Err(
                            "This keyboard's shortcut has reached its modifier limit".into()
                        );
                    }
                }
                Message::SelectKey(usage) => {
                    if !caps.keys.iter().any(|choice| choice.usage == usage) {
                        return Err("Shortcut key is not supported by this keyboard".into());
                    }
                    self.key = Some(usage);
                }
                Message::Search(query) => self.query = query,
                Message::Stage => return self.action(caps).map(Some),
            }
            Ok(None)
        })();
        self.error = result.as_ref().err().cloned();
        result
    }

    pub fn action(&self, caps: &ShortcutCapabilities) -> Result<Action, String> {
        caps.compose(&self.modifiers, self.key.ok_or("Select a shortcut key")?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::model::keymap::UsageChoice;
    fn caps() -> ShortcutCapabilities {
        ShortcutCapabilities {
            modifiers: [1, 2, 3]
                .map(|usage| UsageChoice {
                    label: format!("Modifier {usage}"),
                    usage,
                })
                .into(),
            keys: [10, 11]
                .map(|usage| UsageChoice {
                    label: format!("Key {usage}"),
                    usage,
                })
                .into(),
            min_modifiers: 2,
            max_modifiers: 2,
        }
    }

    #[test]
    fn stage_obeys_advertised_minimum_and_maximum_without_rejecting_intermediate_choices() {
        let caps = caps();
        let mut form = Form::default();
        assert!(form.update(Message::Stage, &caps).is_err());
        assert_eq!(form.update(Message::SelectKey(10), &caps).unwrap(), None);
        assert!(form.error.is_none());
        form.update(Message::ToggleModifier(1), &caps).unwrap();
        assert!(form.update(Message::Stage, &caps).is_err());
        assert_eq!(form.modifiers, [1]);
        form.update(Message::ToggleModifier(2), &caps).unwrap();
        let expected = Action::Shortcut {
            modifiers: vec![1, 2],
            key: 10,
        };
        assert_eq!(
            form.update(Message::Stage, &caps).unwrap(),
            Some(expected.clone())
        );
        assert_eq!(form.action(&caps).unwrap(), expected);
        assert!(form.update(Message::ToggleModifier(3), &caps).is_err());
        assert_eq!(form.modifiers, [1, 2]);
        assert_eq!(form.key, Some(10));
        form.update(Message::ToggleModifier(1), &caps).unwrap();
        assert_eq!(form.modifiers, [2]);
        assert!(form.error.is_none());
        assert!(form.action(&caps).is_err());
    }

    #[test]
    fn rejected_choices_preserve_form_inputs_and_valid_edits_clear_local_error() {
        let caps = caps();
        let mut form = Form::default();
        form.update(Message::SelectKey(10), &caps).unwrap();
        form.update(Message::ToggleModifier(1), &caps).unwrap();
        form.update(Message::Search("Key".into()), &caps).unwrap();
        for message in [Message::ToggleModifier(99), Message::SelectKey(99)] {
            assert!(form.update(message, &caps).is_err());
            assert_eq!(form.modifiers, [1]);
            assert_eq!(form.key, Some(10));
            assert_eq!(form.query, "Key");
            assert!(form.error.is_some());
        }
        assert_eq!(
            form.update(Message::Search("Other".into()), &caps).unwrap(),
            None
        );
        assert!(form.error.is_none());
        assert_eq!(form.modifiers, [1]);
        assert_eq!(form.key, Some(10));
        form.update(Message::SelectKey(11), &caps).unwrap();
        assert_eq!(form.key, Some(11));
    }

    #[test]
    fn loading_a_target_keeps_only_valid_supported_shortcuts_and_resets_transient_form_state() {
        let caps = caps();
        let mut form = Form::default();
        let valid = Action::Shortcut {
            modifiers: vec![2, 1],
            key: 11,
        };
        form.load(Some(&valid), Some(&caps));
        assert_eq!(form.modifiers, [2, 1]);
        assert_eq!(form.key, Some(11));
        assert_eq!(form.action(&caps).unwrap(), valid);
        form.query = "Search".into();
        form.error = Some("Old error".into());
        let unsupported = Action::Shortcut {
            modifiers: vec![1, 99],
            key: 10,
        };
        for (action, catalog) in [
            (Some(&unsupported), Some(&caps)),
            (Some(&valid), None),
            (None, Some(&caps)),
            (Some(&Action::Key(10)), Some(&caps)),
        ] {
            form.load(action, catalog);
            assert!(form.modifiers.is_empty());
            assert!(form.key.is_none());
            assert!(form.query.is_empty());
            assert!(form.error.is_none());
        }
    }
}
