use hidapi::{HidApi, HidDevice};
use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub path: String,
    pub vid: u16,
    pub pid: u16,
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

pub fn candidates() -> Result<Vec<Candidate>> {
    let api = HidApi::new()?;
    Ok(api.device_list().filter(|d| {
        d.vendor_id() == 0x3151 && matches!(d.product_id(), 0x4011 | 0x4015)
        && d.usage_page() == 0xffff && d.usage() == 2
    }).map(|d| Candidate {
        path: d.path().to_string_lossy().into_owned(),
        vid: d.vendor_id(), pid: d.product_id(), interface: d.interface_number(),
        usage_page: d.usage_page(), usage: d.usage(),
        manufacturer: d.manufacturer_string().map(str::to_owned),
        product: d.product_string().map(str::to_owned),
    }).collect())
}

pub fn open_unique() -> Result<(Candidate, HidDevice)> {
    let list = candidates()?;
    if list.len() != 1 {
        return Err(format!("Expected one Nia87 configuration collection; found {}. Connect one keyboard by USB.", list.len()).into());
    }
    let candidate = list.into_iter().next().unwrap();
    let api = HidApi::new()?;
    let path = std::ffi::CString::new(candidate.path.as_str())?;
    let device = api.open_path(&path)?;
    Ok((candidate, device))
}

pub fn descriptor() -> Result<serde_json::Value> {
    let (candidate, device) = open_unique()?;
    let mut bytes = [0u8; 4096];
    let len = device.get_report_descriptor(&mut bytes)?;
    Ok(serde_json::json!({"candidate":candidate,"report_descriptor_hex":bytes[..len].iter().map(|b|format!("{b:02x}")).collect::<Vec<_>>().join(" "),"length":len}))
}

/// Two read-only requests verified against the Nia87-specific vendor call chain.
/// Sending a feature report selects the read operation; it does not mutate settings.
pub fn inspect() -> Result<serde_json::Value> {
    let (candidate, device) = open_unique()?;
    let mut replies = Vec::new();
    for opcode in [0x80u8, 0x85] {
        let mut request = [0u8; 65];
        request[1] = opcode;
        device.send_feature_report(&request)?;
        std::thread::sleep(std::time::Duration::from_millis(30));
        let mut reply = [0u8; 65];
        let n = device.get_feature_report(&mut reply)?;
        if n != 65 || reply[0] != 0 || reply[1] != opcode {
            return Err(format!("Unexpected response to {opcode:02x}: length={n} bytes={:02x?}", &reply[..n]).into());
        }
        replies.push(serde_json::json!({"opcode":opcode,"payload":&reply[1..],"request":&request[1..]}));
    }
    Ok(serde_json::json!({"candidate":candidate,"replies":replies}))
}
