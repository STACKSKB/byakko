//! Native composition only; the desktop library never imports the Nia87 bridge.
mod demo;

use byakko::backend::nia87::{self, Nia87Adapter};
use byakko_devices::Executor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let (descriptor, executor) = match arguments.as_slice() {
        [] => {
            let backups = byakko::storage::user_data_dir()?.join("backups");
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
