// SPDX-License-Identifier: GPL-3.0-or-later
//! Compact local keyboard observations; technical evidence stays in tooltips.
use crate::{
    form::tester::{Form, Message, Source},
    widget::panels::{self, UiStyle},
};
use byakko_core::projection::tester::{self, Legends, Size, Standard};
use iced::{
    Color, Element, Fill,
    widget::{
        button, column, container, pick_list, pin, responsive, row, scrollable, space, stack, text,
        tooltip,
    },
};
use std::fmt;

pub fn view<'a>(form: &'a Form, style: &'a UiStyle) -> Element<'a, Message> {
    let keys = tester::layout_for(form.layout);
    let observable = keys
        .iter()
        .filter_map(|key| key.usage)
        .collect::<std::collections::BTreeSet<_>>();
    let tested = observable.intersection(form.test.tested()).count();
    let outside = form.test.tested().difference(&observable).count();
    let mut coverage = format!(
        "Observed {tested}/{}   Held {}   Peak {}",
        observable.len(),
        form.test.held().count(),
        form.test.peak_held()
    );
    if outside != 0 {
        coverage.push_str(&format!("   Outside {outside}"));
    }
    let last = form.test.last_input().map_or_else(
        || "Last —".into(),
        |input| {
            let edge = match input.kind {
                byakko_core::model::tester::InputKind::Press => "press",
                byakko_core::model::tester::InputKind::Release => "release",
                byakko_core::model::tester::InputKind::Repeat => "repeat",
            };
            format!("Last {} · {edge}", key_label(form, input.usage))
        },
    );
    let counters = row![
        help(text(coverage), "Counts describe reported host keys. Green means held; blue means observed. Firmware remapping can change the reported key, so a highlight cannot identify the actuated switch. Fn usually has no independent host event."),
        text(format!("Press {}   Release {}   Repeat {}", form.test.total_presses(), form.test.total_releases(), form.test.total_repeats())).size(12),
        space::horizontal(),
        text(last).size(12),
    ].spacing(style.spacing.m).align_y(iced::Alignment::Center);
    let mut body =
        column![controls(form, style), board(form, style), counters].spacing(style.spacing.s);
    if !form.notice.is_empty() {
        body = body.push(text(&form.notice).size(12));
    }
    if let Some(usage) = form.inspected {
        let stats = form.test.stats().get(&usage).copied().unwrap_or_default();
        body = body.push(help(text(format!(
            "{} · press {} / release {} / repeat {} · chatter {} · hold {}",
            key_label(form,usage), stats.presses, stats.releases, stats.repeats, stats.chatter_candidates,
            stats.last_hold_us.map_or_else(|| "—".into(), |us| format!("{:.3} ms", ms(us)))
        )).size(12), format!("Reported HID usage 0x{usage:02X}. Click another displayed key to inspect its counts.")));
    }
    body = body.push(responsive(move |size| {
        if size.width >= 950.0 {
            row![
                container(chatter(form, style)).width(iced::Length::FillPortion(1)),
                container(timing(form, style)).width(iced::Length::FillPortion(2))
            ]
            .spacing(style.spacing.s)
            .into()
        } else {
            column![chatter(form, style), timing(form, style)]
                .spacing(style.spacing.s)
                .into()
        }
    }));
    panels::vertical_scroll(style, body).height(Fill).into()
}

fn controls<'a>(form: &'a Form, style: &'a UiStyle) -> Element<'a, Message> {
    responsive(move |size| {
        let layout = form.layout;
        let choices: Vec<_> = form.sources.iter().map(SourceChoice).collect();
        let source = pick_list(choices, Some(SourceChoice(&form.source)), |choice| {
            Message::Source(choice.0.clone())
        })
        .on_open(Message::Pause)
        .text_size(13)
        .width(Fill);
        let input_label = help(text("Input").size(12), match &form.source {
            Source::Window => "Keys delivered to the focused app window, combined across keyboards; no device identity.".into(),
            Source::Device(device) => format!("Selected OS input device:\n{}", device.id),
        });
        let refresh = button(if form.refreshing { "Refreshing…" } else { "Refresh" }).on_press_maybe((!form.refreshing).then_some(Message::Refresh));
        let standard = pick_list(Standard::ALL, Some(layout.standard), Message::Standard)
            .on_open(Message::Pause).text_size(13);
        let board_size = pick_list(Size::ALL,Some(layout.size),move |size| Message::Layout(tester::LayoutSelection {size,..layout})).on_open(Message::Pause).text_size(13);
        let legends = pick_list(Legends::ALL,Some(layout.legends),move |legends| Message::Layout(tester::LayoutSelection {legends,..layout})).on_open(Message::Pause).text_size(13);
        let layout_label = help(text("Layout").size(12), "Representative physical geometry; the display follows host key usages. Legends only change the display, not the operating-system layout or keyboard configuration.");
        let start = button(if form.active() { "Pause" } else { "Start" }).on_press(if form.active() { Message::Pause } else { Message::Start });
        let reset = button("Reset").on_press(Message::Reset);
        let status = help(text(if form.active() { "Listening" } else { "Paused" }).size(12), "Shift+Tab pauses and moves focus backward. Capture also stops on focus loss, page changes and close. Start resumes explicitly.");
        let info = help(text("Info").size(12), "Results stay in memory; no input is saved or sent. Native sources observe one selected OS input endpoint independently of the configurator. Linux requires normal read permission for the input device.");
        if size.width >= 1100.0 {
            row![input_label, source, refresh, layout_label, standard, board_size, legends, start, reset, status, info].spacing(style.spacing.s).align_y(iced::Alignment::Center).into()
        } else {
            column![
                row![input_label,source,refresh,start,reset,status].spacing(style.spacing.s).align_y(iced::Alignment::Center),
                row![layout_label,standard,board_size,legends,space::horizontal(),info].spacing(style.spacing.s).align_y(iced::Alignment::Center),
            ].spacing(style.spacing.s).into()
        }
    }).into()
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SourceChoice<'a>(&'a Source);
impl fmt::Display for SourceChoice<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Source::Window => f.write_str("App window · all keyboards"),
            Source::Device(device) => {
                let id = device.id.to_ascii_uppercase();
                if id.contains("VID_3151&PID_4011") {
                    f.write_str("USB receiver")?;
                    if let Some(interface) = id
                        .split("&MI_")
                        .nth(1)
                        .and_then(|part| part.get(..2))
                        .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                    {
                        write!(f, " · MI{interface:02X}")?;
                    }
                    if let Some(instance) = device.id.split('#').nth(2) {
                        write!(f, " · {}", short_label(instance, 18))?;
                    }
                    return Ok(());
                }
                f.write_str(&short_label(&device.label, 36))
            }
        }
    }
}

fn short_label(value: &str, max: usize) -> String {
    let chars: Vec<_> = value.chars().collect();
    if chars.len() <= max {
        return value.to_owned();
    }
    let head = (max - 1) * 2 / 3;
    let tail = max - 1 - head;
    chars[..head]
        .iter()
        .chain(std::iter::once(&'…'))
        .chain(&chars[chars.len() - tail..])
        .collect()
}
fn chatter<'a>(form: &'a Form, style: &'a UiStyle) -> Element<'a, Message> {
    let threshold = form.test.chatter_threshold_us() / 1_000;
    let header = row![
        help(text(format!("Chatter: {}",form.test.candidate_count())).size(style.type_scale.section_title), "Candidates are a release followed by the same reported key's press within this threshold. Autorepeat is excluded. Intentional rapid taps can look identical; a candidate does not diagnose a switch fault."),
        space::horizontal(), text("Threshold").size(12),
        pick_list([5_u64,10,20,30,50],Some(threshold),|ms| Message::Threshold(ms*1_000)).on_open(Message::Pause).text_size(12), text("ms").size(12),
    ].spacing(style.spacing.s).align_y(iced::Alignment::Center);
    let entries: Element<'_, Message> = if form.test.recent_candidates().is_empty() {
        text("No candidates").size(12).into()
    } else {
        let rows = form.test.recent_candidates().iter().rev().fold(
            column![].spacing(3),
            |rows, candidate| {
                rows.push(
                    text(format!(
                        "{} · {:.3} ms hold · {:.3} ms gap",
                        key_label(form, candidate.usage),
                        ms(candidate.previous_make_duration_us),
                        ms(candidate.release_to_next_make_us)
                    ))
                    .size(12),
                )
            },
        );
        panels::vertical_scroll(style, rows).height(64).into()
    };
    container(column![header, entries].spacing(style.spacing.s))
        .padding(10)
        .width(Fill)
        .style(container::rounded_box)
        .into()
}

fn timing<'a>(form: &'a Form, style: &'a UiStyle) -> Element<'a, Message> {
    let mut intervals: Vec<_> = form
        .test
        .interval_samples()
        .iter()
        .map(|sample| sample.since_previous_us)
        .collect();
    intervals.sort_unstable();
    let spacing = if intervals.is_empty() {
        "Event spacing: —".into()
    } else {
        format!(
            "Event spacing · n={} · min {:.3} · median {:.3} · p95 {:.3} ms",
            intervals.len(),
            ms(intervals[0]),
            median_ms(&intervals),
            ms(intervals[(intervals.len() * 95).div_ceil(100).saturating_sub(1)])
        )
    };
    let (usb, usb_details) = usb_summary(form);
    let title = help(
        text("Timing").size(style.type_scale.section_title),
        format!(
            "Timestamp source: {}. Event spacing includes actuation timing, keyboard processing and OS delivery. It measures neither physical latency nor actual USB polling. Physical latency needs an actuation reference; actual bus cadence needs bus-level observation.",
            form.timing_source()
        ),
    );
    container(
        column![
            title,
            text(spacing).size(12),
            help(text(usb).size(12), usb_details)
        ]
        .spacing(style.spacing.s),
    )
    .padding(10)
    .width(Fill)
    .style(container::rounded_box)
    .into()
}
fn usb_summary(form: &Form) -> (String, String) {
    match (&form.source, &form.usb) {
        (Source::Window, _) => (
            "USB declared: —".into(),
            "Combined window input has no selected USB endpoint to inspect.".into(),
        ),
        (Source::Device(_), None) => (
            "USB declared: checking…".into(),
            "Inspecting the selected input's USB descriptor snapshot.".into(),
        ),
        (_, Some(Err(reason))) => ("USB declared: unavailable".into(), reason.to_string()),
        (_, Some(Ok(metadata))) => {
            let values: Vec<_> = metadata
                .endpoints
                .iter()
                .map(|ep| format!("{:.3} ms", ms(ep.interval_us)))
                .collect();
            let summary = if values.len() <= 2 {
                format!("USB declared: {}", values.join(" / "))
            } else {
                format!("USB declared: {} endpoints", values.len())
            };
            let mut details = format!(
                "Descriptor snapshot · {} · {}",
                metadata.speed, metadata.source
            );
            for ep in &metadata.endpoints {
                details.push_str(&format!(
                    "\nInterface {} / alternate {} / IN 0x{:02X}: {:.3} ms (bInterval {})",
                    ep.interface,
                    ep.alternate,
                    ep.address,
                    ms(ep.interval_us),
                    ep.b_interval
                ));
            }
            details.push_str("\nA declared service interval is a scheduling opportunity, not measured polling, report rate, physical latency or receiver radio timing.");
            (summary, details)
        }
    }
}
fn help<'a>(
    content: impl Into<Element<'a, Message>>,
    explanation: impl Into<String>,
) -> Element<'a, Message> {
    tooltip(
        content,
        text(explanation.into())
            .size(12)
            .width(420)
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
        tooltip::Position::Bottom,
    )
    .gap(4)
    .padding(8)
    .into()
}
fn median_ms(sorted: &[u64]) -> f64 {
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (ms(sorted[middle - 1]) + ms(sorted[middle])) / 2.0
    } else {
        ms(sorted[middle])
    }
}
fn ms(us: u64) -> f64 {
    us as f64 / 1_000.0
}
fn key_label(form: &Form, usage: u16) -> String {
    tester::layout_for(form.layout)
        .into_iter()
        .find(|key| key.usage == Some(usage))
        .map_or_else(|| format!("HID {usage:02X}"), |key| key.label)
}
fn board<'a>(form: &'a Form, style: &'a UiStyle) -> Element<'a, Message> {
    responsive(move |size| {
        let keys = tester::layout_for(form.layout);
        let (width, height) = tester::layout_bounds(&keys);
        let unit = (size.width / width.max(1.0))
            .min(size.height / height.max(1.0))
            .clamp(style.board.min_unit, style.board.max_unit);
        let held: std::collections::BTreeSet<_> = form.test.held().collect();
        let base: Element<'_, Message> = space().width(width * unit).height(height * unit).into();
        let board = keys.into_iter().fold(stack![base], |board, key| {
            let is_held = key.usage.is_some_and(|usage| held.contains(&usage));
            let tested = key
                .usage
                .is_some_and(|usage| form.test.tested().contains(&usage));
            let width = key.width * unit - style.board.key_gap;
            let height = key.height * unit - style.board.key_gap;
            let key_button = button(
                text(key.label.clone())
                    .size(style.board.key_label_size)
                    .center(),
            )
            .width(width)
            .height(height)
            .padding(0)
            .on_press_maybe(key.usage.map(Message::Inspect))
            .style(move |theme, status| {
                let mut appearance = button::secondary(theme, status);
                if is_held {
                    appearance.background = Some(Color::from_rgb8(35, 112, 70).into());
                    appearance.text_color = Color::WHITE;
                } else if tested {
                    appearance.background = Some(Color::from_rgb8(36, 79, 123).into());
                    appearance.text_color = Color::WHITE;
                }
                appearance
            });
            board.push(pin(key_button).position([key.x * unit, key.y * unit]))
        });
        scrollable(board)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new(),
            ))
            .into()
    })
    .height(310)
    .into()
}
