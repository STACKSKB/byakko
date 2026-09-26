//! The public archive workflow is diagnostic capture and export only.
use crate::{
    form::files::{Form, Message, Operation},
    widget::panels::UiStyle,
};
use byakko_core::library::archive::{Capture, CaptureProblem, CaptureStatus};
use iced::{
    Element,
    widget::{button, column, row, text, text_input},
};

pub fn view<'a>(
    form: &'a Form,
    capture: &'a Capture,
    idle: bool,
    connected: bool,
    style: &'a UiStyle,
) -> Element<'a, Message> {
    let mut content = column![
        text("Diagnostic capture").size(style.type_scale.section_title),
        text("Capture the keyboard's stored configuration for diagnostics. Staged edits are not included."),
        button("Capture current").on_press_maybe((idle && connected).then_some(Message::Capture)),
        row![
            text_input("New capture file path", &form.archive_path).on_input_maybe(idle.then_some(Message::ArchivePath)),
            button("Save captured file").on_press_maybe((idle && capture.captured().is_some()).then_some(Message::Begin(Operation::ExportArchive))),
        ].spacing(style.spacing.s),
        text("Whole-configuration restore is unavailable in this pre-alpha."),
    ].spacing(style.spacing.m);
    if let Some(archive) = capture.captured() {
        content = content.push(text(format!(
            "Last successful capture: {} bytes",
            archive.bytes.len()
        )));
    }
    if let CaptureStatus::Failed(problem) = capture.status() {
        let reason = match problem {
            CaptureProblem::Capture(reason) | CaptureProblem::InvalidResult(reason) => reason,
        };
        content = content.push(text(format!(
            "The latest capture failed: {reason}. Any earlier capture is retained."
        )));
    }
    content.into()
}
