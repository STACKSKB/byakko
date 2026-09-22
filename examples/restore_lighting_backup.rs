//! Restore a known lighting backup through the normal verified transaction.
use byakko::{device, lighting::Lighting};

fn main() -> device::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Supply a lighting backup JSON path")?;
    let backup: serde_json::Value = serde_json::from_reader(std::fs::File::open(path)?)?;
    if backup["format_version"] != 1 || backup["firmware"] != 256 || backup["profile"] != 0 {
        return Err("Not a validated Nia87 lighting backup".into());
    }
    let saved: Lighting = serde_json::from_value(backup["before"].clone())?;
    let desired = saved.recognized_setting().ok_or("Unknown saved mode")?;
    let current = device::read_lighting()?;
    let after = device::apply_lighting(
        &current,
        &desired,
        std::path::Path::new("Research/captures/backups"),
    )?;
    if after.raw()[1..8] != saved.raw()[1..8] {
        return Err("Restored fields do not match backup".into());
    }
    println!(
        "Lighting restored to effect {} from backup",
        after.effect_id()
    );
    Ok(())
}
