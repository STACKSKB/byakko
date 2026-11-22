//! The persistent keyboard workspace composes the approved feature layouts.
use crate::widget::focus_button::button;
use crate::{
    controller::discovery::{Availability, Endpoint},
    form::{
        appearance,
        application::{Closing, Message, Page},
        files, host, keymap, lighting, macros, picture, recording, tester,
    },
    view,
    widget::{
        keyboard,
        panels::{self, UiStyle},
    },
};
use byakko_core::{
    editor::Status,
    projection::tester as tester_projection,
    session::{Connection, Session},
};
use iced::{
    Color, Element, Fill, Length,
    widget::{column, container, responsive, row, scrollable, space, text},
};

#[derive(Clone, PartialEq)]
struct KeyOption {
    id: String,
    label: String,
}

impl std::fmt::Display for KeyOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Clone, Copy)]
pub struct View<'a> {
    pub session: &'a Session,
    pub appearance: &'a appearance::Form,
    pub presence: &'a Availability,
    pub selected_transport: Option<&'a str>,
    pub monitor_transports: bool,
    pub transport_scan_pending: bool,
    pub transport_opening: bool,
    #[cfg(target_os = "linux")]
    pub keyboard_access: Option<&'a crate::form::application::KeyboardAccess>,
    pub can_choose_transport: bool,
    pub confirm_transport_switch: bool,
    pub keys: &'a keymap::Form,
    pub macros: &'a macros::Form,
    pub lighting: &'a lighting::Form,
    pub host: &'a host::Form,
    pub host_preparing: bool,
    pub picture: &'a picture::Form,
    pub files: &'a files::Form,
    pub files_busy: bool,
    pub names_available: bool,
    pub tester: &'a tester::Form,
    pub recording_options: &'a recording::Options,
    pub recording_pending: bool,
    pub page: Page,
    pub closing: Closing,
    pub notice: &'a str,
    pub has_unsaved_changes: bool,
    pub style: &'a UiStyle,
}
impl View<'_> {
    fn editing_allowed(&self) -> bool {
        !self.session.blocks_editing()
            && !self.files_busy
            && !self.session.recording()
            && !self.recording_pending
            && !self.host_preparing
            && self.session.host().is_idle()
    }
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
    let base: Element<'a, Message> = responsive(move |size| {
        let wide = size.width >= input.style.key_sidebar_breakpoint;
        let brand = text("Byakko").size(22);
        let navigation = scrollable(navigation(input))
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(4).scroller_width(4),
            ))
            .width(Fill);
        let awaiting_choice = input.selected_transport.is_none()
            && matches!(input.presence, Availability::Choices { .. });
        let reconnect = nav_button(
            "Read / reconnect",
            true,
            (input.idle()
                && !input.has_unsaved_changes
                && !input.transport_opening
                && !awaiting_choice)
                .then_some(Message::Read),
        );
        let appearance = nav_button(
            "Appearance",
            false,
            input
                .editing_allowed()
                .then_some(Message::Appearance(appearance::Message::Open)),
        );
        let header: Element<'a, Message> = if wide {
            row![brand, reconnect, navigation, appearance]
                .spacing(24)
                .align_y(iced::Alignment::Center)
                .height(Length::Fixed(60.0))
                .padding(iced::Padding::default().left(22).right(22))
                .into()
        } else {
            column![
                row![brand, reconnect, space::horizontal(), appearance]
                    .spacing(4)
                    .align_y(iced::Alignment::Center),
                navigation
            ]
            .height(Length::Fixed(76.0))
            .padding(iced::Padding::default().left(10).right(10))
            .into()
        };
        let mut content = column![container(header).style(|_| container::Style {
            background: Some(iced::Background::Color(Color::BLACK)),
            text_color: Some(Color::WHITE),
            ..Default::default()
        })];
        if input.monitor_transports {
            content = content.push(transport_controls(input));
        }
        content.push(workspace(input, wide)).height(Fill).into()
    })
    .width(Fill)
    .height(Fill)
    .into();
    let confirm_close = input.closing == Closing::ConfirmDiscard;
    let blocked = input.appearance.open || confirm_close || input.confirm_transport_switch;
    let base = crate::widget::tester_capture::capture(
        base,
        !blocked && input.page == Page::Tester && input.tester.active(),
        Message::Tester(crate::form::tester::Message::Pause),
    );
    let layers = iced::widget::stack![crate::widget::inert::guard(base, blocked)];
    if input.appearance.open && !confirm_close && !input.confirm_transport_switch {
        return layers
            .push(
                container(iced::widget::opaque(
                    view::appearance::view(input.appearance, input.style).map(Message::Appearance),
                ))
                .padding(input.style.spacing.page_padding)
                .center_x(Fill)
                .center_y(Fill),
            )
            .into();
    }
    let (title, detail, keep, discard, label) = if confirm_close {
        (
            "Discard unsaved changes?",
            "Your unsaved changes will be lost when Byakko closes.",
            Message::KeepEditing,
            Message::Discard,
            "Discard & close",
        )
    } else if input.confirm_transport_switch {
        (
            "Discard changes and switch connections?",
            "Your unsaved edits for the selected connection will be discarded.",
            Message::CancelTransportSwitch,
            Message::ConfirmTransportSwitch,
            "Discard & switch",
        )
    } else {
        return layers.into();
    };
    let dialog = container(
        column![
            text(title).size(input.style.type_scale.section_title),
            text(detail),
            row![
                button("Keep editing").on_press(keep),
                button(label).on_press(discard)
            ]
            .spacing(input.style.spacing.m),
        ]
        .spacing(input.style.spacing.l),
    )
    .padding(input.style.spacing.page_padding)
    .style(container::bordered_box);
    layers
        .push(
            container(iced::widget::opaque(dialog))
                .center_x(Fill)
                .center_y(Fill),
        )
        .into()
}

fn navigation<'a>(input: View<'a>) -> iced::widget::Row<'a, Message> {
    let mut navigation = row![].spacing(2);
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
            "Diagnostics",
            Page::Archive,
            input.session.archive().is_some(),
        ),
        ("Tester", Page::Tester, true),
    ] {
        if available {
            navigation = navigation.push(nav_button(
                label,
                input.page == page || (page == Page::Lighting && input.page == Page::Picture),
                input.editing_allowed().then_some(Message::Page(page)),
            ));
        }
    }
    navigation
}

fn transport_controls(input: View<'_>) -> Element<'_, Message> {
    let controls = row![].spacing(8).align_y(iced::Alignment::Center);
    let controls = match input.presence {
        Availability::Ready(endpoint) if input.selected_transport.is_some() => {
            controls.push(transport_control(input, endpoint))
        }
        Availability::Choices { wired, receiver } => controls
            .push(transport_control(input, wired))
            .push(transport_control(input, receiver)),
        _ => controls,
    };
    let controls = if input.transport_scan_pending {
        controls.push(text("Checking connections…"))
    } else {
        controls.push(
            button("Find connections").on_press_maybe(
                input
                    .can_choose_transport
                    .then_some(Message::RefreshTransports),
            ),
        )
    };
    #[cfg(target_os = "linux")]
    if let Some(access) = input.keyboard_access {
        use crate::form::application::KeyboardAccess;
        let controls = controls.push(
            button("Set up keyboard access").on_press_maybe(
                (*access != KeyboardAccess::Pending && input.editing_allowed())
                    .then_some(Message::EnableKeyboardAccess),
            ),
        );
        let hint = match access {
            KeyboardAccess::Idle => {
                "Allow this desktop session to configure the Nia87. Linux may ask for administrator approval."
            }
            KeyboardAccess::Pending => "Waiting for Linux permission setup…",
            KeyboardAccess::Finished(message) => message.as_str(),
        };
        return container(column![controls, text(hint).size(13)].spacing(3))
            .padding([4, 10])
            .into();
    }
    container(controls).padding([4, 10]).into()
}

fn transport_control<'a>(input: View<'a>, endpoint: &Endpoint) -> Element<'a, Message> {
    if input.selected_transport == Some(endpoint.id.as_str()) {
        text(format!("{} (selected)", endpoint.label)).into()
    } else {
        button(endpoint.label)
            .on_press_maybe(
                (input.can_choose_transport && !input.transport_scan_pending)
                    .then(|| Message::ChooseTransport(endpoint.id.clone())),
            )
            .into()
    }
}

fn nav_button(
    label: &'static str,
    active: bool,
    message: Option<Message>,
) -> Element<'static, Message> {
    use iced::widget::button::Status;
    button(text(label).size(14))
        .padding([8, 12])
        .on_press_maybe(message)
        .style(move |_theme, status| {
            let enabled = status != Status::Disabled;
            let highlighted =
                enabled && (active || matches!(status, Status::Hovered | Status::Pressed));
            iced::widget::button::Style {
                background: highlighted
                    .then_some(iced::Background::Color(Color::from_rgb8(55, 55, 60))),
                text_color: if !enabled {
                    Color::from_rgb8(136, 136, 141)
                } else if highlighted {
                    Color::WHITE
                } else {
                    Color::from_rgb8(189, 189, 194)
                },
                border: iced::Border {
                    color: Color::from_rgb8(109, 101, 169),
                    width: if highlighted { 1.0 } else { 0.0 },
                    radius: 5.0.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

fn workspace<'a>(input: View<'a>, wide: bool) -> Element<'a, Message> {
    let idle = input.idle();
    let detail: Element<'a, Message> = if input.page == Page::Macros {
        macro_workspace(input)
    } else if input.page == Page::Keys {
        let editable = input.editing_allowed()
            && input.session.keymap().status() == &Status::Ready
            && matches!(input.session.connection(), Connection::Connected { .. });
        let assignment = view::keymap::view(
            input.keys,
            input.session,
            input.files,
            editable,
            idle,
            input.style,
        );
        if input.keys.pane == keymap::Pane::Remap {
            let catalog = view::catalog::view(
                &input.keys.catalog,
                &input.session.descriptor().actions,
                input.keys.selected_action(input.session.keymap()),
                editable && input.keys.can_assign(input.session.keymap()),
                idle,
                input.style,
            )
            .map(Message::Keys);
            column![
                container(panels::vertical_scroll(input.style, assignment))
                    .height(Length::Fixed(if wide { 150.0 } else { 120.0 })),
                catalog,
            ]
            .height(Fill)
            .into()
        } else {
            panels::vertical_scroll(input.style, assignment)
                .height(Fill)
                .into()
        }
    } else if input.page == Page::Tester {
        view::tester::view(input.tester, input.style).map(Message::Tester)
    } else {
        match input.page {
            Page::Lighting | Page::Picture => lighting_workspace(input),
            Page::Settings => match input.session.settings() {
                Some(editor) => view::settings::view(
                    editor,
                    !input.files_busy
                        && (!input.session.blocks_editing() || editor.submitted().is_some()),
                    idle && matches!(input.session.connection(), Connection::Connected { .. }),
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
            Page::Keys | Page::Macros | Page::Tester => unreachable!("handled workspace"),
        }
    };
    let inspector = container(detail)
        .padding(16)
        .width(Fill)
        .height(Fill)
        .style(|theme| container::Style {
            background: Some(iced::Background::Color(
                theme.extended_palette().background.weak.color,
            )),
            ..Default::default()
        });
    if wide {
        row![
            container(stage(input, true))
                .width(iced::Length::FillPortion(3))
                .height(Fill),
            inspector.width(iced::Length::FillPortion(2)),
        ]
        .height(Fill)
        .into()
    } else {
        column![
            container(stage(input, false)).height(Length::Fixed(237.0)),
            inspector,
        ]
        .height(Fill)
        .into()
    }
}

fn stage<'a>(input: View<'a>, wide: bool) -> Element<'a, Message> {
    responsive(move |size| {
        let portrait = !wide && size.width <= 650.0;
        let picker = || -> Element<'a, Message> {
            if !matches!(input.page, Page::Keys | Page::Macros) {
                return text("").into();
            }
            let choices: Vec<KeyOption> = input
                .session
                .descriptor()
                .keys
                .iter()
                .filter(|key| key.visible)
                .map(|key| KeyOption {
                    id: key.id.clone(),
                    label: key.label.clone(),
                })
                .collect();
            let selected = choices
                .iter()
                .find(|choice| Some(&choice.id) == input.keys.selected.as_ref())
                .cloned();
            crate::widget::controls::pick_list(choices, selected, |choice: KeyOption| {
                Message::Keys(keymap::Message::Key(choice.id))
            })
            .width(Length::Fixed(115.0))
            .into()
        };
        let layers: Element<'a, Message> = if matches!(input.page, Page::Keys | Page::Macros) {
            view::keymap::layers(
                input.keys,
                input.session,
                input.editing_allowed(),
                input.style,
            )
            .into()
        } else {
            text("").into()
        };
        let mut header = row![
            text(&input.session.descriptor().device_name).size(if wide { 32 } else { 22 }),
            space::horizontal(),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        if !wide && !portrait {
            header = header.push(picker());
        }
        header = header.push(layers);
        let header = header
            .padding(
                iced::Padding::default()
                    .left(if wide { 22 } else { 10 })
                    .right(if wide { 22 } else { 10 }),
            )
            .height(Length::Fixed(if wide { 68.0 } else { 40.0 }));
        let mut footer = row![status(input), space::horizontal()]
            .spacing(8)
            .align_y(iced::Alignment::Center);
        if wide || portrait {
            footer = footer.push(picker());
        }
        let footer = footer
            .padding(
                iced::Padding::default()
                    .left(if wide { 22 } else { 10 })
                    .right(if wide { 22 } else { 10 }),
            )
            .height(Length::Fixed(if wide {
                52.0
            } else if portrait {
                47.0
            } else {
                23.0
            }));
        container(column![header, board(input), footer].height(Fill))
            .height(Fill)
            .width(Fill)
            .style(|theme| container::Style {
                background: Some(iced::Background::Color(
                    theme.extended_palette().background.base.color,
                )),
                ..Default::default()
            })
            .into()
    })
    .height(Fill)
    .width(Fill)
    .into()
}

fn board<'a>(input: View<'a>) -> Element<'a, Message> {
    let descriptor = input.session.descriptor();
    let labels = keyboard::labels_for_layer(
        descriptor,
        input.session.keymap().draft(),
        if matches!(input.page, Page::Keys | Page::Macros) {
            &input.keys.layer
        } else {
            descriptor
                .layers
                .first()
                .map_or("", |layer| layer.id.as_str())
        },
        |action| {
            input
                .session
                .macros()
                .and_then(|editor| input.files.assignment_name(action, editor.capabilities()))
                .map(str::to_owned)
        },
    );
    if input.page == Page::Tester {
        let positions = tester_projection::layout_for(input.tester.layout);
        let usage_by_key: std::collections::BTreeMap<String, u16> = descriptor
            .keys
            .iter()
            .filter(|key| key.visible)
            .filter_map(|key| {
                positions
                    .iter()
                    .find(|position| {
                        (position.x - key.x).abs() < 0.01 && (position.y - key.y).abs() < 0.01
                    })
                    .and_then(|position| position.usage)
                    .map(|usage| (key.id.clone(), usage))
            })
            .collect();
        let colors = usage_by_key
            .iter()
            .filter_map(|(key, usage)| {
                if input.tester.test.held().any(|held| held == *usage) {
                    Some((key.clone(), [35, 112, 70]))
                } else if input.tester.test.tested().contains(usage) {
                    Some((key.clone(), [36, 79, 123]))
                } else {
                    None
                }
            })
            .collect();
        let selected = usage_by_key
            .iter()
            .find(|(_, usage)| Some(**usage) == input.tester.inspected)
            .map(|(key, _)| key.clone());
        return keyboard::colored_view_with_labels(
            input.style,
            descriptor.keys.iter().filter(|key| key.visible).collect(),
            selected,
            colors,
            labels,
            move |key| {
                usage_by_key
                    .get(&key.id)
                    .copied()
                    .map(|usage| Message::Tester(tester::Message::Inspect(usage)))
            },
        );
    }
    if matches!(input.page, Page::Lighting | Page::Picture)
        && lighting_mode(input.session, input.lighting) == Some(lighting::Mode::PerKey)
        && let Some(editor) = input.session.picture()
    {
        return view::picture::board(
            input.picture,
            descriptor,
            editor,
            !input.files_busy
                && (!input.session.blocks_editing() || editor.submitted().is_some())
                && picture_is_displayed(input.session),
            input.style,
            labels,
        )
        .map(Message::Picture);
    }
    keyboard::view_with_labels(
        input.style,
        descriptor.keys.iter().filter(|key| key.visible).collect(),
        matches!(input.page, Page::Keys | Page::Macros)
            .then(|| input.keys.selected.clone())
            .flatten(),
        matches!(input.page, Page::Keys | Page::Macros),
        labels,
        move |key| {
            (input.editing_allowed()
                && matches!(input.page, Page::Keys | Page::Macros)
                && descriptor.key_is_writable(&input.keys.layer, &key.id))
            .then(|| Message::Keys(keymap::Message::Key(key.id.clone())))
        },
    )
}

fn macro_workspace<'a>(input: View<'a>) -> Element<'a, Message> {
    let (Some(editor), Some(library)) = (input.session.macros(), input.session.macro_library())
    else {
        return text("Macros are unavailable for this keyboard.").into();
    };
    let idle = input.idle() && matches!(input.session.connection(), Connection::Connected { .. });
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
    let binding_actions: Vec<_> = editor
        .capabilities()
        .bindings
        .iter()
        .filter(|binding| binding.slot == editor.slot())
        .map(|binding| &binding.action)
        .collect();
    let bound_keys = input
        .session
        .keymap()
        .baseline()
        .map(|snapshot| {
            let binding_actions = &binding_actions;
            snapshot
                .bindings
                .iter()
                .flat_map(|(layer, keys)| {
                    keys.iter().filter_map(move |(key, action)| {
                        if binding_actions.contains(&action) {
                            Some(format!("{layer} · {key}"))
                        } else {
                            None
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let program = editor.draft();
    let phase = if input.recording_pending {
        view::recording::Phase::Waiting
    } else if input.session.recording() {
        view::recording::Phase::Recording {
            events: program.map_or(0, |program| program.events.len()),
        }
    } else {
        view::recording::Phase::Idle {
            empty: program.is_none_or(|program| program.events.is_empty()),
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
    view::macros::view(
        view::macros::View {
            form: input.macros,
            editor,
            descriptor: input.session.descriptor(),
            library,
            names: input.files,
            idle,
            editing_allowed: input.editing_allowed(),
            target: input
                .keys
                .target()
                .filter(|(layer, key)| input.session.descriptor().key_is_writable(layer, key)),
            bound_action: input.keys.selected_action(input.session.keymap()),
            bound_slots,
            bound_keys,
            assignment_ready: input.session.keymap().status() == &Status::Ready
                && !input.session.keymap().dirty(),
            recording: input.session.recording() || input.recording_pending,
            scanning: input.session.catalog_scanning(),
            style: input.style,
        },
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

pub(crate) fn lighting_mode(session: &Session, form: &lighting::Form) -> Option<lighting::Mode> {
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
    let idle = input.idle() && matches!(input.session.connection(), Connection::Connected { .. });
    let mode = lighting_mode(input.session, input.lighting);
    let editable = !input.files_busy
        && !input.host_preparing
        && input.session.host().is_idle()
        && (!input.session.blocks_editing()
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
    if matches!(mode, Some(lighting::Mode::PerKey | lighting::Mode::Host(_)))
        && let Some(editor) = input.session.lighting()
        && editor.status() != &Status::Ready
    {
        controls = controls.push(
            view::lighting::view(input.lighting, editor, editable, idle, input.style)
                .map(Message::Lighting),
        );
    }
    let detail = match mode {
        Some(lighting::Mode::PerKey) => input.session.picture().map(|editor| {
            let mut detail = column![].spacing(input.style.spacing.m);
            if let Some(lighting) = input.session.lighting()
                && lighting.draft().is_some_and(|setting| {
                    editor.capabilities().lighting_effect.as_ref() == Some(&setting.effect)
                })
            {
                detail = detail.push(
                    view::lighting::picture_controls(
                        lighting,
                        input.editing_allowed() && !editor.dirty() && editor.submitted().is_none(),
                        input.style,
                    )
                    .map(Message::Lighting),
                );
            }
            detail
                .push(
                    view::picture::view(
                        input.picture,
                        input.session.descriptor(),
                        editor,
                        !input.files_busy
                            && (!input.session.blocks_editing() || editor.submitted().is_some())
                            && picture_is_displayed(input.session),
                        idle,
                        input.style,
                    )
                    .map(Message::Picture),
                )
                .into()
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
    panels::vertical_scroll(input.style, controls)
        .height(Fill)
        .into()
}

/// Reserve a stable status area, including when there is no message. Long
/// diagnostics wrap and scroll here without moving the editor controls.
fn status<'a>(input: View<'a>) -> Element<'a, Message> {
    panels::vertical_scroll(input.style, text(notice(input).unwrap_or_default()))
        .height(Fill)
        .into()
}

pub(crate) fn notice(input: View<'_>) -> Option<String> {
    if input.closing == Closing::Waiting {
        return Some("Waiting for the operation before closing…".into());
    }
    match input.presence {
        Availability::Missing => {
            return Some(if input.selected_transport.is_some() {
                "Keyboard disconnected. Edits kept.".into()
            } else {
                "No keyboard connection found.".into()
            });
        }
        Availability::Ambiguous { count } => {
            return Some(format!(
                "{count} configuration endpoints found. Leave at most one wired USB connection and one 2.4 GHz receiver connected."
            ));
        }
        Availability::Error(_) => {
            return Some("Keyboard discovery failed. Try Read / reconnect.".into());
        }
        Availability::Choices { .. } if input.selected_transport.is_none() => {
            return Some("Choose a connection to read the keyboard.".into());
        }
        Availability::Choices { .. }
            if input
                .selected_transport
                .is_some_and(|id| !input.presence.contains(id)) =>
        {
            return Some("Selected connection unavailable. Choose a connection to switch.".into());
        }
        Availability::Ready(endpoint)
            if input.selected_transport.is_some_and(|id| id != endpoint.id) =>
        {
            return Some(format!(
                "Selected connection unavailable. Choose {} to switch.",
                endpoint.label
            ));
        }
        Availability::Unknown | Availability::Ready(_) | Availability::Choices { .. } => {}
    }
    if input.session.busy() {
        return (!input.notice.is_empty()).then(|| input.notice.into());
    }
    let attention = match input.page {
        Page::Keys => view::status::editor_attention("Keys", input.session.keymap().status()),
        Page::Macros => input
            .session
            .macros()
            .filter(|editor| {
                editor.baseline().is_some()
                    || !matches!(
                        editor.status(),
                        Status::Unverified {
                            problem: byakko_core::contract::Problem::ReadRequired
                        }
                    )
            })
            .and_then(|editor| view::status::editor_attention("Macro", editor.status()))
            .or_else(|| {
                input.session.macro_library().and_then(|library| {
                    library
                        .error()
                        .map(|_| "Macro discovery failed. Retry scan.".into())
                })
            }),
        Page::Lighting | Page::Picture => input
            .session
            .lighting()
            .and_then(|editor| view::status::editor_attention("Lighting", editor.status()))
            .or_else(|| {
                (lighting_mode(input.session, input.lighting) == Some(lighting::Mode::PerKey))
                    .then(|| {
                        input.session.picture().and_then(|editor| {
                            view::status::editor_attention("Colors", editor.status())
                        })
                    })
                    .flatten()
            }),
        Page::Settings => input
            .session
            .settings()
            .and_then(|editor| view::status::editor_attention("Settings", editor.status())),
        Page::Archive | Page::Tester => None,
    };
    attention
        .or_else(|| (!input.notice.is_empty()).then(|| input.notice.into()))
        .or_else(|| view::status::editor_attention("Keys", input.session.keymap().status()))
        .or_else(|| {
            matches!(input.presence, Availability::Unknown)
                .then(|| "Looking for a keyboard…".into())
        })
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
