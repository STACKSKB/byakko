//! Only unsubmitted form state lives here. Assignments belong to the core editor.
use crate::{panels::UiStyle, physical_board};
use byakko_core::{Change, Descriptor, keymap::Editor};
use iced::{
    Element, Fill,
    widget::{button, column, container, row, scrollable, text, text_input},
};

#[derive(Clone, Debug)]
pub enum Message {
    Layer(String),
    Key(String),
    Search(String),
    Assign(usize),
}

pub struct Form {
    layer: String,
    selected: Option<String>,
    search: String,
}

impl Form {
    pub fn target(&self) -> Option<(&str, &str)> {
        Some((&self.layer, self.selected.as_deref()?))
    }

    pub fn new(descriptor: &Descriptor) -> Self {
        Self {
            layer: descriptor.layers[0].id.clone(),
            selected: None,
            search: String::new(),
        }
    }

    pub fn update(&mut self, message: Message, descriptor: &Descriptor) -> Option<Change> {
        match message {
            Message::Layer(layer) => self.layer = layer,
            Message::Key(key) => self.selected = Some(key),
            Message::Search(search) => self.search = search,
            Message::Assign(index) => {
                return Some(Change {
                    layer: self.layer.clone(),
                    key: self.selected.clone()?,
                    action: descriptor.actions.get(index)?.action.clone(),
                });
            }
        }
        None
    }

    pub fn workspace<'a>(
        &'a self,
        descriptor: &'a Descriptor,
        editor: &'a Editor,
        style: &'a UiStyle,
    ) -> Element<'a, Message> {
        let layers = row(descriptor.layers.iter().map(|layer| {
            crate::panels::selectable_button(
                style,
                &layer.label,
                layer.id == self.layer,
                Some(Message::Layer(layer.id.clone())),
            )
        }))
        .spacing(style.spacing.s);
        let board = physical_board::view_with_labels(
            style,
            descriptor.keys.iter().filter(|key| key.visible).collect(),
            self.selected.clone(),
            physical_board::labels_for_layer(descriptor, editor.draft(), &self.layer),
            |key| Some(Message::Key(key.id.clone())),
        );
        column![layers, board].spacing(style.spacing.m).into()
    }

    pub fn view<'a>(
        &'a self,
        descriptor: &'a Descriptor,
        editor: &'a Editor,
        editable: bool,
        style: &'a UiStyle,
    ) -> Element<'a, Message> {
        let can_assign = editable
            && self.selected.as_ref().is_some_and(|id| {
                descriptor
                    .keys
                    .iter()
                    .any(|key| &key.id == id && key.writable)
            });
        let query = self.search.to_lowercase();
        let choices = column(
            descriptor
                .actions
                .iter()
                .enumerate()
                .filter(|(_, choice)| choice.label.to_lowercase().contains(&query))
                .map(|(index, choice)| {
                    button(text(&choice.label))
                        .on_press_maybe(can_assign.then_some(Message::Assign(index)))
                        .into()
                }),
        )
        .spacing(style.spacing.s);
        let assignments = column![
            text_input("Search assignments", &self.search).on_input(Message::Search),
            scrollable(choices).height(Fill)
        ]
        .spacing(style.spacing.s);
        column![
            self.workspace(descriptor, editor, style),
            container(assignments).width(Fill).height(Fill)
        ]
        .spacing(style.spacing.l)
        .into()
    }
}
