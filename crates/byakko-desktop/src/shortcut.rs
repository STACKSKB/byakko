//! Device-described shortcut composition for the selected physical key.
use crate::{Desktop, Message as AppMessage, panels};
use byakko_core::{Action, ShortcutCapabilities};
use iced::{
    Element, Fill,
    widget::{button, column, pick_list, row, text},
};
use std::fmt;

#[derive(Clone, Debug)]
pub(super) enum Message {
    ToggleModifier(u16),
    SelectKey(u16),
    Stage,
}

#[derive(Default)]
pub(super) struct Form {
    pub modifiers: Vec<u16>,
    pub key: Option<u16>,
}

impl Form {
    pub fn load(&mut self, action: Option<&Action>, caps: Option<&ShortcutCapabilities>) {
        self.modifiers.clear();
        self.key = None;
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct KeyItem {
    usage: u16,
    label: String,
}

impl fmt::Display for KeyItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
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
    let items: Vec<_> = caps
        .keys
        .iter()
        .map(|choice| KeyItem {
            usage: choice.usage,
            label: choice.label.clone(),
        })
        .collect();
    let selected = items
        .iter()
        .find(|item| Some(item.usage) == app.shortcut.key)
        .cloned();
    let key = pick_list(items, selected, |item: KeyItem| {
        AppMessage::Shortcut(Message::SelectKey(item.usage))
    })
    .placeholder("Choose key")
    .width(app.ui.fields.regular);
    let stage = button("Stage shortcut").on_press_maybe(
        (editable && app.shortcut.action(caps).is_ok())
            .then_some(AppMessage::Shortcut(Message::Stage)),
    );
    Some(
        column![text("Shortcut"), modifiers, key, stage]
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
