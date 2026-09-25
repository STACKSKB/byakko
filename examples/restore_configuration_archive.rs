//! Explicit recovery using a captured current state and a durable target archive.
fn main() -> byakko::device::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(2..=3).contains(&args.len()) {
        return Err(
            "Usage: restore_configuration_archive CURRENT.json ORIGINAL.json [NEW_TRACE.json]"
                .into(),
        );
    }
    #[cfg(not(feature = "research-tools"))]
    if args.len() == 3 {
        return Err(
            "Transport tracing requires the research-tools feature; no device access".into(),
        );
    }
    let current = byakko::configuration::load(std::path::Path::new(&args[0]))?;
    let original = byakko::configuration::load(std::path::Path::new(&args[1]))?;
    let plan = byakko::configuration_plan::plan(&current, &original)?;
    println!(
        "Restoring {} bindings, macros {:?}, {} colors, lighting {}, {} settings",
        plan.key_bindings,
        plan.macro_slots,
        plan.picture_keys,
        plan.lighting,
        plan.settings.len()
    );
    let apply = || {
        byakko::device::apply_configuration(
            &current,
            &original,
            std::path::Path::new("Research/captures/backups"),
            |s| println!("{s}"),
        )
    };
    #[cfg(feature = "research-tools")]
    if let Some(trace_path) = args.get(2) {
        // Reserve the path before device I/O. Collection stays in memory until
        // apply, verification and any recovery have all finished.
        use std::io::Write;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(trace_path)?;
        let (run, trace) = byakko::research_trace::with_trace(apply);
        let outcome = match &run {
            Ok(Ok(_)) => "Complete original archive restored and verified".to_owned(),
            Ok(Err(error)) => error.to_string(),
            Err(error) => error.clone(),
        };
        let saved = (|| -> byakko::device::Result<()> {
            serde_json::to_writer_pretty(
                &mut output,
                &serde_json::json!({
                    "format": "byakko-research-transport-trace", "version": 2,
                    "scope": "Setter attempts and read_payload exchanges; no injected fault; not a USB bus capture or proof of firmware delivery",
                    "outcome": outcome, "trace": trace,
                }),
            )?;
            output.write_all(b"\n")?;
            output.sync_all()?;
            Ok(())
        })();
        if let Err(error) = saved {
            return Err(format!("{outcome}; could not save trace to {trace_path}: {error}").into());
        }
        println!("Transport trace saved to {trace_path}");
        run.map_err(std::io::Error::other)??;
    } else {
        apply()?;
    }
    #[cfg(not(feature = "research-tools"))]
    apply()?;
    println!("Complete original archive restored and verified.");
    Ok(())
}
