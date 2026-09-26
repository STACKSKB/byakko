//! Search and navigation over advertised assignment choices.
use byakko_core::model::keymap::{Action, ActionCategory, ActionChoice};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum InputMode {
    #[default]
    Browse,
    Capture,
}

#[derive(Clone, Debug)]
pub enum Message {
    Category(ActionCategory),
    Search(String),
    SubmitSearch,
    Capture,
    CancelCapture,
    Captured(u16),
    Scrolled,
    VisibleSection(ActionCategory),
}
pub enum Scroll {
    Top,
    Section(ActionCategory),
    Measure,
}
pub enum Intent {
    Assign(usize),
    Scroll(Scroll),
}

#[derive(Default)]
pub struct Form {
    pub category: Option<ActionCategory>,
    pub query: String,
    pub input: InputMode,
}
impl Form {
    pub fn update(
        &mut self,
        message: Message,
        actions: &[ActionChoice],
    ) -> Result<Option<Intent>, String> {
        match message {
            Message::Category(category) => {
                self.category = Some(category);
                self.query.clear();
                self.input = InputMode::Browse;
                Ok(Some(Intent::Scroll(Scroll::Section(category))))
            }
            Message::Search(query) => {
                self.query = query;
                self.input = InputMode::Browse;
                Ok(Some(Intent::Scroll(Scroll::Top)))
            }
            Message::SubmitSearch => Ok(exact_match(actions, &self.query).map(Intent::Assign)),
            Message::Capture => {
                self.input = InputMode::Capture;
                Ok(None)
            }
            Message::CancelCapture => {
                self.input = InputMode::Browse;
                Ok(None)
            }
            Message::Captured(usage) if self.input == InputMode::Capture => {
                self.input = InputMode::Browse;
                actions
                    .iter()
                    .position(|choice| choice.action == Action::Key(usage))
                    .map(|index| Some(Intent::Assign(index)))
                    .ok_or_else(|| {
                        "This key is not supported as an assignment. Choose an advertised action."
                            .into()
                    })
            }
            Message::Captured(_) => Ok(None),
            Message::Scrolled => Ok(Some(Intent::Scroll(Scroll::Measure))),
            Message::VisibleSection(category) => {
                self.category = Some(category);
                Ok(None)
            }
        }
    }
}

pub fn group_label(group: ActionCategory) -> &'static str {
    match group {
        ActionCategory::Alphanumeric => "Alphanumeric",
        ActionCategory::Modifiers => "Modifiers",
        ActionCategory::Navigation => "Navigation",
        ActionCategory::Function => "Function keys",
        ActionCategory::Numpad => "Numpad",
        ActionCategory::Media => "Media keys",
        ActionCategory::Mouse => "Mouse",
        ActionCategory::System => "System",
        ActionCategory::Shortcuts => "Shortcuts",
        ActionCategory::Other => "Other",
    }
}
pub fn compact_label(choice: &ActionChoice) -> String {
    if let Action::Key(usage) = choice.action {
        match usage {
            0x59..=0x61 => return format!("Num {}", usage - 0x58),
            0x62 => return "Num 0".into(),
            0x63 => return "Num .".into(),
            0x54 => return "Num /".into(),
            0x55 => return "Num *".into(),
            0x56 => return "Num −".into(),
            0x57 => return "Num +".into(),
            0x58 => return "Num Enter".into(),
            _ => {}
        }
    }
    choice.label.clone()
}
pub fn results<'a>(actions: &'a [ActionChoice], query: &str) -> Vec<(usize, &'a ActionChoice)> {
    let words: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    actions
        .iter()
        .enumerate()
        .filter(|(_, choice)| {
            let haystack = format!(
                "{} {} {}",
                choice.label,
                compact_label(choice),
                group_label(choice.category)
            )
            .to_lowercase();
            words.iter().all(|word| haystack.contains(word))
        })
        .collect()
}
pub fn exact_match(actions: &[ActionChoice], query: &str) -> Option<usize> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }
    let found = results(actions, query);
    let exact: Vec<_> = found
        .iter()
        .filter(|(_, choice)| {
            choice.label.eq_ignore_ascii_case(query)
                || compact_label(choice).eq_ignore_ascii_case(query)
        })
        .collect();
    match (exact.as_slice(), found.as_slice()) {
        ([choice], _) => Some(choice.0),
        (_, [choice]) => Some(choice.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn choices() -> Vec<ActionChoice> {
        vec![
            ActionChoice {
                label: "A".into(),
                action: Action::Key(4),
                category: ActionCategory::Alphanumeric,
            },
            ActionChoice {
                label: "Numpad 9".into(),
                action: Action::Key(0x61),
                category: ActionCategory::Numpad,
            },
            ActionChoice {
                label: "Page up".into(),
                action: Action::Key(0x4b),
                category: ActionCategory::Navigation,
            },
        ]
    }
    #[test]
    fn search_uses_words_aliases_categories_and_unique_exact_matches() {
        let choices = choices();
        assert_eq!(exact_match(&choices, " a "), Some(0));
        assert_eq!(exact_match(&choices, " NUM 9 "), Some(1));
        assert_eq!(exact_match(&choices, "navigation up"), Some(2));
        assert_eq!(exact_match(&choices, "pa"), None);
        assert_eq!(exact_match(&choices, ""), None);
        assert_eq!(exact_match(&choices, "unavailable"), None);
        let mut form = Form::default();
        form.update(Message::Search("A".into()), &choices).unwrap();
        form.update(Message::Capture, &choices).unwrap();
        form.update(Message::Category(ActionCategory::Numpad), &choices)
            .unwrap();
        assert_eq!(form.input, InputMode::Browse);
        assert!(form.query.is_empty());
        assert_eq!(
            results(&choices, &form.query).len(),
            3,
            "categories navigate without hiding sections"
        );
    }
    #[test]
    fn physical_capture_is_one_shot_and_never_invents_an_action() {
        let choices = choices();
        let mut form = Form::default();
        assert!(
            form.update(Message::Captured(4), &choices)
                .unwrap()
                .is_none()
        );
        form.update(Message::Capture, &choices).unwrap();
        assert!(matches!(
            form.update(Message::Captured(0x61), &choices).unwrap(),
            Some(Intent::Assign(1))
        ));
        assert_eq!(form.input, InputMode::Browse);
        form.update(Message::Capture, &choices).unwrap();
        assert!(form.update(Message::Captured(0xff), &choices).is_err());
        assert_eq!(form.input, InputMode::Browse);
    }
}
