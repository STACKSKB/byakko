//! Per-key RGB controls over a device-neutral physical-key catalog.
use super::{Desktop, Message as AppMessage};
use crate::{
    control_widgets::{self, Choice},
    panels,
};
use byakko_core::picture::{
    self, Content, Edit,
    editor::{Editor, Status},
};
use iced::{
    Element, Fill,
    widget::{column, scrollable, text},
};

#[derive(Clone, Debug)]
pub(super) enum Message {
    Select(String),
    Read,
    Apply,
    Revert,
    Edit(Edit),
}

impl Desktop {
    pub(super) fn update_picture(&mut self, message: Message) {
        if self.busy() {
            return;
        }
        match message {
            Message::Select(key) => {
                if self
                    .session
                    .picture()
                    .is_some_and(|editor| editor.capabilities().keys.contains(&key))
                {
                    self.picture_selected = Some(key);
                }
            }
            Message::Read => {
                let request = self.session.request_picture_read();
                self.submit(request);
            }
            Message::Apply => {
                let request = self.session.request_picture_apply();
                self.submit(request);
            }
            Message::Revert => self.notice = self.session.revert_picture().err(),
            Message::Edit(edit) => self.notice = self.session.edit_picture(edit).err(),
        }
    }
}

pub(super) fn view(app: &Desktop) -> Element<'_, AppMessage> {
    let Some(editor) = app.session.picture() else {
        return text("Per-key colors are unavailable on this device").into();
    };
    let editable = !app.busy() && *editor.status() == Status::Ready && editor.draft().is_some();
    let mut content = column![toolbar(app, editor, editable), text(status(app, editor))]
        .spacing(app.ui.spacing.m);
    let Some(draft) = editor.draft() else {
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
    let color = draft[selected];
    let title = physical
        .iter()
        .find(|key| key.id == *selected)
        .map_or(selected.clone(), |key| key.label.clone());
    let entries: Vec<_> = physical
        .into_iter()
        .map(|key| {
            let rgb = draft[&key.id];
            Choice {
                label: format!(
                    "{} · #{:02X}{:02X}{:02X}",
                    key.label, rgb[0], rgb[1], rgb[2]
                ),
                selected: &key.id == selected,
                message: (!app.busy())
                    .then(|| AppMessage::Picture(Message::Select(key.id.clone()))),
            }
        })
        .collect();
    let style = &app.ui;
    let selected = selected.clone();
    let workbench = panels::split(
        style,
        move || {
            panels::panel(
                style,
                "Physical keys",
                scrollable(control_widgets::choices(
                    style,
                    "Available",
                    entries.clone(),
                ))
                .height(Fill)
                .into(),
            )
        },
        move || {
            let controls = column(picture::channels(color).into_iter().map(|channel| {
                let key = selected.clone();
                control_widgets::level(
                    style,
                    channel.label,
                    0..=u8::MAX as u16,
                    channel.value.into(),
                    editable.then_some(move |value| {
                        AppMessage::Picture(Message::Edit(Edit::Channel {
                            key: key.clone(),
                            channel: channel.channel,
                            value: u8::try_from(value).expect("byte channel range"),
                        }))
                    }),
                )
            }))
            .spacing(style.spacing.l);
            panels::panel(
                style,
                title.clone(),
                column![
                    text(format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])),
                    controls
                ]
                .spacing(style.spacing.m)
                .into(),
            )
        },
    );
    content.push(workbench).height(Fill).into()
}

fn toolbar<'a>(app: &Desktop, editor: &Editor, editable: bool) -> Element<'a, AppMessage> {
    control_widgets::transaction_toolbar(
        &app.ui,
        "Read colors",
        (!app.busy()).then_some(AppMessage::Picture(Message::Read)),
        (!app.busy() && editor.dirty()).then_some(AppMessage::Picture(Message::Revert)),
        (editable && editor.dirty()).then_some(AppMessage::Picture(Message::Apply)),
        format!("{} staged", editor.changes().len()),
    )
}

fn status(app: &Desktop, editor: &Editor) -> String {
    if app.busy() {
        return super::view::status(app);
    }
    match editor.status() {
        Status::Unloaded => "Read the stored colors to begin".into(),
        Status::Ready => "Readback verified · edits are staged until applied".into(),
        Status::Conflict { .. } => "Colors changed since the draft began. Draft retained; revert it, then read again to use device values.".into(),
        Status::Unverified { problem } => super::view::problem_label(problem),
    }
}
