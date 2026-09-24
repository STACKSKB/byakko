//! Opt-in physical acceptance probe: twelve consecutive F1 uploads, no intervening reads.
use byakko_core::picture::Content;
use byakko_devices::{KeymapDevice, nia87, research_trace};
use std::{fs::OpenOptions, path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [flag, output] = args.as_slice() else {
        return Err("Usage: exercise_picture --write-f1 NEW_OUTPUT_DIRECTORY".into());
    };
    if flag != "--write-f1" {
        return Err("Explicit --write-f1 required".into());
    }
    let output = PathBuf::from(output);
    std::fs::create_dir(&output)?;
    let nia87::device::Availability::Available(candidate) = nia87::device::availability() else {
        return Err("Expected one Nia87".into());
    };
    let target = nia87::device::Target::from_candidate(&candidate)?;
    let mut device = nia87::BoundNia87Adapter::new(target);
    let mut baseline = device.read_picture()?;
    if baseline.context_revision != [13, 0] {
        return Err("Select per-key layer 1 first".into());
    }
    serde_json::to_writer_pretty(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join("before.json"))?,
        &baseline,
    )?;
    let started = Instant::now();
    let (result, trace) = research_trace::with_trace(|| -> Result<(), String> {
        for index in 0..12 {
            let Content::Editable(mut colors) = baseline.content.clone() else {
                return Err("Opaque picture".into());
            };
            colors.insert(
                "slot-012".into(),
                [[255, 0, 0], [0, 255, 0], [0, 0, 255]][index % 3],
            );
            baseline = device
                .apply_picture(&baseline, &colors, &output)
                .map_err(|e| format!("{e:?}"))?;
        }
        Ok(())
    });
    serde_json::to_writer_pretty(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join("trace.json"))?,
        &trace,
    )?;
    result??;
    serde_json::to_writer_pretty(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join("accepted.json"))?,
        &baseline,
    )?;
    println!(
        "12 uploads accepted in {:?}; {} traced reports. Final F1 blue. No final getter or automatic restore.",
        started.elapsed(),
        trace.events.len()
    );
    Ok(())
}
