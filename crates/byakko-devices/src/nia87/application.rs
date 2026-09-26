//! Nia87 connection composition shared by native frontends.
use byakko_core::session::Session;
pub fn session() -> Result<Session, String> {
    Session::new(super::descriptor())?.with_macros(super::macro_adapter::capabilities())
}
