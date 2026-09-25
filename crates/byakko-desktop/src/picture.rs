//! Per-key RGB controls over a device-neutral physical-key catalog.
use super::{Desktop, Message as AppMessage};
use byakko_core::picture::{
    self, Content, Edit,
    editor::{Editor, Status},
};
use iced::{
    Element, Fill,
    widget::{button, column, scrollable, text},
};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct Pending {
    queued: BTreeMap<String, [u8; 3]>,
    in_flight: Option<BTreeMap<String, [u8; 3]>>,
    due: Option<Instant>,
    pub(super) blocked: bool,
}

impl Pending {
    pub(crate) fn postpone(&mut self, now: Instant, delay: Duration) {
        self.due = Some(now + delay);
    }
    fn ready(&self, now: Instant) -> bool {
        !self.blocked && !self.queued.is_empty() && self.due.is_none_or(|due| now >= due)
    }
    pub(super) fn has_queued(&self) -> bool {
        !self.queued.is_empty() || self.in_flight.is_some()
    }
    pub(super) fn has_pending(&self) -> bool {
        !self.blocked && (!self.queued.is_empty() || self.in_flight.is_some())
    }

    fn queue(&mut self, key: String, color: [u8; 3], now: Instant, delay: Duration) {
        self.queued.insert(key, color);
        self.due = Some(now + delay);
    }

    fn reconcile(&mut self, status: &Status) {
        let Some(in_flight) = self.in_flight.take() else {
            return;
        };
        if status == &Status::Ready {
            return;
        }
        for (key, color) in in_flight {
            self.queued.entry(key).or_insert(color);
        }
        self.blocked = failed_status(status);
    }
}

fn failed_status(status: &Status) -> bool {
    matches!(
        status,
        Status::Conflict { .. }
            | Status::Unverified {
                problem: byakko_core::session::Problem::Read(_)
                    | byakko_core::session::Problem::Apply(_)
                    | byakko_core::session::Problem::InvalidApplyResult(_)
                    | byakko_core::session::Problem::ApplyReadbackMismatch
            }
    )
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug)]
pub(super) enum Message {
    Select(String),
    Read,
    Apply,
    Revert,
    Edit(Edit),
    Live(Edit),
    EnableLighting,
    Retry,
}

impl Desktop {
    pub(super) fn update_picture(&mut self, message: Message) {
        match message {
            Message::Live(edit) => {
                let Some(mut colors) = projected_colors(self) else {
                    self.notice = Some("Read the stored colors before editing".into());
                    return;
                };
                let (key, color) = match edit {
                    Edit::Color { key, color } => (key, color),
                    Edit::Channel {
                        key,
                        channel,
                        value,
                    } => {
                        let Some(mut color) = colors.remove(&key) else {
                            self.notice = Some("Unknown picture key".into());
                            return;
                        };
                        color[match channel {
                            picture::Channel::Red => 0,
                            picture::Channel::Green => 1,
                            picture::Channel::Blue => 2,
                        }] = value;
                        (key, color)
                    }
                };
                if !self
                    .session
                    .picture()
                    .is_some_and(|editor| editor.capabilities().keys.contains(&key))
                {
                    self.notice = Some("Unknown picture key".into());
                    return;
                }
                self.brush_color = Some(color);
                self.live_picture
                    .queue(key, color, Instant::now(), self.config.auto_save_delay);
                if !self.live_picture.blocked {
                    self.notice = None;
                }
                self.flush_live_picture();
            }
            Message::Select(key) => {
                if self
                    .session
                    .picture()
                    .is_some_and(|editor| editor.capabilities().keys.contains(&key))
                {
                    let color = self
                        .brush_color
                        .or_else(|| projected_colors(self)?.get(&key).copied());
                    self.picture_selected = Some(key.clone());
                    self.selected = self.picture_selected.clone();
                    if let Some(color) = color {
                        self.update_picture(Message::Live(Edit::Color { key, color }));
                    }
                }
            }
            _ if self.busy() => (),
            Message::EnableLighting => {
                if !can_enable_lighting(self) {
                    self.notice = Some("Read lighting and apply or revert existing drafts before enabling per-key lighting.".into());
                    return;
                }
                let effect = self
                    .session
                    .picture()
                    .and_then(|editor| editor.capabilities().lighting_effect.clone())
                    .expect("advertised by can_enable_lighting");
                let request = self
                    .session
                    .edit_lighting(byakko_core::lighting::Edit::Effect(effect))
                    .and_then(|_| self.session.request_lighting_apply());
                if let Ok(byakko_core::session::Command::ApplyLighting {
                    generation,
                    operation,
                    ..
                }) = &request
                {
                    self.picture_activation = Some((*generation, *operation));
                }
                self.submit(request);
            }
            Message::Read => {
                self.live_picture.blocked = false;
                let request = self.session.request_picture_read();
                self.submit(request);
            }
            Message::Apply => {
                let request = self.session.request_picture_apply();
                self.submit(request);
            }
            Message::Revert => {
                self.live_picture = Pending::default();
                self.notice = self.session.revert_picture().err();
            }
            Message::Edit(edit) => self.notice = self.session.edit_picture(edit).err(),
            Message::Retry => {
                if !self.refresh_connection() {
                    return;
                }
                self.live_picture.blocked = false;
                if self.session.picture().is_some_and(Editor::dirty) {
                    let _ = self.session.revert_picture();
                }
                let request = self.session.request_picture_read();
                self.submit(request);
            }
        }
    }

    pub(super) fn flush_live_picture(&mut self) -> bool {
        if self.busy() {
            return false;
        }
        let Some(editor) = self.session.picture() else {
            return false;
        };
        if self.live_picture.in_flight.is_some() {
            self.live_picture.reconcile(editor.status());
        }
        if self.picker_gesture == crate::color_picker::Gesture::Dragging
            || !self.live_picture.ready(Instant::now())
        {
            return false;
        }
        if failed_status(editor.status()) {
            self.live_picture.blocked = true;
            return false;
        }
        if editor.status() != &Status::Ready {
            let request = self.session.request_picture_read();
            self.submit(request);
            self.live_picture.blocked = !self.busy();
            return self.busy();
        }
        if !lighting_enabled(self) {
            if can_enable_lighting(self) {
                self.update_picture(Message::EnableLighting);
                return self.busy();
            }
            if !self.session.lighting().is_some_and(|editor| {
                matches!(
                    editor.status(),
                    byakko_core::lighting::editor::Status::Unloaded
                        | byakko_core::lighting::editor::Status::Unverified {
                            problem: byakko_core::session::Problem::ReadRequired
                        }
                )
            }) {
                self.live_picture.blocked = true;
                self.notice =
                    Some("Resolve the lighting change before sending per-key colors.".into());
                return false;
            }
            let request = self.session.request_lighting_read();
            self.submit(request);
            self.live_picture.blocked = !self.busy();
            return self.busy();
        }
        let queued = self.live_picture.queued.clone();
        for (key, color) in &queued {
            if let Err(reason) = self.session.edit_picture(Edit::Color {
                key: key.clone(),
                color: *color,
            }) {
                self.notice = Some(reason);
                self.live_picture.blocked = true;
                return false;
            }
        }
        if !self.session.picture().is_some_and(Editor::dirty) {
            self.live_picture.queued.clear();
            return false;
        }
        match self.session.request_picture_apply() {
            Ok(command) => {
                self.live_picture.in_flight = Some(std::mem::take(&mut self.live_picture.queued));
                self.submit(Ok(command));
                true
            }
            Err(reason) => {
                self.notice = Some(reason);
                self.live_picture.blocked = true;
                false
            }
        }
    }
}

pub(crate) fn projected_colors(app: &Desktop) -> Option<BTreeMap<String, [u8; 3]>> {
    let editor = app.session.picture()?;
    let mut colors = editor
        .draft()
        .cloned()
        .or_else(|| match &editor.baseline()?.content {
            Content::Editable(colors) => Some(colors.clone()),
            Content::Opaque { .. } => None,
        })?;
    colors.extend(
        app.live_picture
            .queued
            .iter()
            .map(|(key, color)| (key.clone(), *color)),
    );
    Some(colors)
}

pub(super) fn view(app: &Desktop) -> Element<'_, AppMessage> {
    let Some(editor) = app.session.picture() else {
        return text("Per-key colors are unavailable on this device").into();
    };
    let projected = projected_colors(app);
    let editable = projected.is_some() && app.session.status() != &super::Status::Disconnected;
    let mut content = column![text(status(app, editor))].spacing(app.ui.spacing.m);
    if self_needs_retry(app, editor) {
        content = content.push(
            button("Retry color read")
                .on_press_maybe((!app.busy()).then_some(AppMessage::Picture(Message::Retry))),
        );
    }
    let Some(draft) = projected.as_ref() else {
        if let Some(snapshot) = editor.baseline()
            && let Content::Opaque { reason } = &snapshot.content
        {
            content = content.push(text(reason));
        }
        return content.into();
    };
    let mut physical: Vec<_> = app
        .session
        .descriptor()
        .keys
        .iter()
        .filter(|key| editor.capabilities().keys.contains(&key.id))
        .collect();
    physical.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let selected = app
        .picture_selected
        .as_ref()
        .filter(|id| draft.contains_key(*id))
        .or_else(|| physical.first().map(|key| &key.id));
    let Some(selected) = selected else {
        return content.push(text("No color-capable physical keys")).into();
    };
    let color = app.brush_color.unwrap_or(draft[selected]);
    let selected_id = selected.clone();
    let picker = crate::color_picker::view(
        &app.ui,
        color,
        "picture-brush".into(),
        |event| AppMessage::Lighting(super::lighting::Message::PickerInteraction(event)),
        editable.then_some(move |color| {
            AppMessage::Picture(Message::Live(Edit::Color {
                key: selected_id.clone(),
                color,
            }))
        }),
    );
    content = content.push(picker);
    scrollable(content).height(Fill).into()
}

fn self_needs_retry(app: &Desktop, editor: &Editor) -> bool {
    app.live_picture.blocked
        || matches!(
            editor.status(),
            Status::Conflict { .. }
                | Status::Unverified {
                    problem: byakko_core::session::Problem::Read(_)
                        | byakko_core::session::Problem::Apply(_)
                        | byakko_core::session::Problem::InvalidApplyResult(_)
                        | byakko_core::session::Problem::ApplyReadbackMismatch
                }
        )
}

fn lighting_enabled(app: &Desktop) -> bool {
    let Some(effect) = app
        .session
        .picture()
        .and_then(|editor| editor.capabilities().lighting_effect.as_ref())
    else {
        return true;
    };
    app.session.lighting().and_then(|editor| editor.baseline()).is_some_and(|snapshot| matches!(&snapshot.content, byakko_core::lighting::Content::Editable(setting) if &setting.effect == effect))
}

fn can_enable_lighting(app: &Desktop) -> bool {
    !app.busy()
        && !lighting_enabled(app)
        && app.session.picture().is_some_and(|editor| !editor.dirty())
        && app.session.lighting().is_some_and(|editor| {
            editor.status() == &byakko_core::lighting::editor::Status::Ready && !editor.dirty()
        })
}

fn status(app: &Desktop, editor: &Editor) -> String {
    if app.live_picture.has_pending() {
        return "Updating keyboard colors…".into();
    }
    if app.busy() {
        return super::view::status(app);
    }
    match editor.status() {
        Status::Unloaded => "Loading stored colors".into(),
        Status::Ready => String::new(),
        Status::Conflict { .. } => "Colors changed since the draft began. Draft retained; revert it, then read again to use device values.".into(),
        Status::Unverified { problem } => super::view::problem_label(problem),
    }
}

#[cfg(test)]
mod batching_tests {
    use super::*;
    #[test]
    fn each_stroke_restarts_the_configured_idle_deadline() {
        let now = Instant::now();
        let delay = Duration::from_millis(420);
        let mut pending = Pending::default();
        pending.queue("a".into(), [1, 2, 3], now, delay);
        pending.queue(
            "b".into(),
            [1, 2, 3],
            now + Duration::from_millis(300),
            delay,
        );
        assert!(!pending.ready(now + delay));
        assert!(!pending.ready(now + Duration::from_millis(719)));
        assert!(pending.ready(now + Duration::from_millis(720)));
        assert_eq!(pending.queued.len(), 2);
    }
}
