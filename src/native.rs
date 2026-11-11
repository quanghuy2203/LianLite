//! Hardware diagnostics and native controller inventory.
//! No HID writes, feature reports, SMBus/I2C, firmware or GPU control commands.
use serde::Serialize;
use std::{collections::BTreeSet, fs, path::PathBuf, process::Command};

#[derive(Clone, Serialize)]
pub struct NativeDevice {
    pub kind: &'static str,
    pub name: String,
    pub id: String,
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
    pub driver: &'static str,
}

fn identify(vid: u16, pid: u16) -> Option<(&'static str, &'static str)> {
    match (vid, pid) {
        (0x0B05, 0x19AF) => Some(("ASUS Aura USB", "Protocol candidate: ASUS Aura USB; writes disabled until verified")),
        (0x1A86, 0x2107) => Some(("Lian Li HID", "SLV3H seen; L-Wireless identity not verified")),
        _ => None,
    }
}
pub fn scan_hid() -> Result<Vec<NativeDevice>, String> {
    let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for device in api.device_list() {
        let vid = device.vendor_id();
        let pid = device.product_id();
        let Some((kind, driver)) = identify(vid, pid) else { continue; };
        let id = format!("VID_{vid:04X}&PID_{pid:04X}");
        let product = device.product_string().unwrap_or("Unnamed HID device");
        let item = NativeDevice {
            kind, name: product.into(), id, interface: device.interface_number(),
            usage_page: device.usage_page(), usage: device.usage(), driver,
        };
        let key = format!("{}:{}:{}:{}:{}", item.id, item.interface, item.usage_page, item.usage, item.name);
        if seen.insert(key) { result.push(item); }
    }
    Ok(result)
}
fn ps(script: &str) -> Result<String, String> {
    let o = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output().map_err(|e| format!("Windows PowerShell not available: {e}"))?;
    if !o.status.success() { return Err(format!("PowerShell error: {}", String::from_utf8_lossy(&o.stderr))); }
    String::from_utf8(o.stdout).map_err(|e| e.to_string())
}
pub fn scan_gpu_and_memory() -> Result<String, String> {
    // Windows built-in CIM inventory. Values are returned as JSON.
    // Serial numbers, memory part numbers and PnP identifiers may be personal identifiers:
    // review the report before sharing.
    ps(r#"$ErrorActionPreference='Stop'; $gpu = @(Get-CimInstance Win32_VideoController | Select-Object Name,PNPDeviceID,DriverVersion); $ram = @(Get-CimInstance Win32_PhysicalMemory | Select-Object Manufacturer,PartNumber,Capacity,Speed,ConfiguredClockSpeed); @{Graphics=$gpu;Memory=$ram} | ConvertTo-Json -Depth 5"#)
}
pub fn export_report(hid: &[NativeDevice], windows: &str) -> Result<PathBuf, String> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("LianLite");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join("native-hardware-v06.json");
    let raw: serde_json::Value = serde_json::from_str(windows).map_err(|e| e.to_string())?;
    let report = serde_json::json!({"HID":hid, "Windows":raw, "note":"Read-only inventory. Check IDs before sharing."});
    let bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    fs::write(&target, bytes).map_err(|e| e.to_string())?;
    Ok(target)
}

pub fn inspect_aura_pnp() -> Result<String, String> {
    // Windows PnP properties are read-only. Exact VID/PID allowlist prevents
    // indiscriminate access to unrelated system devices.
    // No HID handle is opened, and no I/O packets are sent to the controller.
    ps(r#"
$ErrorActionPreference = 'Stop'
$devices = @(Get-PnpDevice -PresentOnly | Where-Object {
  $_.InstanceId -match 'VID_0B05&PID_19AF'
})
$report = @(
  foreach ($d in $devices) {
    $props = @(Get-PnpDeviceProperty -InstanceId $d.InstanceId -ErrorAction SilentlyContinue |
      Where-Object { $_.KeyName -in @(
        'DEVPKEY_Device_DriverVersion',
        'DEVPKEY_Device_DriverProvider',
        'DEVPKEY_Device_BusReportedDeviceDesc',
        'DEVPKEY_Device_HardwareIds',
        'DEVPKEY_Device_CompatibleIds',
        'DEVPKEY_Device_LocationInfo'
      ) } |
      ForEach-Object {
        @{ Key = $_.KeyName; Value = @($_.Data | ForEach-Object { [string]$_ }) }
      })
    @{
      Name = $d.FriendlyName
      InstanceId = $d.InstanceId
      Status = [string]$d.Status
      Class = [string]$d.Class
      Properties = $props
    }
  }
)
@{
    DeviceCount = $report.Count
    Devices = $report
    HardwareProtocol = 'UNVERIFIED: no HID writes are enabled'
} | ConvertTo-Json -Depth 7 -Compress
"#)
}

pub fn export_aura_report(aura_json: &str) -> Result<PathBuf, String> {
    let parsed: serde_json::Value = serde_json::from_str(aura_json).map_err(|e| e.to_string())?;
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("LianLite");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("asus-aura-pnp-v07.json");
    fs::write(&path, serde_json::to_vec_pretty(&parsed).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_exact_known_vid_pid_only() {
        assert!(identify(0x0B05, 0x19AF).is_some());
        assert!(identify(0x1A86, 0x2107).is_some());
        assert!(identify(0x0B05, 0x0001).is_none());
        assert!(identify(0x36A7, 0xA868).is_none());
    }
}
