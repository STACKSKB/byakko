//! Small, reusable layout primitives shared by desktop views.

use iced::widget::{button, column, container, responsive, row, text};
use iced::{Background, Color, Element, Fill, FillPortion, Size, Theme};

#[derive(Debug, Clone, Copy)]
pub struct Spacing {
    pub xs: u32,
    pub s: u32,
    pub m: u32,
    pub l: u32,
    pub page_padding: u16,
    pub panel_padding: u16,
    pub control_padding: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct TypeScale {
    pub body: u32,
    pub section_title: u32,
    pub page_title: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct PaneRatios {
    pub sidebar: u16,
    pub detail: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct FieldWidths {
    pub compact: u32,
    pub regular: u32,
}

/// Semantic colors supplied by the application style. `None` keeps the active
/// Iced theme in charge, so the default adapts to light and dark themes.
#[derive(Debug, Clone, Copy, Default)]
pub struct SemanticPalette {
    pub selected_background: Option<Color>,
    pub selected_text: Option<Color>,
}

/// The small set of visual decisions shared by desktop views.
///
/// Keeping geometry and semantic colors here lets pages share a visual system
/// without introducing a theme framework or coupling core data to Iced.
#[derive(Debug, Clone)]
pub struct UiStyle {
    pub initial_window: (f32, f32),
    pub theme: Theme,
    pub spacing: Spacing,
    pub type_scale: TypeScale,
    pub compact_breakpoint: f32,
    pub panes: PaneRatios,
    pub fields: FieldWidths,
    pub palette: SemanticPalette,
}

impl UiStyle {
    pub const DEFAULT: Self = Self {
        initial_window: (1140.0, 760.0),
        theme: Theme::Dark,
        spacing: Spacing {
            xs: 4,
            s: 8,
            m: 12,
            l: 20,
            page_padding: 20,
            panel_padding: 16,
            control_padding: 8,
        },
        type_scale: TypeScale {
            body: 14,
            section_title: 16,
            page_title: 22,
        },
        compact_breakpoint: 720.0,
        panes: PaneRatios {
            sidebar: 1,
            detail: 2,
        },
        fields: FieldWidths {
            compact: 120,
            regular: 240,
        },
        palette: SemanticPalette {
            selected_background: None,
            selected_text: None,
        },
    };
}

pub fn panel<'a, Message: 'a>(
    style: &UiStyle,
    title: impl Into<String>,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    container(
        column![
            text(title.into()).size(style.type_scale.section_title),
            content
        ]
        .spacing(style.spacing.m),
    )
    .width(Fill)
    .padding(style.spacing.panel_padding)
    .into()
}

pub fn split<'a, Message: 'a>(
    style: &UiStyle,
    sidebar: impl Fn() -> Element<'a, Message> + 'a,
    detail: impl Fn() -> Element<'a, Message> + 'a,
) -> Element<'a, Message> {
    let breakpoint = style.compact_breakpoint;
    let gap = style.spacing.m;
    let sidebar_ratio = style.panes.sidebar;
    let detail_ratio = style.panes.detail;

    responsive(move |size: Size| {
        if size.width < breakpoint {
            column![sidebar(), detail()].spacing(gap).width(Fill).into()
        } else {
            row![
                container(sidebar()).width(FillPortion(sidebar_ratio)),
                container(detail()).width(FillPortion(detail_ratio)),
            ]
            .spacing(gap)
            .width(Fill)
            .into()
        }
    })
    .into()
}

pub fn selectable_button<'a, Message: Clone + 'a>(
    style: &UiStyle,
    label: impl Into<String>,
    selected: bool,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let palette = style.palette;
    button(text(label.into()).size(style.type_scale.body))
        .padding(style.spacing.control_padding)
        .on_press_maybe(on_press)
        .style(move |theme: &Theme, status| {
            let mut visual = if selected {
                button::primary(theme, status)
            } else {
                button::secondary(theme, status)
            };
            if selected {
                if let Some(background) = palette.selected_background {
                    visual.background = Some(Background::Color(background));
                }
                if let Some(text) = palette.selected_text {
                    visual.text_color = text;
                }
            }
            visual
        })
        .into()
}
