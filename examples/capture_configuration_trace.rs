//! Read-only complete capture with raw getter diagnostics, including on failure.
use byakko::{configuration, device, research_trace};
use std::{fs::OpenOptions, io::Write, path::Path};

fn main() -> device::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("Usage: capture_configuration_trace NEW_ARCHIVE.json NEW_TRACE.json".into());
    }
    let archive_path = Path::new(&args[0]);
    if archive_path.exists() {
        return Err("Archive already exists; choose a new path".into());
    }
    let mut trace_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    // Also catches aliases of the newly reserved trace path before USB I/O.
    if archive_path.exists() {
        return Err(
            "Archive path is now occupied; archive and trace need distinct new paths".into(),
        );
    }
    let (result, trace) = research_trace::with_trace(|| {
        device::capture_configuration(|done, total| {
            if done % 10 == 0 {
                eprintln!("Read-only capture: {done}/{total} macro slots");
            }
        })
    });
    let outcome = match &result {
        Ok(Ok(_)) => "One complete capture read".to_owned(),
        Ok(Err(error)) => error.to_string(),
        Err(error) => error.clone(),
    };
    serde_json::to_writer_pretty(
        &mut trace_file,
        &serde_json::json!({
            "format": "byakko-research-transport-trace", "version": 2,
            "scope": "Read-only read_payload exchanges; no setters; not a USB bus capture",
            "outcome": outcome, "trace": trace,
        }),
    )?;
    trace_file.write_all(b"\n")?;
    trace_file.sync_all()?;
    println!("Trace saved to {}", args[1]);
    let captured = result.map_err(std::io::Error::other)??;
    configuration::save_new(archive_path, &captured)?;
    println!("Verified archive saved to {}", archive_path.display());
    Ok(())
}
