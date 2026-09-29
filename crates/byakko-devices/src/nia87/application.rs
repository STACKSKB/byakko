//! Nia87 connection composition shared by native frontends.
use byakko_core::session::Session;
use byakko_protocol::nia87::{adapter, lighting_adapter, macro_adapter, picture_adapter};
pub fn session() -> Result<Session, String> {
    Session::new(adapter::descriptor())?
        .with_macros(macro_adapter::capabilities())?
        .with_lighting(lighting_adapter::capabilities())?
        .with_picture(picture_adapter::capabilities())?
        .with_settings(byakko_protocol::nia87::settings_adapter::capabilities())?
        .with_archive(byakko_protocol::nia87::archive_adapter::capabilities())
}
