//! Read-only projections and widgets; no backend imports or report knowledge.
use super::{Closing, Desktop, Message, Page};
use crate::control_widgets;
use crate::{panels, physical_board, shortcut};
use byakko_core::{
    Action,
    session::{Problem, ReconnectCause, ReconnectCaution, ReconnectSurface, Status},
};
use iced::{
    Element, Fill, Length,
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
    if app.session.picture().is_some() {
        navigation = navigation.push(panels::selectable_button(
            &app.ui,
            "Per-key colors",
            app.page == Page::Picture,
            Some(Message::Page(Page::Picture)),
        ));
    }
    if app.session.settings().is_some() {
        navigation = navigation.push(panels::selectable_button(
            &app.ui,
            "Settings",
            app.page == Page::Settings,
            Some(Message::Page(Page::Settings)),
        ));
    }
    if app.session.archive().is_some() {
        navigation = navigation.push(panels::selectable_button(
            &app.ui,
            "Local configurations",
            app.page == Page::Archive,
            Some(Message::Page(Page::Archive)),
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
        Page::Archive => super::archive::view(app),
        Page::Keys => keymap(app),
        Page::Macros => super::macro_view::view(app),
        Page::Lighting => super::lighting::view(app),
        Page::Picture => super::picture::view(app),
        Page::Settings => super::settings::view(app),
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
    let toolbar = control_widgets::transaction_toolbar(
        &app.ui,
        "Read / reconnect",
        (!app.busy()).then_some(Message::Read),
        (!app.busy() && !dirty.is_empty()).then_some(Message::Revert),
        (ready && !dirty.is_empty()).then_some(Message::Apply),
        format!("{} staged", dirty.len()),
    );
    let content = column![toolbar, text(status(app)), layers]
        .spacing(app.ui.spacing.m)
        .push(panels::panel(&app.ui, "Physical keys", keys(app)))
        .push(keymap_detail(app));
    scrollable(content).height(Fill).into()
}

fn keymap_detail(app: &Desktop) -> Element<'_, Message> {
    panels::split(
        &app.ui,
        || {
            let mut controls =
                column![text(selected_action(app)), search(app)].spacing(app.ui.spacing.s);
            if let Some(shortcut) = shortcut::view(app) {
                controls = controls.push(shortcut);
            }
            panels::panel(&app.ui, selected_label(app), controls.into())
        },
        || panels::panel(&app.ui, "Staged changes", staged_edits(app)),
    )
}

fn staged_edits(app: &Desktop) -> Element<'_, Message> {
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
    scrollable(edits)
        .height(Length::Fixed(app.ui.list_preview_height))
        .into()
}

fn keys(app: &Desktop) -> Element<'_, Message> {
    let keys = app
        .session
        .descriptor()
        .keys
        .iter()
        .filter(|key| key.visible)
        .collect();
    physical_board::view(&app.ui, keys, app.selected.clone(), |key| {
        Some(Message::SelectKey(key.id.clone()))
    })
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

fn selected_action(app: &Desktop) -> String {
    let Some(key) = app
        .session
        .descriptor()
        .keys
        .iter()
        .find(|key| Some(&key.id) == app.selected.as_ref())
    else {
        return "Select a key to inspect its action".into();
    };
    let binding = app
        .session
        .draft()
        .and_then(|draft| draft.get(&app.layer))
        .and_then(|layer| layer.get(&key.id));
    let action = binding.map_or_else(|| "Unread".into(), |action| action_label(app, action));
    if key.writable {
        format!("Current action: {action}")
    } else {
        format!("Current action: {action} · fixed")
    }
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
        scrollable(actions).height(Length::Fixed(app.ui.list_preview_height))
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
        Action::Shortcut { modifiers, key } => app
            .session
            .descriptor()
            .shortcuts
            .as_ref()
            .and_then(|caps| shortcut::label(caps, modifiers, *key))
            .unwrap_or_else(|| format!("Shortcut {modifiers:?} + {key}")),
        Action::Named { id } => id.clone(),
        Action::Opaque { label, .. } => label.clone(),
    }
}

pub(super) fn status(app: &Desktop) -> String {
    use crate::discovery::Availability;
    use byakko_core::session::Activity;
    match app.session.activity() {
        Activity::MacroFile { .. } => return "Working with a local macro file…".into(),
        Activity::Recording { .. } => return "Recording into the local draft…".into(),
        Activity::HostLighting { phase, .. } => {
            return match phase {
                byakko_core::session::HostPhase::Starting => "Starting host lighting…".into(),
                byakko_core::session::HostPhase::Streaming => "Streaming host lighting…".into(),
                byakko_core::session::HostPhase::Stopping => "Restoring saved lighting…".into(),
            };
        }
        Activity::Read { .. }
        | Activity::ReadMacro { .. }
        | Activity::ReadLighting { .. }
        | Activity::ReadPicture { .. }
        | Activity::ReadSettings { .. }
        | Activity::CaptureArchive { .. }
        | Activity::ReviewArchive { .. } => {
            return "Reading device…".into();
        }
        Activity::Apply { .. }
        | Activity::ApplyMacro { .. }
        | Activity::ApplyLighting { .. }
        | Activity::ApplyPicture { .. }
        | Activity::ApplySetting { .. }
        | Activity::ApplyArchive { .. } => {
            return "Backing up, applying and verifying…".into();
        }
        Activity::Idle => {}
    }
    if let Some(presence) = &app.presence {
        match presence {
            Availability::Missing => return "Keyboard not connected · draft retained".into(),
            Availability::Ambiguous { count } => {
                return format!("{count} matching configuration interfaces · connect one keyboard");
            }
            Availability::Error(reason) => return format!("USB discovery failed: {reason}"),
            Availability::Ready { .. } => {}
        }
    }
    match app.session.status() {
        Status::Disconnected if app.auto_read == super::AutoRead::ManualOnly => {
            "Draft retained · read manually before editing".into()
        }
        Status::Disconnected if app.presence.is_none() => "Looking for keyboard…".into(),
        Status::Disconnected => "Keyboard found · reading…".into(),
        Status::Ready => "Readback verified · edits are staged until applied".into(),
        Status::Conflict { .. } => "Device changed since the draft began. Draft retained; revert it, then read again to use device values.".into(),
        Status::Unverified { problem } => problem_label(problem),
    }
}

pub(super) fn problem_label(problem: &Problem) -> String {
    match problem {
        Problem::ReadRequired => "Read the device before editing".into(),
        Problem::Read(reason) => format!("Read failed: {reason}"),
        Problem::Apply(failure) => apply_failure_label(failure),
        Problem::InvalidApplyResult(reason) => format!("Invalid readback: {reason}"),
        Problem::ApplyReadbackMismatch => {
            "Readback differs from the draft; state is unverified".into()
        }
    }
}

pub(super) fn apply_failure_label(failure: &byakko_core::session::ApplyFailure) -> String {
    format!(
        "Apply failed ({:?} recovery): {}",
        failure.recovery, failure.message
    )
}

pub(super) fn reconnect_caution_label(caution: ReconnectCaution<'_>) -> String {
    let surface = match caution.surface {
        ReconnectSurface::Keymap => "Keys",
        ReconnectSurface::Macro => "Macros",
        ReconnectSurface::Lighting => "Lighting",
        ReconnectSurface::Picture => "Per-key colors",
        ReconnectSurface::Settings => "Settings",
        ReconnectSurface::Archive => "Local configuration",
    };
    let reason = match caution.cause {
        ReconnectCause::Conflict => "Device values conflict with a retained draft".into(),
        ReconnectCause::Apply(failure) => apply_failure_label(failure),
        ReconnectCause::InvalidApplyResult(reason) => format!("Invalid readback: {reason}"),
        ReconnectCause::ApplyReadbackMismatch => {
            "Readback differs from the draft; state is unverified".into()
        }
    };
    format!("{surface}: {reason}")
}
