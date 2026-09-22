//! Explicit recovery using a captured current state and a durable target archive.
fn main() -> byakko::device::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("Usage: restore_configuration_archive CURRENT.json ORIGINAL.json".into());
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
    byakko::device::apply_configuration(
        &current,
        &original,
        std::path::Path::new("Research/captures/backups"),
        |s| println!("{s}"),
    )?;
    println!("Complete original archive restored and verified.");
    Ok(())
}
