//! Only unfinished numeric input belongs to the form; typed settings live in core.
use byakko_core::{
    editor::{Editor, settings::SettingsRules},
    model::settings::{Edit, Value},
    validation::settings::validate_value,
};

#[derive(Clone, Debug)]
pub enum Message {
    Number(String, String),
    ApplyNumber(String),
    Edit(Edit),
    Read,
    Revert,
    Save,
}

#[derive(Default)]
pub struct Form {
    pub(crate) input: Option<(String, String)>,
}

impl Form {
    pub fn has_input(&self) -> bool {
        self.input.is_some()
    }

    pub fn clear(&mut self) {
        self.input = None;
    }

    /// Clear submitted text only after the application has accepted its edit.
    pub fn accepted(&mut self, id: &str) {
        if self.input.as_ref().is_some_and(|(field, _)| field == id) {
            self.input = None;
        }
    }

    pub fn update(
        &mut self,
        message: Message,
        editor: &Editor<SettingsRules>,
    ) -> Result<Option<Edit>, String> {
        match message {
            Message::Number(id, value) => self.input = Some((id, value)),
            Message::ApplyNumber(id) => {
                let value = self
                    .input
                    .as_ref()
                    .filter(|(field, _)| field == &id)
                    .ok_or("Enter a setting value first")?
                    .1
                    .trim()
                    .parse::<u16>()
                    .map_err(|_| "Enter a whole number from 0 to 65535")?;
                let edit = Edit {
                    id,
                    value: Value::Number(value),
                };
                validate_value(editor.capabilities(), &edit)?;
                return Ok(Some(edit));
            }
            Message::Edit(edit) => {
                validate_value(editor.capabilities(), &edit)?;
                return Ok(Some(edit));
            }
            Message::Read | Message::Revert | Message::Save => {}
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::model::settings::{Capabilities, Field, Kind};

    #[test]
    fn numeric_input_respects_step_and_disabled_zero_and_survives_rejection() {
        let editor = Editor::new(
            SettingsRules::new(Capabilities {
                backend_id: "test".into(),
                fields: vec![Field {
                    id: "sleep".into(),
                    label: "Sleep".into(),
                    kind: Kind::Number {
                        min: 10,
                        max: 60,
                        step: 10,
                        unit: "s".into(),
                        disabled_zero: true,
                    },
                }],
            })
            .unwrap(),
        );
        let mut form = Form::default();
        for invalid in ["-1", "65536", "12", ""] {
            form.update(Message::Number("sleep".into(), invalid.into()), &editor)
                .unwrap();
            assert!(
                form.update(Message::ApplyNumber("sleep".into()), &editor)
                    .is_err()
            );
            assert_eq!(form.input.as_ref().unwrap().1, invalid);
        }
        form.update(Message::Number("sleep".into(), "0".into()), &editor)
            .unwrap();
        assert_eq!(
            form.update(Message::ApplyNumber("sleep".into()), &editor)
                .unwrap(),
            Some(Edit {
                id: "sleep".into(),
                value: Value::Number(0)
            })
        );
        assert!(form.has_input());
        form.accepted("sleep");
        assert!(!form.has_input());
    }
}
