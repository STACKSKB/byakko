//! Nia87 capability composition shared by native frontends.
use byakko_core::session::Session;

use super::{archive_adapter, lighting_adapter, macro_adapter, picture_adapter, settings_adapter};

pub fn session() -> Result<Session, String> {
    Session::new(super::descriptor())?
        .with_macros(macro_adapter::capabilities())?
        .with_lighting(lighting_adapter::capabilities())?
        .with_picture(picture_adapter::capabilities())?
        .with_settings(settings_adapter::capabilities())?
        .with_archive(archive_adapter::capabilities())
}
