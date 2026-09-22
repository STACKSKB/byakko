//! Native composition only; the desktop library never imports Nia87 details.
mod demo;

use byakko_devices::{
    Executor,
    nia87::{self, Nia87Adapter},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let (descriptor, executor) = match arguments.as_slice() {
        [] => {
            let backups = byakko_devices::storage::user_data_dir()?.join("backups");
            (nia87::descriptor(), Executor::spawn(Nia87Adapter, backups)?)
        }
        [flag] if flag == "--demo" => {
            let device = demo::device()?;
            (
                device.descriptor().clone(),
                Executor::spawn(device, Default::default())?,
            )
        }
        _ => return Err("Usage: byakko-desktop [--demo]".into()),
    };
    byakko_desktop::run(descriptor, executor)
}
