use std::process::ExitCode;

fn run() -> byakko::device::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["inspect"] {
        println!("{}", serde_json::to_string_pretty(&byakko::device::inspect()?)?);
        return Ok(());
    }
    if args.as_slice() == ["descriptor"] {
        println!("{}", serde_json::to_string_pretty(&byakko::device::descriptor()?)?);
        return Ok(());
    }
    if args.as_slice() != ["devices"] {
        return Err("Usage: byakko devices (enumeration only; no configuration commands)".into());
    }
    let api = hidapi::HidApi::new()?;
    let mut count = 0;
    for device in api
        .device_list()
        .filter(|d| d.vendor_id() == 0x3151 && matches!(d.product_id(), 0x4011 | 0x4015))
    {
        count += 1;
        println!(
            "candidate {:04x}:{:04x} interface={} usage={:04x}:{:04x} release={:04x} bus={:?}",
            device.vendor_id(),
            device.product_id(),
            device.interface_number(),
            device.usage_page(),
            device.usage(),
            device.release_number(),
            device.bus_type()
        );
        println!(
            "  manufacturer={:?} product={:?}",
            device.manufacturer_string(),
            device.product_string()
        );
        println!("  path={}", device.path().to_string_lossy());
        if device.usage_page() == 0xffff && device.usage() == 2 {
            println!(
                "  matches vendor configuration collection; board identity still needs protocol verification"
            );
        }
    }
    if count == 0 {
        return Err("No candidate Nia87 HID collections found. Check USB connection and device permissions.".into());
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
