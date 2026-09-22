//! Read-only projections and widgets; no backend imports or report knowledge.
use super::{Closing, Desktop, Message, Page};
use crate::panels;
use byakko_core::{
    Action,
    session::{Problem, Status},
};
use iced::{
    Element, Fill,
    widget::{button, column, container, row, scrollable, text, text_input},
};

pub(super) fn shell(app: &Desktop) -> Element<'_, Message> {
    let mut navigation = row![panels::selectable_button(
        &app.ui,
        "Keys",
        app.page == Page::Keys,
        Some(Message::Page(Page::Keys)),
    )]
    .spacing(app.ui.spacing.s);
    if app.session.macros().is_some() {
        navigation = navigation.push(panels::selectable_button(
            &app.ui,
            "Macros",
            app.page == Page::Macros,
            Some(Message::Page(Page::Macros)),
        ));
    }
    if app.session.lighting().is_some() {
        navigation = navigation.push(panels::selectable_button(
            &app.ui,
            "Lighting",
            app.page == Page::Lighting,
            Some(Message::Page(Page::Lighting)),
        ));
    }
    let mut content = column![
        text(&app.session.descriptor().device_name).size(app.ui.type_scale.page_title),
        navigation
    ]
    .spacing(app.ui.spacing.m);
    if let Some(notice) = &app.notice {
        content = content.push(text(notice));
    }
    content = match app.closing {
        Closing::Open => content,
        Closing::Waiting => content.push(text("Waiting for the device operation before closing…")),
        Closing::ConfirmDiscard => content.push(
            row![
                text("Discard all staged drafts and close?"),
                button("Keep editing").on_press(Message::KeepEditing),
                button("Discard & close").on_press(Message::DiscardAndClose),
            ]
            .spacing(app.ui.spacing.m),
        ),
    };
    content = content.push(match app.page {
        Page::Keys => keymap(app),
        Page::Macros => super::macro_view::view(app),
        Page::Lighting => super::lighting::view(app),
    });
    container(content)
        .padding(app.ui.spacing.page_padding)
        .height(Fill)
        .width(Fill)
        .into()
}

fn keymap(app: &Desktop) -> Element<'_, Message> {
    let descriptor = app.session.descriptor();
    let dirty = app.session.changes();
    let ready = !app.busy() && *app.session.status() == Status::Ready;
    let layers = row(descriptor.layers.iter().map(|layer| {
        panels::selectable_button(
            &app.ui,
            &layer.label,
            layer.id == app.layer,
            Some(Message::SelectLayer(layer.id.clone())),
        )
    }))
    .spacing(app.ui.spacing.s);
    let toolbar = row![
        button("Read / reconnect").on_press_maybe((!app.busy()).then_some(Message::Read)),
        button("Revert draft")
            .on_press_maybe((!app.busy() && !dirty.is_empty()).then_some(Message::Revert)),
        button("Apply & verify")
            .on_press_maybe((ready && !dirty.is_empty()).then_some(Message::Apply)),
        text(format!("{} staged", dirty.len())),
    ]
    .spacing(app.ui.spacing.m)
    .align_y(iced::Center);
    let content = column![toolbar, text(status(app)), layers]
        .spacing(app.ui.spacing.m)
        .push(panels::split(
            &app.ui,
            || panels::panel(&app.ui, "Keys", scrollable(keys(app)).height(Fill).into()),
            || panels::panel(&app.ui, "Assign & review", keymap_detail(app)),
        ));
    content.height(Fill).into()
}

fn keymap_detail(app: &Desktop) -> Element<'_, Message> {
    let descriptor = app.session.descriptor();
    let dirty = app.session.changes();
    let edits = column(dirty.iter().map(|change| {
        let key = descriptor
            .keys
            .iter()
            .find(|key| key.id == change.key)
            .map_or(change.key.as_str(), |key| key.label.as_str());
        let layer = descriptor
            .layers
            .iter()
            .find(|layer| layer.id == change.layer)
            .map_or(change.layer.as_str(), |layer| layer.label.as_str());
        let before = app
            .session
            .baseline()
            .and_then(|state| state.bindings.get(&change.layer))
            .and_then(|layer| layer.get(&change.key));
        text(format!(
            "{layer} / {key}: {} → {}",
            before.map_or_else(|| "Unknown".into(), |action| action_label(app, action)),
            action_label(app, &change.action)
        ))
        .into()
    }))
    .spacing(app.ui.spacing.xs);
    column![
        text(selected_label(app)).size(app.ui.type_scale.section_title),
        search(app),
        text("Staged changes"),
        scrollable(edits).height(Fill)
    ]
    .spacing(app.ui.spacing.m)
    .height(Fill)
    .into()
}

fn keys(app: &Desktop) -> Element<'_, Message> {
    let descriptor = app.session.descriptor();
    let mut keys: Vec<_> = descriptor.keys.iter().filter(|key| key.visible).collect();
    keys.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    column(keys.into_iter().map(|key| {
        let binding = app
            .session
            .draft()
            .and_then(|draft| draft.get(&app.layer))
            .and_then(|layer| layer.get(&key.id));
        let label = format!(
            "{}  ·  {}{}",
            key.label,
            binding.map_or_else(|| "Unread".into(), |action| action_label(app, action)),
            if key.writable { "" } else { " (fixed)" }
        );
        container(panels::selectable_button(
            &app.ui,
            label,
            app.selected.as_ref() == Some(&key.id),
            Some(Message::SelectKey(key.id.clone())),
        ))
        .width(Fill)
        .into()
    }))
    .spacing(app.ui.spacing.xs)
    .into()
}

fn selected_label(app: &Desktop) -> String {
    app.session
        .descriptor()
        .keys
        .iter()
        .find(|key| Some(&key.id) == app.selected.as_ref())
        .map_or_else(
            || "Select a key".into(),
            |key| format!("Assign action · {}", key.label),
        )
}

fn search(app: &Desktop) -> Element<'_, Message> {
    let query = app.search.to_lowercase();
    let editable = !app.busy()
        && *app.session.status() == Status::Ready
        && app
            .session
            .descriptor()
            .keys
            .iter()
            .any(|key| key.writable && Some(&key.id) == app.selected.as_ref());
    let actions = column(
        app.session
            .descriptor()
            .actions
            .iter()
            .enumerate()
            .filter(|(_, choice)| choice.label.to_lowercase().contains(&query))
            .map(|(index, choice)| {
                button(text(&choice.label))
                    .width(Fill)
                    .on_press_maybe(editable.then_some(Message::Stage(index)))
                    .into()
            }),
    )
    .spacing(app.ui.spacing.xs);
    column![
        text_input("Find an action…", &app.search).on_input(Message::Search),
        scrollable(actions).height(Fill)
    ]
    .spacing(app.ui.spacing.s)
    .height(Fill)
    .into()
}

fn action_label(app: &Desktop, action: &Action) -> String {
    if let Some(editor) = app.session.macros()
        && let Some(binding) = editor
            .capabilities()
            .bindings
            .iter()
            .find(|binding| &binding.action == action)
    {
        let slot = editor
            .capabilities()
            .slots
            .iter()
            .find(|slot| slot.id == binding.slot)
            .map_or(binding.slot.as_str(), |slot| slot.label.as_str());
        return format!("{slot} · {}", binding.label);
    }
    let descriptor = app.session.descriptor();
    if let Some(choice) = descriptor
        .actions
        .iter()
        .find(|choice| &choice.action == action)
    {
        return choice.label.clone();
    }
    match action {
        Action::Key(usage) => format!("Key {usage}"),
        Action::Disabled => "Disabled".into(),
        Action::Macro { slot, mode } => format!("Macro {slot} · mode {mode}"),
        Action::Shortcut { modifiers, key } => format!("Shortcut {modifiers:?} + {key}"),
        Action::Named { id } => id.clone(),
        Action::Opaque { label, .. } => label.clone(),
    }
}

pub(super) fn status(app: &Desktop) -> String {
    use byakko_core::session::Activity;
    match app.session.activity() {
        Activity::MacroFile { .. } => return "Working with a local macro file…".into(),
        Activity::Recording { .. } => return "Recording into the local draft…".into(),
        Activity::Read { .. } | Activity::ReadMacro { .. } | Activity::ReadLighting { .. } => {
            return "Reading device…".into();
        }
        Activity::Apply { .. } | Activity::ApplyMacro { .. } | Activity::ApplyLighting { .. } => {
            return "Backing up, applying and verifying…".into();
        }
        Activity::Idle => {}
    }
    match app.session.status() {
        Status::Disconnected => "Disconnected · draft retained".into(),
        Status::Ready => "Readback verified · edits are staged until applied".into(),
        Status::Conflict { .. } => "Device changed since the draft began. Draft retained; revert it, then read again to use device values.".into(),
        Status::Unverified { problem } => problem_label(problem),
    }
}

pub(super) fn problem_label(problem: &Problem) -> String {
    match problem {
        Problem::ReadRequired => "Read the device before editing".into(),
        Problem::Read(reason) => format!("Read failed: {reason}"),
        Problem::Apply(failure) => format!(
            "Apply failed ({:?} recovery): {}",
            failure.recovery, failure.message
        ),
        Problem::InvalidApplyResult(reason) => format!("Invalid readback: {reason}"),
        Problem::ApplyReadbackMismatch => {
            "Readback differs from the draft; state is unverified".into()
        }
    }
}
