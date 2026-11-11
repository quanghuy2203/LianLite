//! Read-only USB HID report-descriptor inspection for a single allowlisted
//! ASUS Aura interface. Windows/HIDAPI may reconstruct the descriptor.
use serde::Serialize;
use std::{fs, path::PathBuf};

const ASUS_VID: u16 = 0x0B05;
const ASUS_PID: u16 = 0x19AF;
const MAX_DESCRIPTOR: usize = 4096;

#[derive(Serialize)]
pub struct DescriptorInfo {
    pub manufacturer: String,
    pub product: String,
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
    pub length: usize,
    pub descriptor_hex: String,
    pub warning: &'static str,
}

pub fn inspect() -> Result<Vec<DescriptorInfo>, String> {
    let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    let mut found = false;
    let mut errors = Vec::new();

    for entry in api.device_list() {
        if entry.vendor_id() != ASUS_VID || entry.product_id() != ASUS_PID {
            continue;
        }
        // MI_02 is the HID vendor-defined interface in the user's PnP report.
        if entry.interface_number() != 2 || entry.usage_page() != 0xFF72 || entry.usage() != 0x00A1 {
            continue;
        }
        found = true;
        let device = match entry.open_device(&api) {
            Ok(device) => device,
            Err(error) => {
                errors.push(format!("ASUS HID interface could not be opened: {error}"));
                continue;
            }
        };
        let mut buffer = [0u8; MAX_DESCRIPTOR];
        match device.get_report_descriptor(&mut buffer) {
            Ok(size) if size > 0 && size <= MAX_DESCRIPTOR => {
                let mut hex = String::with_capacity(size * 2);
                for byte in &buffer[..size] {
                    use std::fmt::Write as _;
                    let _ = write!(&mut hex, "{byte:02X}");
                }
                result.push(DescriptorInfo {
                    manufacturer: entry.manufacturer_string().unwrap_or("").into(),
                    product: entry.product_string().unwrap_or("").into(),
                    interface: entry.interface_number(),
                    usage_page: entry.usage_page(),
                    usage: entry.usage(),
                    length: size,
                    descriptor_hex: hex,
                    warning: "Read-only HIDAPI descriptor; Windows may reconstruct it. No HID output or feature reports issued.",
                });
            }
            Ok(_) => errors.push("ASUS HID report descriptor was empty".to_string()),
            Err(error) => errors.push(format!("ASUS HID report descriptor unavailable: {error}")),
        }
    }
    if !result.is_empty() { return Ok(result); }
    if !errors.is_empty() { return Err(errors.join(" | ")); }
    if !found { return Err("ASUS 0B05:19AF / MI_02 / FF72:00A1 not detected".into()); }
    Err("Unable to inspect ASUS HID descriptor".into())
}

pub fn export(items: &[DescriptorInfo]) -> Result<PathBuf, String> {
    let root = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let dir = root.join("LianLite");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("aura-hid-descriptor-v09.json");
    let payload = serde_json::to_vec_pretty(items).map_err(|e| e.to_string())?;
    fs::write(&path, payload).map_err(|e| e.to_string())?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    #[test]
    fn known_aura_interface() {
        assert_eq!((0x0B05u16, 0x19AFu16, 2i32, 0xFF72u16, 0x00A1u16),
            (0x0B05, 0x19AF, 2, 0xFF72, 0x00A1));
    }
}
