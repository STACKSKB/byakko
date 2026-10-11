// SPDX-License-Identifier: GPL-3.0-or-later
//! Focus and navigation boundaries for the local keyboard tester.
use byakko_core::model::tester::{Input, InputKind};
use iced::{Event, keyboard, window};
use keyboard::key::{Code, Physical};

pub fn pauses(event: &Event) -> bool {
    matches!(event, Event::Window(window::Event::Unfocused))
        || matches!(event, Event::Keyboard(keyboard::Event::KeyPressed {
            physical_key: Physical::Code(Code::Tab), modifiers, ..
        }) if modifiers.shift())
}

pub fn input(event: &Event, at_us: u64) -> Option<Input> {
    let (code, kind) = match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            physical_key: Physical::Code(code),
            repeat,
            ..
        }) => (
            *code,
            if *repeat {
                InputKind::Repeat
            } else {
                InputKind::Press
            },
        ),
        Event::Keyboard(keyboard::Event::KeyReleased {
            physical_key: Physical::Code(code),
            ..
        }) => (*code, InputKind::Release),
        _ => return None,
    };
    Some(Input {
        usage: super::recording::key_usage(code)?,
        kind,
        at_us,
    })
}
