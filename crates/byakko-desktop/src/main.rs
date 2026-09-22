//! Native composition only; the desktop library never imports Nia87 details.
mod demo;

use byakko_core::session::Session;
use byakko_devices::{
    Executor,
    nia87::{self, Nia87Adapter},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let (session, executor) = match arguments.as_slice() {
        [] => {
            let backups = byakko_devices::storage::user_data_dir()?.join("backups");
            (
                Session::new(nia87::descriptor())?
                    .with_macros(nia87::macro_adapter::capabilities())?
                    .with_lighting(nia87::lighting_adapter::capabilities())?
                    .with_picture(nia87::picture_adapter::capabilities())?,
                Executor::spawn(Nia87Adapter, backups)?,
            )
        }
        [flag] if flag == "--demo" => {
            let device = demo::device()?;
            (
                Session::new(device.descriptor().clone())?
                    .with_macros(
                        device
                            .macro_capabilities()
                            .expect("demo macros configured")
                            .clone(),
                    )?
                    .with_lighting(
                        device
                            .lighting_capabilities()
                            .expect("demo lighting configured")
                            .clone(),
                    )?
                    .with_picture(
                        device
                            .picture_capabilities()
                            .expect("demo picture configured")
                            .clone(),
                    )?,
                Executor::spawn(device, Default::default())?,
            )
        }
        _ => return Err("Usage: byakko-desktop [--demo]".into()),
    };
    byakko_desktop::run(session, executor)
}
