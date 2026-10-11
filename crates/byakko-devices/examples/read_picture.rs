//! Read-only probe of the picture command path used by the desktop.
use byakko_core::contract::CompletionPayload;
use byakko_core::contract::FeatureResult;
use byakko_core::contract::{Command, CommandPayload, Completion, FeatureCommand};
use byakko_devices::{Executor, nia87};
use std::{
    fs::OpenOptions,
    io::Write,
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let [output] = arguments.as_slice() else {
        return Err("Usage: read_picture NEW_COMPLETION.json".into());
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let candidates = nia87::device::candidates()?;
    let [candidate] = candidates.as_slice() else {
        return Err(format!("Expected one Nia87 collection; found {}", candidates.len()).into());
    };
    let target = nia87::device::Target::from_candidate(candidate)?;
    let executor = Executor::spawn(nia87::BoundNia87Adapter::new(target), Default::default())?;
    executor.set_generation(1);
    executor
        .try_submit(Command {
            generation: 1,
            operation: 1,
            payload: CommandPayload::Picture(FeatureCommand::Read(())),
        })
        .map_err(|_| "Read submission failed")?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let completion = loop {
        match executor.try_receive() {
            Ok(completion) => break completion,
            Err(TryRecvError::Disconnected) => return Err("Read worker stopped".into()),
            Err(TryRecvError::Empty) if Instant::now() >= deadline => {
                return Err("Read timed out".into());
            }
            Err(TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    serde_json::to_writer_pretty(&mut file, &completion)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    let read_ok = matches!(
        &completion,
        Completion {
            payload: CompletionPayload::Picture(FeatureResult::Read(Ok(_))),
            ..
        }
    );
    if !read_ok {
        return Err("Read failed; completion preserved".into());
    }
    println!("Captured read completion. No setters sent.");
    Ok(())
}
