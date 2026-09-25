//! Read-only probe of the settings command path used by the desktop.
use byakko_core::session::CompletionPayload;
use byakko_core::{
    session::{Completion, Session},
    settings::{Content, editor::Status},
};
use byakko_devices::{Executor, nia87};
use std::{
    fs::OpenOptions,
    io::Write,
    sync::mpsc::TryRecvError,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let [output] = arguments.as_slice() else {
        return Err("Usage: read_settings NEW_COMPLETION.json".into());
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let mut session =
        Session::new(nia87::descriptor())?.with_settings(nia87::settings_adapter::capabilities())?;
    let executor = Executor::spawn(nia87::Nia87Adapter, Default::default())?;
    executor.set_generation(session.connect()?);
    executor
        .try_submit(session.request_settings_read()?)
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
            payload: CompletionPayload::ReadSettings { result: Ok(_), .. },
            ..
        }
    );
    session.accept(completion);
    let editor = session.settings().expect("configured settings");
    if !read_ok || *editor.status() != Status::Ready {
        return Err(format!("Read failed; completion preserved: {:?}", editor.status()).into());
    }
    match &editor.baseline().expect("verified baseline").content {
        Content::Editable(values) => {
            println!(
                "Verified {} scalar settings. No setters sent.",
                values.len()
            )
        }
        Content::Opaque { reason } => {
            println!("Read preserved as opaque: {reason}. No setters sent.")
        }
    }
    Ok(())
}
