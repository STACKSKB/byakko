// SPDX-License-Identifier: GPL-3.0-or-later
//! Local tester state and the lifetime of its explicit input observation.
use byakko_core::{
    model::tester::Tester,
    projection::tester::{LayoutSelection, Legends, Standard},
};
use byakko_devices::{
    tester_input::{self, InputDevice, Sample, Sampler},
    tester_usb::{Metadata, Unavailable},
};
use iced::Event;
use std::{fmt, time::Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Window,
    Device(InputDevice),
}
impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Window => f.write_str("Window input (all keyboards)"),
            Self::Device(device) => f.write_str(&device.label),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    Start,
    Pause,
    Reset,
    Refresh,
    Sources(Result<Vec<InputDevice>, String>),
    Source(Source),
    Usb {
        request: u64,
        device: InputDevice,
        result: Result<Metadata, Unavailable>,
    },
    Layout(LayoutSelection),
    Standard(Standard),
    Threshold(u64),
    Input(Event, Instant),
    Drain,
    Inspect(u16),
}

pub struct Form {
    pub test: Tester,
    pub layout: LayoutSelection,
    pub sources: Vec<Source>,
    pub source: Source,
    pub notice: String,
    pub refreshing: bool,
    pub inspected: Option<u16>,
    pub usb: Option<Result<Metadata, Unavailable>>,
    usb_request: u64,
    sampler: Option<Sampler>,
    origin: Instant,
}
impl Default for Form {
    fn default() -> Self {
        let mut test = Tester::default();
        test.pause();
        Self {
            test,
            layout: LayoutSelection::default(),
            sources: vec![Source::Window],
            source: Source::Window,
            notice: String::new(),
            refreshing: false,
            inspected: None,
            usb: None,
            usb_request: 0,
            sampler: None,
            origin: Instant::now(),
        }
    }
}
impl Form {
    pub fn usb_request(&self) -> u64 {
        self.usb_request
    }
    pub fn active(&self) -> bool {
        !self.test.is_paused()
    }
    pub fn native(&self) -> bool {
        self.sampler.is_some()
    }
    pub fn pause(&mut self) {
        self.sampler.take();
        self.test.pause();
    }
    pub fn update(&mut self, message: Message) {
        match message {
            Message::Start => {
                self.pause();
                self.notice.clear();
                if let Source::Device(device) = &self.source {
                    match Sampler::start(device) {
                        Ok(sampler) => self.sampler = Some(sampler),
                        Err(reason) => {
                            if reason == tester_input::InputError::DeviceUnavailable {
                                self.usb_unavailable();
                            }
                            self.notice = reason.to_string();
                            return;
                        }
                    }
                }
                self.origin = Instant::now();
                self.test.resume();
            }
            Message::Pause => self.pause(),
            Message::Reset => {
                self.pause();
                self.test.reset();
                self.notice.clear();
            }
            Message::Source(source) => {
                self.pause();
                // A result belongs to one explicit source; never mix two keyboards.
                self.test.reset();
                self.source = source;
                self.usb = None;
                self.usb_request = self.usb_request.wrapping_add(1);
                self.notice.clear();
            }
            Message::Usb {
                request,
                device,
                result,
            } => {
                if request == self.usb_request
                    && self.source == Source::Device(device)
                    && self.usb.is_none()
                {
                    self.usb = Some(result);
                }
            }
            Message::Layout(layout) => self.layout = layout,
            Message::Standard(standard) => {
                if self.layout.standard != standard {
                    self.layout.standard = standard;
                    if standard == Standard::Jis {
                        self.layout.legends = Legends::Japanese;
                    } else if self.layout.legends == Legends::Japanese {
                        self.layout.legends = Legends::Qwerty;
                    }
                }
            }
            Message::Threshold(us) => self.test.set_chatter_threshold_us(us),
            Message::Sources(result) => {
                self.refreshing = false;
                match result {
                    Ok(devices) => {
                        self.sources = std::iter::once(Source::Window)
                            .chain(devices.into_iter().map(Source::Device))
                            .collect();
                        if !self.sources.contains(&self.source) {
                            self.pause();
                            self.usb_unavailable();
                            self.notice = "Selected input keyboard is unavailable. Choose a source explicitly.".into();
                        }
                    }
                    Err(reason) => self.notice = reason,
                }
            }
            Message::Input(event, at) if self.active() => {
                if crate::input::tester::pauses(&event) {
                    self.pause();
                    self.notice = "Paused. Choose Start test to resume.".into();
                } else if !self.native() {
                    let at_us = at
                        .saturating_duration_since(self.origin)
                        .as_micros()
                        .min(u128::from(u64::MAX)) as u64;
                    if let Some(input) = crate::input::tester::input(&event, at_us) {
                        self.test.accept(input);
                    }
                }
            }
            Message::Drain => {
                // Receipt timestamps come from the sampler, independently of UI refresh.
                for _ in 0..512 {
                    let Some(sample) = self.sampler.as_ref().and_then(Sampler::try_recv) else {
                        break;
                    };
                    match sample {
                        Sample::Input(input) if self.active() => self.test.accept(input),
                        Sample::Input(_) => {}
                        Sample::Discontinuity => {
                            self.pause();
                            self.notice = "Input continuity was lost. Results are retained; start a new observation.".into();
                            break;
                        }
                        Sample::Error(reason) => {
                            self.pause();
                            if reason == tester_input::InputError::DeviceUnavailable {
                                self.usb_unavailable();
                            }
                            self.notice = reason.to_string();
                            break;
                        }
                    }
                }
            }
            Message::Inspect(usage) => self.inspected = Some(usage),
            Message::Refresh | Message::Input(..) => {}
        }
    }
    fn usb_unavailable(&mut self) {
        self.usb_request = self.usb_request.wrapping_add(1);
        self.usb = Some(Err(Unavailable(
            "Selected input device is unavailable".into(),
        )));
    }
    pub fn timing_source(&self) -> &'static str {
        match self.source {
            Source::Window => "Iced window event receipt (all keyboards)",
            Source::Device(_) => tester_input::TIMESTAMP_SOURCE,
        }
    }
}
