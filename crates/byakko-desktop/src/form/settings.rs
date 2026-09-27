//! Sliders and toggles emit typed edits; the core editor owns all setting values.
use byakko_core::model::settings::Edit;

#[derive(Clone, Debug)]
pub enum Message {
    Edit(Edit),
    Read,
    Revert,
}
