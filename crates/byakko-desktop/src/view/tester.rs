// SPDX-License-Identifier: GPL-3.0-or-later
//! Native keyboard tester: reported host keys, input edges, and timing evidence.
use crate::{
    form::tester::{Form, Message},
    widget::panels::{self, UiStyle},
};
use byakko_core::projection::tester::{self, Legends, Size, Standard};
use iced::{
    Color, Element, Fill,
    widget::{
        button, column, pick_list, pin, responsive, row, scrollable, space, stack, text, tooltip,
    },
};

pub fn view<'a>(form: &'a Form, style: &'a UiStyle) -> Element<'a, Message> {
    let layout = form.layout;
    let choices = row![
        column![
            text("Physical layout"),
            pick_list(Standard::ALL, Some(layout.standard), move |standard| {
                Message::Layout(tester::LayoutSelection { standard, ..layout })
            })
            .on_open(Message::Pause)
        ],
        column![
            text("Size"),
            pick_list(Size::ALL, Some(layout.size), move |size| Message::Layout(
                tester::LayoutSelection { size, ..layout }
            ))
            .on_open(Message::Pause)
        ],
        column![
            text("Legends"),
            pick_list(Legends::ALL, Some(layout.legends), move |legends| {
                Message::Layout(tester::LayoutSelection { legends, ..layout })
            })
            .on_open(Message::Pause)
        ],
    ]
    .spacing(style.spacing.m);
    let source = row![
        pick_list(form.sources.as_slice(), Some(&form.source), Message::Source)
            .on_open(Message::Pause)
            .width(Fill),
        button(if form.refreshing {
            "Refreshing…"
        } else {
            "Refresh keyboards"
        })
        .on_press_maybe((!form.refreshing).then_some(Message::Refresh)),
    ]
    .spacing(style.spacing.m);
    let mut body = column![
        text("Keyboard tester").size(style.type_scale.section_title),
        text("Choose an input source. Results stay in memory; no input is saved or sent."),
        source,
        choices,
        row![
            button(if form.active() { "Pause" } else { "Start test" }).on_press(if form.active() {
                Message::Pause
            } else {
                Message::Start
            }),
            button("Reset results").on_press(Message::Reset),
            text(if form.active() {
                "Listening · Shift+Tab for navigation"
            } else {
                "Paused · keyboard input is available to controls"
            }),
        ]
        .spacing(style.spacing.m)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(style.spacing.m);
    if let crate::form::tester::Source::Device(device) = &form.source {
        body = body.push(text(format!("Selected input: {}", device.id)).size(11));
    }
    if !form.notice.is_empty() {
        body = body.push(text(&form.notice));
    }
    let keys = tester::layout_for(layout);
    let observable = keys
        .iter()
        .filter_map(|key| key.usage)
        .collect::<std::collections::BTreeSet<_>>();
    let tested = observable.intersection(form.test.tested()).count();
    let outside = form.test.tested().difference(&observable).count();

    body = body.push(board(form, style)).push(text(format!(
        "Reported host keys · held: {}    Observed: {tested}/{}    Most held at once: {}    Outside layout: {outside}",
        form.test.held().count(),
        observable.len(),
        form.test.peak_held()
    )));
    body = body.push(text(format!("Presses: {}    Releases: {}    Repeats: {}", form.test.total_presses(), form.test.total_releases(), form.test.total_repeats())))
        .push(text("Green: held host key · Blue: observed host key. Firmware remapping prevents identifying the actuated switch. Fn usually has no independent host event.").size(12));
    if let Some(input) = form.test.last_input() {
        body = body.push(text(format!(
            "Last input: {} · {:?} · HID {:02X}",
            key_label(form, input.usage),
            input.kind,
            input.usage
        )));
    }
    if let Some(usage) = form.inspected {
        let stats = form.test.stats().get(&usage).copied().unwrap_or_default();
        body = body.push(text(format!(
            "{} (HID {usage:02X}) · presses {} · releases {} · repeats {}",
            key_label(form, usage),
            stats.presses,
            stats.releases,
            stats.repeats
        )));
        body = body.push(text(format!(
            "Chatter candidates for this host key: {} · last observed hold: {}",
            stats.chatter_candidates,
            stats
                .last_hold_us
                .map_or_else(|| "unavailable".into(), |us| format!("{:.3} ms", ms(us)))
        )));
    }
    let threshold_ms = form.test.chatter_threshold_us() / 1_000;
    let mut chatter = column![
        row![text(format!("Chatter candidates: {} · release → next press", form.test.candidate_count())), pick_list([5_u64, 10, 20, 30, 50], Some(threshold_ms), |ms| Message::Threshold(ms * 1_000)).on_open(Message::Pause), text("ms threshold")].spacing(style.spacing.s).align_y(iced::Alignment::Center),
        text("Repeats are excluded. Intentional rapid tapping can produce the same candidate; this does not diagnose a faulty switch.").size(12),
    ].spacing(style.spacing.s);
    if form.test.recent_candidates().is_empty() {
        chatter = chatter.push(text("No candidates observed."));
    }
    for candidate in form.test.recent_candidates().iter().rev().take(8) {
        chatter = chatter.push(text(format!(
            "{} · held {:.3} ms, released {:.3} ms before next press",
            key_label(form, candidate.usage),
            ms(candidate.previous_make_duration_us),
            ms(candidate.release_to_next_make_us)
        )));
    }
    let mut intervals: Vec<_> = form
        .test
        .interval_samples()
        .iter()
        .map(|sample| sample.since_previous_us)
        .collect();
    intervals.sort_unstable();
    let timing = if intervals.is_empty() {
        "No input intervals observed yet.".into()
    } else {
        format!(
            "Recent edge spacing · n={} · min {:.3} ms · median {:.3} ms · p95 {:.3} ms",
            intervals.len(),
            ms(intervals[0]),
            median_ms(&intervals),
            ms(intervals[(intervals.len() * 95).div_ceil(100).saturating_sub(1)])
        )
    };
    body = body.push(panels::panel(style, "Switch checks", chatter.into()))
        .push(panels::panel(style, "Input timing", column![
            text(format!("Timestamp source: {}", form.timing_source())),
            text(timing),
            text("Observed OS event spacing includes your tapping, keyboard processing, and OS delivery. It does not measure USB polling or switch-to-host latency.").size(12),
            text("USB descriptor interval: unavailable in this observation. Actual USB polling: not measured. Physical latency: requires an actuation reference.").size(12),
        ].spacing(style.spacing.s).into()));
    panels::vertical_scroll(style, body).height(Fill).into()
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
            let description = key.usage.map_or_else(
                || "Firmware Fn · usually no independent host input".into(),
                |usage| {
                    format!(
                        "{} · HID {usage:02X} · reported host usage · click for counts",
                        key.label
                    )
                },
            );
            board.push(
                pin(tooltip(
                    key_button,
                    text(description),
                    tooltip::Position::Top,
                ))
                .position([key.x * unit, key.y * unit]),
            )
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
