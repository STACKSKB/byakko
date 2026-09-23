//! Original minimal native HID adapter for the Nia87 configuration transport.
//! Operating-system backends implement only enumeration and feature reports.

use std::ffi::{CStr, CString};

pub mod selection;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as backend;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as backend;
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
compile_error!("The original HID backend currently supports Windows and Linux only");

pub use backend::Device as HidDevice;

pub struct DeviceInfo {
    pub(crate) path: CString,
    pub(crate) vid: u16,
    pub(crate) pid: u16,
    pub(crate) interface: i32,
    pub(crate) usage_page: u16,
    pub(crate) usage: u16,
    pub(crate) manufacturer: Option<String>,
    pub(crate) product: Option<String>,
    pub(crate) release: u16,
}

impl DeviceInfo {
    pub fn identity(&self) -> selection::Identity {
        selection::Identity {
            path: self.path.to_string_lossy().into_owned(),
            vendor_id: self.vid,
            product_id: self.pid,
            interface: self.interface,
            usage_page: self.usage_page,
            usage: self.usage,
        }
    }

    pub fn path(&self) -> &CStr {
        &self.path
    }
    pub fn vendor_id(&self) -> u16 {
        self.vid
    }
    pub fn product_id(&self) -> u16 {
        self.pid
    }
    pub fn interface_number(&self) -> i32 {
        self.interface
    }
    pub fn usage_page(&self) -> u16 {
        self.usage_page
    }
    pub fn usage(&self) -> u16 {
        self.usage
    }
    pub fn manufacturer_string(&self) -> Option<&str> {
        self.manufacturer.as_deref()
    }
    pub fn product_string(&self) -> Option<&str> {
        self.product.as_deref()
    }
    pub fn release_number(&self) -> u16 {
        self.release
    }
    pub fn bus_type(&self) -> &'static str {
        "native HID"
    }
}

pub struct HidApi {
    devices: Vec<DeviceInfo>,
}

impl HidApi {
    pub fn new() -> Result<Self> {
        Ok(Self {
            devices: backend::enumerate()?,
        })
    }
    pub fn device_list(&self) -> std::slice::Iter<'_, DeviceInfo> {
        self.devices.iter()
    }
    pub fn open_path(&self, path: &CStr) -> Result<HidDevice> {
        backend::open(path)
    }
}
