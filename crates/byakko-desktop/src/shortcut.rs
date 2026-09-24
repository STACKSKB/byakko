//! Device-described shortcut composition for the selected physical key.
use crate::{Desktop, Message as AppMessage, panels};
use byakko_core::{Action, ShortcutCapabilities};
use iced::{
    Element, Fill,
    widget::{button, column, row, text, text_input},
};

#[derive(Clone, Debug)]
pub(super) enum Message {
    ToggleModifier(u16),
    SelectKey(u16),
    Stage,
    Search(String),
}

#[derive(Default)]
pub(super) struct Form {
    pub modifiers: Vec<u16>,
    pub key: Option<u16>,
    pub query: String,
}

impl Form {
    pub fn load(&mut self, action: Option<&Action>, caps: Option<&ShortcutCapabilities>) {
        self.modifiers.clear();
        self.key = None;
        self.query.clear();
        if let (Some(Action::Shortcut { modifiers, key }), Some(caps)) = (action, caps)
            && caps.compose(modifiers, *key).is_ok()
        {
            self.modifiers.clone_from(modifiers);
            self.key = Some(*key);
        }
    }

    pub fn toggle_modifier(
        &mut self,
        caps: &ShortcutCapabilities,
        usage: u16,
    ) -> Result<(), String> {
        if !caps.modifiers.iter().any(|choice| choice.usage == usage) {
            return Err("Modifier is not supported by this keyboard".into());
        }
        if let Some(index) = self.modifiers.iter().position(|current| *current == usage) {
            self.modifiers.remove(index);
        } else if self.modifiers.len() < caps.max_modifiers {
            self.modifiers.push(usage);
        } else {
            return Err("This keyboard's shortcut has reached its modifier limit".into());
        }
        Ok(())
    }

    pub fn select_key(&mut self, caps: &ShortcutCapabilities, usage: u16) -> Result<(), String> {
        if !caps.keys.iter().any(|choice| choice.usage == usage) {
            return Err("Shortcut key is not supported by this keyboard".into());
        }
        self.key = Some(usage);
        Ok(())
    }

    pub fn action(&self, caps: &ShortcutCapabilities) -> Result<Action, String> {
        caps.compose(&self.modifiers, self.key.ok_or("Select a shortcut key")?)
    }
}

pub(super) fn view(app: &Desktop) -> Option<Element<'_, AppMessage>> {
    let caps = app.session.descriptor().shortcuts.as_ref()?;
    let editable = !app.busy()
        && *app.session.status() == byakko_core::session::Status::Ready
        && app
            .session
            .descriptor()
            .keys
            .iter()
            .any(|key| key.writable && Some(&key.id) == app.selected.as_ref());
    let modifiers = row(caps.modifiers.iter().map(|choice| {
        panels::selectable_button(
            &app.ui,
            &choice.label,
            app.shortcut.modifiers.contains(&choice.usage),
            editable.then_some(AppMessage::Shortcut(Message::ToggleModifier(choice.usage))),
        )
    }))
    .spacing(app.ui.spacing.s);
    let key = text_input("Find shortcut key…", &app.shortcut.query)
        .on_input_maybe(editable.then_some(|query| AppMessage::Shortcut(Message::Search(query))))
        .width(app.ui.fields.regular);
    let selected = caps
        .keys
        .iter()
        .find(|choice| Some(choice.usage) == app.shortcut.key)
        .map_or("Choose a key", |choice| choice.label.as_str());
    let matches = row(caps
        .keys
        .iter()
        .filter(|choice| {
            !app.shortcut.query.trim().is_empty()
                && choice
                    .label
                    .to_lowercase()
                    .contains(&app.shortcut.query.trim().to_lowercase())
        })
        .take(12)
        .map(|choice| {
            panels::selectable_button(
                &app.ui,
                &choice.label,
                Some(choice.usage) == app.shortcut.key,
                editable.then_some(AppMessage::Shortcut(Message::SelectKey(choice.usage))),
            )
        }))
    .spacing(app.ui.spacing.xs)
    .wrap();
    let stage = button("Stage shortcut").on_press_maybe(
        (editable && app.shortcut.action(caps).is_ok())
            .then_some(AppMessage::Shortcut(Message::Stage)),
    );
    Some(
        column![
            text("Shortcut"),
            modifiers,
            key,
            matches,
            text(selected),
            stage
        ]
        .spacing(app.ui.spacing.s)
        .width(Fill)
        .into(),
    )
}

pub(super) fn label(caps: &ShortcutCapabilities, modifiers: &[u16], key: u16) -> Option<String> {
    let mut names = Vec::with_capacity(modifiers.len() + 1);
    for usage in modifiers {
        names.push(
            caps.modifiers
                .iter()
                .find(|choice| choice.usage == *usage)?
                .label
                .as_str(),
        );
    }
    names.push(
        caps.keys
            .iter()
            .find(|choice| choice.usage == key)?
            .label
            .as_str(),
    );
    Some(names.join("+"))
}
