// SPDX-License-Identifier: GPL-3.0-or-later
//! Standard Iced button styling and pointer behavior with shared keyboard focus.
use iced::{Element, Length, Padding, Theme, widget::button as iced_button};

pub fn button<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
) -> Button<'a, Message> {
    Button {
        inner: iced_button(content),
        on_press: None,
    }
}

pub struct Button<'a, Message> {
    inner: iced_button::Button<'a, Message>,
    on_press: Option<Message>,
}

impl<'a, Message: Clone + 'a> Button<'a, Message> {
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.inner = self.inner.height(height);
        self
    }
    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.inner = self.inner.padding(padding);
        self
    }
    pub fn on_press(mut self, message: Message) -> Self {
        self.on_press = Some(message.clone());
        self.inner = self.inner.on_press(message);
        self
    }
    pub fn on_press_maybe(mut self, message: Option<Message>) -> Self {
        self.on_press = message.clone();
        self.inner = self.inner.on_press_maybe(message);
        self
    }
    pub fn style(
        mut self,
        style: impl Fn(&Theme, iced_button::Status) -> iced_button::Style + 'a,
    ) -> Self {
        self.inner = self.inner.style(style);
        self
    }
}

impl<'a, Message: Clone + 'a> From<Button<'a, Message>> for Element<'a, Message> {
    fn from(button: Button<'a, Message>) -> Self {
        match button.on_press {
            Some(message) => super::focus_control::toggle(button.inner, message, true),
            None => button.inner.into(),
        }
    }
}
