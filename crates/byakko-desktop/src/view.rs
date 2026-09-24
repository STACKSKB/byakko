//! Read-only projections and widgets; no backend imports or report knowledge.
use super::{Closing, Desktop, Message, Page};
use crate::control_widgets;
use crate::{panels, physical_board, shortcut};
use byakko_core::{
    Action,
    session::{Problem, ReconnectCause, ReconnectCaution, ReconnectSurface, Status},
};
use iced::{
    Element, Fill,
    widget::{button, column, container, row, scrollable, text},
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
        Closing::ConfirmDiscard => content,
    };
    if app.page != Page::Keys {
        content = content.push(keys(app));
    }
    content = content.push(match app.page {
        Page::Archive => super::archive::view(app),
        Page::Keys => key_workspace(app),
        Page::Macros => super::macro_view::view(app),
        Page::Lighting => super::lighting::view(app),
        Page::Picture => super::picture::view(app),
        Page::Settings => super::settings::view(app),
    });
    let workspace = container(content)
        .width(Fill)
        .max_width(app.ui.workspace_width)
        .height(Fill);
    let base: Element<'_, Message> = container(workspace)
        .padding(app.ui.spacing.page_padding)
        .height(Fill)
        .center_x(Fill)
        .width(Fill)
        .into();
    if app.closing != Closing::ConfirmDiscard {
        return base;
    }
    let dialog = container(
        column![
            text("Discard unsaved changes?").size(app.ui.type_scale.section_title),
            text("Your unsaved changes will be lost when Byakko closes."),
            row![
                button("Keep editing").on_press(Message::KeepEditing),
                button("Discard & close").on_press(Message::DiscardAndClose),
            ]
            .spacing(app.ui.spacing.m),
        ]
        .spacing(app.ui.spacing.l),
    )
    .padding(app.ui.spacing.page_padding)
    .style(container::bordered_box);
    let backdrop = container(iced::widget::opaque(dialog))
        .center_x(Fill)
        .center_y(Fill)
        .style(|_| container::Style {
            background: Some(iced::Color::BLACK.scale_alpha(0.65).into()),
            ..Default::default()
        });
    iced::widget::stack![base, iced::widget::opaque(backdrop)].into()
}

fn keymap_toolbar(app: &Desktop) -> Element<'_, Message> {
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
    column![toolbar, text(status(app)), layers]
        .spacing(app.ui.spacing.m)
        .into()
}

fn keymap(app: &Desktop) -> Element<'_, Message> {
    column![keymap_toolbar(app), keymap_detail(app)]
        .spacing(app.ui.spacing.m)
        .height(Fill)
        .into()
}

fn keymap_detail(app: &Desktop) -> Element<'_, Message> {
    let mut extra = column![].spacing(app.ui.spacing.s);
    if let Some(shortcut) = shortcut::view(app) {
        extra = extra.push(shortcut);
    }
    if !app.session.changes().is_empty() {
        extra = extra.push(text("Changes to save")).push(staged_edits(app));
    }
    scrollable(extra).height(Fill).into()
}

fn key_workspace(app: &Desktop) -> Element<'_, Message> {
    iced::widget::responsive(move |size| {
        if size.width >= app.ui.key_sidebar_breakpoint {
            row![
                column![keys(app), keymap(app)]
                    .spacing(app.ui.spacing.m)
                    .width(Fill),
                container(crate::action_catalog::view(app))
                    .width(app.ui.key_sidebar_width)
                    .height(Fill)
            ]
            .spacing(app.ui.spacing.l)
            .height(Fill)
            .into()
        } else {
            column![
                keys(app),
                keymap_toolbar(app),
                row![
                    container(crate::action_catalog::view(app))
                        .width(iced::FillPortion(app.ui.panes.detail)),
                    container(keymap_detail(app)).width(iced::FillPortion(app.ui.panes.sidebar)),
                ]
                .spacing(app.ui.spacing.m)
                .height(Fill)
            ]
            .spacing(app.ui.spacing.m)
            .height(Fill)
            .into()
        }
    })
    .into()
}
fn staged_edits(app: &Desktop) -> Element<'_, Message> {
    let descriptor = app.session.descriptor();
    let dirty = app.session.changes();
    if dirty.is_empty() {
        return text("Nothing staged").into();
    }
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
    edits.into()
}

fn keys(app: &Desktop) -> Element<'_, Message> {
    let keys = app
        .session
        .descriptor()
        .keys
        .iter()
        .filter(|key| key.visible)
        .collect();
    let labels =
        physical_board::labels_for_layer(app.session.descriptor(), app.session.draft(), &app.layer);
    if app.page == Page::Picture {
        let colors = super::picture::projected_colors(app).unwrap_or_default();
        return physical_board::colored_view_with_labels(
            &app.ui,
            keys,
            app.selected.clone(),
            colors,
            labels,
            |key| {
                Some(Message::Picture(super::picture::Message::Select(
                    key.id.clone(),
                )))
            },
        );
    }
    physical_board::view_with_labels(&app.ui, keys, app.selected.clone(), labels, |key| {
        Some(Message::SelectKey(key.id.clone()))
    })
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
        | Activity::ReadMacroCatalog { .. }
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
