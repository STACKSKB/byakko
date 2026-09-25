//! Capability-driven scalar settings; the view knows no firmware fields.
use super::{Desktop, Message as AppMessage};
use crate::panels;
use byakko_core::settings::{
    self, Content, Edit, Field, Kind, Value,
    editor::{Editor, Status},
};
use iced::{
    Element, Fill, Length,
    widget::{button, checkbox, column, container, row, scrollable, slider, text},
};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct Pending {
    queued: BTreeMap<String, Value>,
    in_flight: Option<Edit>,
    due: Option<Instant>,
    blocked: bool,
}

impl Pending {
    pub(super) fn has_queued(&self) -> bool {
        !self.queued.is_empty() || self.in_flight.is_some()
    }
    pub(super) fn has_pending(&self) -> bool {
        !self.blocked && self.has_queued()
    }
    pub(super) fn ready(&self, now: Instant) -> bool {
        !self.blocked && !self.queued.is_empty() && self.due.is_none_or(|due| now >= due)
    }
    pub(super) fn blocked(&self) -> bool {
        self.blocked
    }
    pub(super) fn retry(&mut self) {
        self.blocked = false;
        self.due = None;
    }
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
    pub(super) fn queue(&mut self, edit: Edit, now: Instant, delay: Duration) {
        self.queued.insert(edit.id, edit.value);
        self.due = Some(now + delay);
    }
    #[cfg(test)]
    pub(super) fn queued_value(&self, id: &str) -> Option<&Value> {
        self.queued.get(id)
    }
    pub(super) fn begin(&mut self, edit: Edit) {
        self.queued.remove(&edit.id);
        self.in_flight = Some(edit);
    }
    pub(super) fn reconcile(&mut self, status: &Status) {
        let Some(edit) = self.in_flight.take() else {
            return;
        };
        if status != &Status::Ready {
            self.queued.entry(edit.id).or_insert(edit.value);
            self.blocked = true;
        }
    }
    fn projected(&self, editor: &Editor) -> Option<BTreeMap<String, Value>> {
        let mut values = editor.draft()?.clone();
        for (id, value) in &self.queued {
            values.insert(id.clone(), value.clone());
        }
        Some(values)
    }
}

#[derive(Clone, Debug)]
pub(super) enum Message {
    Read,
    #[cfg(test)]
    Apply,
    #[cfg(test)]
    Edit(Edit),
    Live(Edit),
    Retry,
}

impl Desktop {
    pub(super) fn update_settings(&mut self, message: Message) {
        if let Message::Live(edit) = message {
            let result = self
                .session
                .settings()
                .ok_or("Settings are unavailable".to_owned())
                .and_then(|editor| settings::validate_value(editor.capabilities(), &edit))
                .and_then(|_| {
                    self.session
                        .settings()
                        .and_then(Editor::draft)
                        .ok_or("Read settings before editing".into())
                        .map(|_| ())
                });
            match result {
                Ok(()) => {
                    self.live_settings
                        .queue(edit, Instant::now(), self.config.short_edit_delay);
                    self.notice = None;
                    self.flush_live_settings();
                }
                Err(reason) => self.notice = Some(reason),
            }
            return;
        }
        if self.busy() {
            return;
        }
        match message {
            Message::Read => {
                let request = self.session.request_settings_read();
                self.submit(request);
            }
            #[cfg(test)]
            Message::Apply => {
                let request = self.session.request_setting_apply();
                self.submit(request);
            }
            #[cfg(test)]
            Message::Edit(edit) => self.notice = self.session.edit_setting(edit).err(),
            Message::Retry => {
                if !self.refresh_connection() {
                    return;
                }
                self.live_settings.retry();
                if let Err(reason) = self.session.revert_settings() {
                    self.notice = Some(reason);
                    return;
                }
                let request = self.session.request_settings_read();
                self.submit(request);
            }
            Message::Live(_) => unreachable!(),
        }
    }

    pub(crate) fn flush_live_settings(&mut self) -> bool {
        if self.busy() {
            return false;
        }
        let Some(editor) = self.session.settings() else {
            self.live_settings.clear();
            return false;
        };
        self.live_settings.reconcile(editor.status());
        if !self.live_settings.ready(Instant::now()) {
            return false;
        }
        if editor.status() != &Status::Ready {
            self.live_settings.blocked = true;
            return false;
        }
        let Some(values) = self.live_settings.projected(editor) else {
            return false;
        };
        let Some(baseline) = editor.baseline() else {
            return false;
        };
        let Content::Editable(original) = &baseline.content else {
            return false;
        };
        let Some((id, value)) = values
            .iter()
            .find(|(id, value)| original.get(*id) != Some(*value))
        else {
            self.live_settings.queued.clear();
            self.live_settings.due = None;
            return false;
        };
        let edit = Edit {
            id: id.clone(),
            value: value.clone(),
        };
        if let Err(reason) = self.session.edit_setting(edit.clone()) {
            self.notice = Some(reason);
            self.live_settings.blocked = true;
            return false;
        }
        match self.session.request_setting_apply() {
            Ok(command) => {
                self.live_settings.begin(edit);
                self.submit(Ok(command));
                true
            }
            Err(reason) => {
                self.notice = Some(reason);
                self.live_settings.blocked = true;
                false
            }
        }
    }
}

pub(super) fn view(app: &Desktop) -> Element<'_, AppMessage> {
    let Some(editor) = app.session.settings() else {
        return text("Settings are unavailable on this device").into();
    };
    let editable = editor.draft().is_some() && !matches!(editor.status(), Status::Unloaded);
    let mut content = column![
        row![
            toolbar(app, editor, editable),
            text(status(app, editor).unwrap_or_else(|| " ".into()))
        ]
        .spacing(app.ui.spacing.s)
    ]
    .spacing(app.ui.spacing.s);
    let Some(draft) = app.live_settings.projected(editor) else {
        if let Some(snapshot) = editor.baseline()
            && let Content::Opaque { reason } = &snapshot.content
        {
            content = content.push(text(reason));
        }
        return content.into();
    };
    let controls = scrollable(settings_grid(
        &app.ui,
        &editor.capabilities().fields,
        &draft,
        editable,
    ))
    .height(Fill);
    content.push(controls).height(Fill).into()
}

fn settings_grid(
    style: &panels::UiStyle,
    fields: &[Field],
    draft: &BTreeMap<String, Value>,
    editable: bool,
) -> Element<'static, AppMessage> {
    let mut grid = column!().spacing(style.spacing.xs).width(Fill);
    for field in fields {
        let Some(value) = draft.get(&field.id) else {
            continue;
        };
        grid = grid.push(
            container(
                row![
                    text(field.label.clone()).width(Length::Fixed(style.fields.regular as f32)),
                    control(style, field, value, editable)
                ]
                .spacing(style.spacing.s)
                .align_y(iced::Alignment::Center),
            )
            .padding(style.spacing.xs as u16)
            .width(Fill),
        );
    }
    grid.into()
}

fn toolbar<'a>(app: &Desktop, editor: &Editor, editable: bool) -> Element<'a, AppMessage> {
    let mut actions = row!().spacing(app.ui.spacing.s);
    if app.session.status() != &byakko_core::session::Status::Disconnected
        && (app.live_settings.blocked()
            || matches!(
                editor.status(),
                Status::Conflict { .. } | Status::Unverified { .. }
            ))
    {
        actions = actions.push(
            button("Reload & retry")
                .on_press_maybe((!app.busy()).then_some(AppMessage::Settings(Message::Retry))),
        );
    } else if !editable && app.session.status() != &byakko_core::session::Status::Disconnected {
        actions = actions.push(
            button("Read settings")
                .on_press_maybe((!app.busy()).then_some(AppMessage::Settings(Message::Read))),
        );
    }
    actions.into()
}

fn status(app: &Desktop, editor: &Editor) -> Option<String> {
    if app.busy() {
        return Some(super::view::status(app));
    }
    match editor.status() {
        Status::Unloaded | Status::Ready => None,
        Status::Conflict { .. } => {
            Some("Settings changed on the device. Reload to continue.".into())
        }
        Status::Unverified { problem } => Some(super::view::problem_label(problem)),
    }
}

fn control(
    style: &panels::UiStyle,
    field: &Field,
    value: &Value,
    editable: bool,
) -> Element<'static, AppMessage> {
    match (&field.kind, value) {
        (Kind::Toggle, Value::Toggle(current)) => {
            let id = field.id.clone();
            checkbox(*current)
                .label(if *current { "Enabled" } else { "Disabled" })
                .on_toggle_maybe(editable.then_some(move |enabled| {
                    AppMessage::Settings(Message::Live(Edit {
                        id: id.clone(),
                        value: Value::Toggle(enabled),
                    }))
                }))
                .into()
        }
        (
            Kind::Number {
                min,
                max,
                step,
                disabled_zero,
                ..
            },
            Value::Number(current),
        ) => {
            let mut controls = row![
                text(display_value(field, value)).width(Length::Fixed(style.fields.compact as f32))
            ]
            .spacing(style.spacing.s)
            .align_y(iced::Alignment::Center);
            if editable {
                let id = field.id.clone();
                let disabled_position = u32::from(*max) + u32::from(*step);
                let max_value = u32::from(*max);
                controls = controls.push(
                    slider(
                        u32::from(*min)..=if *disabled_zero {
                            disabled_position
                        } else {
                            max_value
                        },
                        if *disabled_zero && *current == 0 {
                            disabled_position
                        } else {
                            u32::from(*current)
                        },
                        move |number| {
                            AppMessage::Settings(Message::Live(Edit {
                                id: id.clone(),
                                value: Value::Number(if number > max_value {
                                    0
                                } else {
                                    number as u16
                                }),
                            }))
                        },
                    )
                    .step(u32::from(*step))
                    .width(Length::Fixed(style.fields.regular as f32)),
                );
            }
            controls.into()
        }
        _ => text("Setting value does not match its capability").into(),
    }
}

fn display_value(field: &Field, value: &Value) -> String {
    match (&field.kind, value) {
        (Kind::Toggle, Value::Toggle(true)) => "Enabled".into(),
        (Kind::Toggle, Value::Toggle(false)) => "Disabled".into(),
        (
            Kind::Number {
                disabled_zero: true,
                ..
            },
            Value::Number(0),
        ) => "Disabled".into(),
        (Kind::Number { unit, .. }, Value::Number(value)) => format!("{value} {unit}"),
        _ => "Unknown value".into(),
    }
}
