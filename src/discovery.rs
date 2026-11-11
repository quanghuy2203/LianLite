//! Evidence-based, read-only Lian Li device discovery.
//! No USB transactions, radio polling, services stopped or vendor settings modified.
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::{BTreeMap, BTreeSet}, fs, io::Read, path::{Path, PathBuf}, process::Command};

const MAX_FILES: usize = 512;
const MAX_SETTING_BYTES: u64 = 512 * 1024;
const MAX_JSON_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
pub struct GroupEvidence {
    pub device_key: String,
    pub fan_count: Option<usize>,
    pub name: String,
    pub source: String,
    pub live: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct DongleEvidence {
    pub model: String,
    pub detected: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct DiscoveryReport {
    pub groups: Vec<GroupEvidence>,
    pub dongles: Vec<DongleEvidence>,
    pub lcd_usb_receivers: Option<usize>,
    pub saved_records_checked: usize,
    pub warnings: Vec<String>,
    pub live_rf_groups_verified: bool,
}
impl DiscoveryReport {
    pub fn identified_fans(&self) -> usize {
        self.groups.iter().filter_map(|x| x.fan_count).sum()
    }
}
fn identify_adapter(id: &str) -> Option<&'static str> {
    let upper = id.to_ascii_uppercase();
    [
        ("VID_0416&PID_8040", "L-Wireless V1 TX"),
        ("VID_0416&PID_8041", "L-Wireless V1 RX"),
        ("VID_1A86&PID_E304", "L-Wireless V2 TX"),
        ("VID_1A86&PID_E305", "L-Wireless V2 RX"),
    ].iter().find_map(|(key,name)| upper.contains(key).then_some(*name))
}
fn is_lcd_receiver_root(instance_id: &str) -> bool {
    instance_id.to_ascii_uppercase().starts_with("USB\\VID_1CBE&PID_0006\\")
}
fn windows_dongles() -> Result<(Vec<DongleEvidence>, usize), String> {
    // A PnP inventory is NOT a discovery protocol and may return multiple interfaces.
    let code = r#"$ErrorActionPreference='Stop'
@(Get-PnpDevice -PresentOnly | Where-Object {
  $_.InstanceId -match 'VID_(0416&PID_804[01]|1A86&PID_E30[45]|1CBE&PID_0006)'
} | Select-Object -ExpandProperty InstanceId) | ConvertTo-Json -Compress"#;
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", code])
        .output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("PnP query failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    let ids: Vec<String> = match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Array(v)) => v.into_iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
        Ok(Value::String(v)) => vec![v],
        _ => vec![],
    };
    let unique: BTreeSet<&str> = ids.iter().filter_map(|x| identify_adapter(x)).collect();
    // Only the root USB instance is counted. Composite HID/MI_XX interfaces
    // are not separate LCD fans. These endpoints cannot reveal RF grouping.
    let lcd = ids.iter()
        .filter(|id| is_lcd_receiver_root(id))
        .map(|id| id.to_ascii_uppercase())
        .collect::<BTreeSet<_>>().len();
    Ok((unique.into_iter().map(|model| DongleEvidence {
        model: model.to_owned(), detected: true
    }).collect(), lcd))
}

fn walk_files(root: &Path, files: &mut Vec<PathBuf>, depth: u8) {
    if depth > 5 || files.len() >= MAX_FILES { return; }
    let Ok(dir) = fs::read_dir(root) else { return; };
    for entry in dir.flatten() {
        if files.len() >= MAX_FILES { break; }
        let Ok(kind) = entry.file_type() else { continue; };
        if kind.is_symlink() { continue; }
        if kind.is_dir() { walk_files(&entry.path(), files, depth+1); }
        else if kind.is_file() { files.push(entry.path()); }
    }
}
fn setting(path: &Path) -> Option<Value> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() < 5 || metadata.len() > MAX_SETTING_BYTES { return None; }
    let bytes = fs::read(path).ok()?;
    let mut decoded = Vec::new();
    if bytes.starts_with(&[0x1F,0x8B]) {
        let mut limited = GzDecoder::new(bytes.as_slice()).take(MAX_JSON_BYTES + 1);
        limited.read_to_end(&mut decoded).ok()?;
    } else if bytes.first() == Some(&b'{') {
        decoded = bytes;
    } else {
        return None;
    }
    if decoded.len() as u64 > MAX_JSON_BYTES { return None; }
    serde_json::from_slice(&decoded).ok()
}
fn fan_count(type_name: &str, data: &Value) -> Option<usize> {
    if !type_name.eq_ignore_ascii_case("FanQuantity") { return None; }
    // L-Connect also stores non-wireless fan counts. Never interpret this as
    // a live RF group count unless a wireless-specific source is established.
    let count = data.as_u64().or_else(|| data.as_str()?.parse::<u64>().ok())?;
    (1..=4).contains(&count).then_some(count as usize)
}
fn key_safe(index: usize) -> String {
    format!("Saved setting #{index}")
}
fn saved_groups(root: &Path) -> (Vec<GroupEvidence>, usize) {
    let mut files = Vec::new();
    walk_files(root, &mut files, 0);
    let mut by_id: BTreeMap<String, usize> = BTreeMap::new();
    let mut records = 0;
    for file in files {
        let Some(v) = setting(&file) else { continue; };
        let Some(id) = v.get("DeviceID").and_then(Value::as_str) else { continue; };
        let Some(typ) = v.get("Type").and_then(Value::as_str) else { continue; };
        records += 1;
        if let Some(count) = v.get("Data").and_then(|data| fan_count(typ, data)) {
            by_id.insert(id.to_string(), count);
        }
    }
    let groups = by_id.into_iter().enumerate().map(|(i, (_id, count))| GroupEvidence {
        device_key: key_safe(i + 1),
        fan_count: Some(count),
        name: format!("Saved fan controller {}", i+1),
        source: "L-Connect saved FanQuantity (may be wired / historical)".into(),
        live: false,
    }).collect();
    (groups, records)
}
pub fn scan() -> DiscoveryReport {
    let mut warnings = vec![
        "USB PnP detects only adapters, NOT wireless fan groups.".to_string(),
        "FanQuantity comes from historical L-Connect settings; do not treat as live devices.".to_string(),
        "RF GetDev channel discovery is not implemented; no USB commands are sent.".to_string(),
    ];
    let (dongles, lcd_usb_receivers) = match windows_dongles() {
        Ok((adapters, count)) => (adapters, Some(count)),
        Err(_) => {
            // Do not leak PowerShell errors containing local paths or IDs into reports.
            warnings.push("Windows PnP inventory unavailable (local query failed).".into());
            (vec![], None)
        }
    };
    warnings.push("USB 1CBE:0006 receivers may represent individual TL Wireless LCD modules; their count is NOT RF group topology.".into());
    let root = std::env::var_os("PROGRAMDATA").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("Lian-Li").join("L-Connect 3").join("device");
    let (groups, checked) = saved_groups(&root);
    if checked == 0 { warnings.push("No readable L-Connect saved device settings found.".into()); }
    DiscoveryReport { groups, dongles, lcd_usb_receivers, saved_records_checked: checked,
        warnings, live_rf_groups_verified: false }
}
pub fn export(report: &DiscoveryReport) -> Result<PathBuf,String> {
    let root = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let dir = root.join("LianLite");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("wireless-discovery-v012.json");
    let json = serde_json::to_vec_pretty(report).map_err(|e|e.to_string())?;
    fs::write(&path,json).map_err(|e|e.to_string())?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn recognizes_only_exact_adapter_vid_pid() {
        assert_eq!(identify_adapter("USB\\VID_1A86&PID_E304\\A"),Some("L-Wireless V2 TX"));
        assert_eq!(identify_adapter("USB\\VID_1A86&PID_E305\\A"),Some("L-Wireless V2 RX"));
        assert_eq!(identify_adapter("USB\\VID_1A86&PID_2107\\A"),None);
    }
    #[test] fn counts_only_usb_lcd_root_not_interface() {
        assert!(is_lcd_receiver_root("USB\\VID_1CBE&PID_0006\\LCD001"));
        assert!(!is_lcd_receiver_root("USB\\VID_1CBE&PID_0006&MI_00\\LCD001"));
        assert!(!is_lcd_receiver_root("HID\\VID_1CBE&PID_0006\\LCD001"));
    }
    #[test] fn saved_fan_count_is_bounded_and_typed() {
        assert_eq!(fan_count("FanQuantity", &serde_json::json!(3)),Some(3));
        assert_eq!(fan_count("FanQuantity", &serde_json::json!(9)),None);
        assert_eq!(fan_count("Other", &serde_json::json!(3)),None);
    }
    #[test] fn empty_report_never_claims_live_groups() {
        let report=DiscoveryReport { groups:vec![], dongles:vec![], lcd_usb_receivers:None,
            saved_records_checked:0,warnings:vec![],live_rf_groups_verified:false };
        assert_eq!(report.identified_fans(),0);
        assert!(!report.live_rf_groups_verified);
    }
}
