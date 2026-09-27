//! The persistent keyboard workspace composes the approved feature layouts.
use crate::{
    controller::discovery::Availability,
    form::{
        application::{Closing, Message, Page},
        files, host, keymap, lighting, macros, picture, recording,
    },
    view,
    widget::{
        keyboard,
        panels::{self, UiStyle},
    },
};
use byakko_core::{
    editor::Status,
    session::{Connection, Session},
};
use iced::{
    Element, Fill,
    widget::{button, column, container, responsive, row, space, text},
};

#[derive(Clone, Copy)]
pub struct View<'a> {
    pub session: &'a Session,
    pub presence: &'a Availability,
    pub keys: &'a keymap::Form,
    pub macros: &'a macros::Form,
    pub lighting: &'a lighting::Form,
    pub host: &'a host::Form,
    pub host_preparing: bool,
    pub picture: &'a picture::Form,
    pub files: &'a files::Form,
    pub files_busy: bool,
    pub names_available: bool,
    pub recording_options: &'a recording::Options,
    pub recording_pending: bool,
    pub page: Page,
    pub closing: Closing,
    pub notice: &'a str,
    pub style: &'a UiStyle,
}
impl View<'_> {
    fn idle(&self) -> bool {
        !self.session.busy()
            && !self.files_busy
            && !self.session.recording()
            && !self.recording_pending
            && !self.host_preparing
            && self.session.host().is_idle()
    }
}

pub fn view<'a>(input: View<'a>) -> Element<'a, Message> {
    let idle = input.idle();
    let mut navigation = row![].spacing(input.style.spacing.s);
    for (label, page, available) in [
        ("Keys", Page::Keys, true),
        ("Macros", Page::Macros, input.session.macros().is_some()),
        (
            "Lighting",
            Page::Lighting,
            input.session.lighting().is_some() || input.session.picture().is_some(),
        ),
        (
            "Settings",
            Page::Settings,
            input.session.settings().is_some(),
        ),
        (
            "Diagnostic capture",
            Page::Archive,
            input.session.archive().is_some(),
        ),
    ] {
        if available {
            navigation = navigation.push(panels::selectable_button(
                input.style,
                label,
                input.page == page || (page == Page::Lighting && input.page == Page::Picture),
                idle.then_some(Message::Page(page)),
            ));
        }
    }
    if !matches!(input.session.connection(), Connection::Connected { .. })
        || input.session.requires_manual_read()
        || matches!(input.session.keymap().status(), Status::Unverified { .. })
    {
        navigation = navigation
            .push(button("Read / reconnect").on_press_maybe(idle.then_some(Message::Read)));
    }
    let base: Element<'a, Message> = container(
        column![
            text(&input.session.descriptor().device_name).size(input.style.type_scale.page_title),
            navigation,
            responsive(move |size| workspace(
                input,
                size.width >= input.style.key_sidebar_breakpoint
            )),
        ]
        .spacing(input.style.spacing.m)
        .height(Fill),
    )
    .max_width(input.style.workspace_width)
    .width(Fill)
    .height(Fill)
    .into();
    let base = container(base)
        .padding(input.style.spacing.page_padding)
        .center_x(Fill)
        .width(Fill)
        .height(Fill)
        .into();
    if input.closing != Closing::ConfirmDiscard {
        return base;
    }
    let dialog = container(
        column![
            text("Discard unsaved changes?").size(input.style.type_scale.section_title),
            text("Your unsaved changes will be lost when Byakko closes."),
            row![
                button("Keep editing").on_press(Message::KeepEditing),
                button("Discard & close").on_press(Message::Discard)
            ]
            .spacing(input.style.spacing.m),
        ]
        .spacing(input.style.spacing.l),
    )
    .padding(input.style.spacing.page_padding)
    .style(container::bordered_box);
    iced::widget::stack![
        base,
        container(iced::widget::opaque(dialog))
            .center_x(Fill)
            .center_y(Fill)
    ]
    .into()
}

fn workspace<'a>(input: View<'a>, wide: bool) -> Element<'a, Message> {
    let idle = input.idle();
    if input.page == Page::Macros {
        return macro_workspace(input, wide);
    }
    let board = board(input);
    let mut details = column![].spacing(input.style.spacing.s);
    if let Some(notice) = notice(input) {
        details = details.push(text(notice));
    }
    if input.page == Page::Keys {
        let editable = idle
            && input.session.keymap().status() == &Status::Ready
            && matches!(input.session.connection(), Connection::Connected { .. });
        details = details.push(view::keymap::view(
            input.keys,
            input.session,
            input.files,
            editable,
            idle,
            input.style,
        ));
        let catalog = view::catalog::view(
            &input.keys.catalog,
            &input.session.descriptor().actions,
            input.keys.selected_action(input.session.keymap()),
            editable && input.keys.can_assign(input.session.keymap()),
            input.style,
        )
        .map(Message::Keys);
        if wide {
            return row![
                column![
                    board,
                    panels::vertical_scroll(input.style, details).height(Fill)
                ]
                .spacing(input.style.spacing.m)
                .width(Fill)
                .height(Fill),
                container(catalog)
                    .width(input.style.key_sidebar_width)
                    .height(Fill),
            ]
            .spacing(input.style.spacing.l)
            .height(Fill)
            .into();
        }
        return column![
            board,
            row![
                container(catalog).width(iced::Length::FillPortion(input.style.panes.detail)),
                panels::vertical_scroll(input.style, details)
                    .width(iced::Length::FillPortion(input.style.panes.sidebar)),
            ]
            .spacing(input.style.spacing.m)
            .height(Fill)
        ]
        .spacing(input.style.spacing.m)
        .height(Fill)
        .into();
    }
    details = details.push(match input.page {
        Page::Lighting | Page::Picture => lighting_workspace(input),
        Page::Settings => match input.session.settings() {
            Some(editor) => view::settings::view(
                editor,
                !input.files_busy && (!input.session.busy() || editor.submitted().is_some()),
                idle,
                input.style,
            )
            .map(Message::Settings),
            None => text("Settings are unavailable for this keyboard.").into(),
        },
        Page::Archive => match input.session.archive() {
            Some(capture) => view::archive::view(
                input.files,
                capture,
                idle,
                matches!(input.session.connection(), Connection::Connected { .. }),
                input.style,
            )
            .map(Message::Files),
            None => text("Diagnostic capture is unavailable for this keyboard.").into(),
        },
        Page::Keys | Page::Macros => unreachable!("handled workspace"),
    });
    let board = if wide {
        row![
            container(board).width(Fill),
            space().width(input.style.key_sidebar_width)
        ]
        .spacing(input.style.spacing.l)
        .into()
    } else {
        board
    };
    column![board, details.height(Fill)]
        .spacing(input.style.spacing.m)
        .height(Fill)
        .into()
}

fn board<'a>(input: View<'a>) -> Element<'a, Message> {
    let descriptor = input.session.descriptor();
    let labels = keyboard::labels_for_layer(
        descriptor,
        input.session.keymap().draft(),
        &input.keys.layer,
        |action| {
            input
                .session
                .macros()
                .and_then(|editor| input.files.assignment_name(action, editor.capabilities()))
                .map(str::to_owned)
        },
    );
    if matches!(input.page, Page::Lighting | Page::Picture)
        && lighting_mode(input.session, input.lighting, input.page) == Some(lighting::Mode::PerKey)
        && let Some(editor) = input.session.picture()
    {
        return view::picture::board(
            input.picture,
            descriptor,
            editor,
            !input.files_busy
                && (!input.session.busy() || editor.submitted().is_some())
                && picture_is_displayed(input.session),
            input.style,
            labels,
        )
        .map(Message::Picture);
    }
    keyboard::view_with_labels(
        input.style,
        descriptor.keys.iter().filter(|key| key.visible).collect(),
        input.keys.selected.clone(),
        labels,
        move |key| {
            input
                .idle()
                .then(|| Message::Keys(keymap::Message::Key(key.id.clone())))
        },
    )
}

fn macro_workspace<'a>(input: View<'a>, wide: bool) -> Element<'a, Message> {
    let (Some(editor), Some(library)) = (input.session.macros(), input.session.macro_library())
    else {
        return text("Macros are unavailable for this keyboard.").into();
    };
    let idle = input.idle();
    let bound_slots = editor
        .capabilities()
        .bindings
        .iter()
        .filter(|binding| {
            input.session.keymap().draft().is_some_and(|layers| {
                layers
                    .values()
                    .any(|keys| keys.values().any(|action| action == &binding.action))
            })
        })
        .map(|binding| binding.slot.clone())
        .collect();
    let program = editor.draft();
    let phase = if input.recording_pending {
        view::recording::Phase::Waiting
    } else if input.session.recording() {
        view::recording::Phase::Recording {
            events: program.map_or(0, |program| program.events.len()),
        }
    } else {
        view::recording::Phase::Idle {
            editable: idle
                && editor.status() == &Status::Ready
                && program.is_some_and(|program| {
                    editor
                        .capabilities()
                        .editable_repeat_counts
                        .contains(&program.repeat_count)
                }),
        }
    };
    let mut keyboard = column![board(input)].spacing(input.style.spacing.s);
    if let Some(notice) = notice(input) {
        keyboard = keyboard.push(text(notice));
    }
    view::macros::view(
        view::macros::View {
            form: input.macros,
            editor,
            descriptor: input.session.descriptor(),
            library,
            names: input.files,
            idle,
            target: input.keys.target(),
            bound_action: input.keys.selected_action(input.session.keymap()),
            bound_slots,
            scanning: input.session.catalog_scanning(),
            style: input.style,
            wide,
        },
        keyboard.into(),
        view::recording::controls(input.recording_options, phase, input.style).map(Message::Record),
        program
            .filter(|_| input.session.recording())
            .map(|program| {
                view::recording::preview(
                    program,
                    input.session.descriptor(),
                    editor.capabilities(),
                    input.style,
                )
                .map(Message::Record)
            }),
        view::files::name_controls(
            input.files,
            editor,
            idle,
            input.names_available,
            input.style,
        )
        .map(Message::Files),
        view::files::macros(
            input.files,
            editor,
            idle,
            input.names_available,
            input.style,
        )
        .map(Message::Files),
        Message::Macros,
    )
}

pub(crate) fn lighting_mode(
    session: &Session,
    form: &lighting::Form,
    page: Page,
) -> Option<lighting::Mode> {
    if page == Page::Picture {
        return Some(lighting::Mode::PerKey);
    }
    if let Some(mode) = &form.mode {
        return Some(mode.clone());
    }
    let effect = &session.lighting()?.draft()?.effect;
    if session
        .picture()
        .and_then(|editor| editor.capabilities().lighting_effect.as_ref())
        == Some(effect)
    {
        Some(lighting::Mode::PerKey)
    } else {
        Some(lighting::Mode::Onboard(effect.clone()))
    }
}

fn lighting_workspace<'a>(input: View<'a>) -> Element<'a, Message> {
    let idle = input.idle();
    let mode = lighting_mode(input.session, input.lighting, input.page);
    let editable = !input.files_busy
        && !input.host_preparing
        && input.session.host().is_idle()
        && (!input.session.busy()
            || input
                .session
                .lighting()
                .is_some_and(|editor| editor.submitted().is_some()));
    let mut controls = column![
        view::lighting::mode_selector(
            input.session.lighting(),
            input.session.picture(),
            mode.clone(),
            editable,
            input.style
        )
        .map(Message::Lighting)
    ]
    .spacing(input.style.spacing.m);
    let detail = match mode {
        Some(lighting::Mode::PerKey) => input.session.picture().map(|editor| {
            view::picture::view(
                input.picture,
                input.session.descriptor(),
                editor,
                !input.files_busy
                    && (!input.session.busy() || editor.submitted().is_some())
                    && picture_is_displayed(input.session),
                idle,
                input.style,
            )
            .map(Message::Picture)
        }),
        Some(lighting::Mode::Host(_)) => input.session.lighting().map(|editor| {
            view::host::view(
                input.host,
                editor,
                idle,
                input.host_preparing,
                input.session.host().phase(),
                input.style,
            )
            .map(Message::Host)
        }),
        _ => input.session.lighting().map(|editor| {
            view::lighting::view(input.lighting, editor, editable, idle, input.style)
                .map(Message::Lighting)
        }),
    };
    if let Some(detail) = detail {
        controls = controls.push(detail);
    }
    if idle && input.session.requires_manual_read() {
        controls = controls.push(button("Read / reconnect").on_press(Message::Read));
    }
    panels::vertical_scroll(input.style, controls)
        .height(Fill)
        .into()
}

fn notice(input: View<'_>) -> Option<String> {
    if input.closing == Closing::Waiting {
        return Some("Waiting for the operation before closing…".into());
    }
    if !input.notice.is_empty() {
        return Some(input.notice.into());
    }
    match input.presence {
        Availability::Unknown => Some("Looking for a keyboard…".into()),
        Availability::Missing => Some("Keyboard not connected. Your edits are kept.".into()),
        Availability::Ambiguous { count } => Some(format!(
            "Found {count} supported keyboards. Connect one at a time."
        )),
        Availability::Error(reason) => Some(format!("Could not find the keyboard: {reason}")),
        Availability::Ready { .. } => None,
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
