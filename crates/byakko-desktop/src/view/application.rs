//! Application presentation borrows the core read model and local forms.
use crate::{
    form::{
        application::{Closing, Message, Page},
        keymap, lighting, macros, picture, recording, settings,
    },
    view,
    widget::panels::UiStyle,
};
use byakko_core::{
    editor::Status,
    session::{Connection, Session},
};
use iced::{
    Element, Fill,
    widget::{button, column, container, row, text},
};

/// Borrowed render inputs, assembled for a frame; no effects or owned state.
pub struct View<'a> {
    pub session: &'a Session,
    pub keys: &'a keymap::Form,
    pub macros: &'a macros::Form,
    pub lighting: &'a lighting::Form,
    pub picture: &'a picture::Form,
    pub settings: &'a settings::Form,
    pub recording_options: &'a recording::Options,
    pub recording_pending: bool,
    pub page: Page,
    pub closing: Closing,
    pub notice: &'a str,
    pub style: &'a UiStyle,
}
pub fn view<'a>(input: View<'a>) -> Element<'a, Message> {
    let idle = !input.session.busy() && !input.session.recording() && !input.recording_pending;
    let editable = idle
        && matches!(input.session.connection(), Connection::Connected { .. })
        && input.session.keymap().status() == &Status::Ready;
    let toolbar = row![
        button("Assignments").on_press_maybe(idle.then_some(Message::Page(Page::Keys))),
        button("Macros").on_press_maybe(
            (idle && input.session.macros().is_some()).then_some(Message::Page(Page::Macros))
        ),
        button("Lighting").on_press_maybe(
            (idle && input.session.lighting().is_some()).then_some(Message::Page(Page::Lighting))
        ),
        button("Key colors").on_press_maybe(
            (idle && input.session.picture().is_some()).then_some(Message::Page(Page::Picture))
        ),
        button("Settings").on_press_maybe(
            (idle && input.session.settings().is_some()).then_some(Message::Page(Page::Settings))
        ),
        button("Read / reconnect").on_press_maybe(idle.then_some(Message::Read)),
        button("Save assignments")
            .on_press_maybe((editable && input.session.keymap().dirty()).then_some(Message::Save)),
        button("Revert")
            .on_press_maybe((idle && input.session.keymap().dirty()).then_some(Message::Revert)),
    ]
    .spacing(input.style.spacing.s);
    let status = if input.closing == Closing::Waiting {
        "Waiting for the device operation before closing…"
    } else if input.session.busy() {
        "Working…"
    } else {
        input.notice
    };
    let base: Element<'_, Message> = container(
        column![
            text(&input.session.descriptor().device_name).size(input.style.type_scale.page_title),
            toolbar,
            text(status),
            feature_view(&input, editable),
        ]
        .spacing(input.style.spacing.m),
    )
    .padding(input.style.spacing.page_padding)
    .max_width(input.style.workspace_width)
    .center_x(Fill)
    .height(Fill)
    .into();
    if input.closing != Closing::ConfirmDiscard {
        return base;
    }
    let dialog = container(
        column![
            text("Discard unsaved changes?").size(input.style.type_scale.section_title),
            row![
                button("Keep editing").on_press(Message::KeepEditing),
                button("Discard & close").on_press(Message::Discard)
            ]
            .spacing(input.style.spacing.m),
        ]
        .spacing(input.style.spacing.l),
    )
    .padding(input.style.spacing.panel_padding)
    .style(container::bordered_box);
    iced::widget::stack![
        base,
        container(iced::widget::opaque(dialog))
            .center_x(Fill)
            .center_y(Fill)
    ]
    .into()
}

fn feature_view<'a>(input: &View<'a>, editable: bool) -> Element<'a, Message> {
    if (input.session.recording() || input.recording_pending)
        && let Some(editor) = input.session.macros()
    {
        let program = editor.draft();
        let phase = if input.recording_pending {
            view::recording::Phase::Waiting
        } else {
            view::recording::Phase::Recording {
                events: program.map_or(0, |program| program.events.len()),
            }
        };
        let mut content = column![
            view::keymap::workspace(
                input.keys,
                input.session.descriptor(),
                input.session.keymap(),
                false,
                input.style
            )
            .map(Message::Keys),
            view::recording::controls(input.recording_options, phase, input.style)
                .map(Message::Record),
        ]
        .spacing(input.style.spacing.m);
        if let Some(program) = program {
            content =
                content.push(view::recording::preview(program, input.style).map(Message::Record));
        }
        return content.height(Fill).into();
    }
    match input.page {
        Page::Keys => view::keymap::view(
            input.keys,
            input.session.descriptor(),
            input.session.keymap(),
            editable,
            input.style,
        )
        .map(Message::Keys),
        Page::Macros => match (input.session.macros(), input.session.macro_library()) {
            (Some(editor), Some(library)) => column![
                view::keymap::workspace(
                    input.keys,
                    input.session.descriptor(),
                    input.session.keymap(),
                    true,
                    input.style
                )
                .map(Message::Keys),
                view::recording::controls(
                    input.recording_options,
                    view::recording::Phase::Idle {
                        editable: !input.session.busy()
                            && editor.status() == &Status::Ready
                            && editor.draft().is_some_and(|program| editor
                                .capabilities()
                                .editable_repeat_counts
                                .contains(&program.repeat_count)),
                    },
                    input.style
                )
                .map(Message::Record),
                view::macros::view(
                    input.macros,
                    editor,
                    library,
                    !input.session.busy(),
                    input.keys.target(),
                    input.session.catalog_scanning(),
                    input.style
                )
                .map(Message::Macros),
            ]
            .spacing(input.style.spacing.m)
            .height(Fill)
            .into(),
            _ => text("Macros are unavailable for this keyboard.").into(),
        },
        Page::Lighting => match input.session.lighting() {
            Some(editor) => column![
                view::keymap::workspace(
                    input.keys,
                    input.session.descriptor(),
                    input.session.keymap(),
                    false,
                    input.style
                )
                .map(Message::Keys),
                view::lighting::view(
                    input.lighting,
                    editor,
                    !input.session.busy() || editor.submitted().is_some(),
                    !input.session.busy(),
                    input.style
                )
                .map(Message::Lighting),
            ]
            .spacing(input.style.spacing.m)
            .height(Fill)
            .into(),
            None => text("Lighting is unavailable for this keyboard.").into(),
        },
        Page::Picture => match input.session.picture() {
            Some(editor) => view::picture::view(
                input.picture,
                input.session.descriptor(),
                editor,
                (!input.session.busy() || editor.submitted().is_some())
                    && picture_is_displayed(input.session),
                !input.session.busy(),
                input.style,
            )
            .map(Message::Picture),
            None => text("Per-key colors are unavailable for this keyboard.").into(),
        },
        Page::Settings => match input.session.settings() {
            Some(editor) => column![
                view::keymap::workspace(
                    input.keys,
                    input.session.descriptor(),
                    input.session.keymap(),
                    false,
                    input.style
                )
                .map(Message::Keys),
                view::settings::view(
                    input.settings,
                    editor,
                    !input.session.busy() || editor.submitted().is_some(),
                    !input.session.busy(),
                    input.style
                )
                .map(Message::Settings),
            ]
            .spacing(input.style.spacing.m)
            .height(Fill)
            .into(),
            None => text("Settings are unavailable for this keyboard.").into(),
        },
    }
}

pub(crate) fn picture_is_displayed(session: &Session) -> bool {
    session.picture().is_some_and(|picture| {
        picture
            .capabilities()
            .lighting_effect
            .as_ref()
            .is_none_or(|effect| {
                session.lighting().is_some_and(|lighting| {
                    lighting.status() == &Status::Ready
                        && !lighting.dirty()
                        && lighting
                            .draft()
                            .is_some_and(|setting| setting.effect == *effect)
                })
            })
    })
}
