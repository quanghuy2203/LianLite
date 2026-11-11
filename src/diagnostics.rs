//! A shareable diagnostic summary that intentionally omits PnP instance IDs,
//! hardware serials, drive paths, MACs, and process command lines.
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
use crate::{native::NativeDevice, health::Health};
fn present<T>(values: &[T]) -> bool { !values.is_empty() }
pub fn summary(hid: &[NativeDevice], gpu_ram: &str, aura_pnp: &str, health: &Health) -> Value {
    let parsed: Value = serde_json::from_str(gpu_ram).unwrap_or(Value::Null);
    let graphic_names = parsed.get("Graphics")
        .and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.get("Name").and_then(Value::as_str)).collect::<Vec<_>>())
        .unwrap_or_default();
    let ram_parts = parsed.get("Memory")
        .and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.get("PartNumber").and_then(Value::as_str)).collect::<Vec<_>>())
        .unwrap_or_default();
    let aura: Value = serde_json::from_str(aura_pnp).unwrap_or(Value::Null);
    let aura_entries = aura.get("DeviceCount").and_then(Value::as_u64).unwrap_or(0);
    let aura_found = hid.iter().any(|d| d.id == "VID_0B05&PID_19AF");
    let lianli_found = hid.iter().any(|d| d.id == "VID_1A86&PID_2107");
    json!({
        "LianLiteVersion": health.version,
        "ReadOnly": true,
        "HardwareWritesSupported": false,
        "DetectedHid": {
            "AsusAura_0B05_19AF": aura_found,
            "LianLiSLV3H_1A86_2107": lianli_found,
            "DistinctHidUsages": hid.len()
        },
        "AsusPnPEntries": aura_entries,
        "GpuNames": graphic_names,
        "RamPartNumbers": ram_parts,
        "Updater": {
            "HelperPresent": health.updater_present,
            "BackupPresent": health.backup_present,
            "PreviousBackupPresent": health.old_backup_present,
            "PendingPresent": health.pending_present
        },
        "Privacy": "PnP instance IDs, serial numbers, and file paths are omitted."
    })
}
pub fn export(summary: &Value) -> Result<PathBuf, String> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("LianLite");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("diagnostic-summary-v08.json");
    let bytes = serde_json::to_vec_pretty(summary).map_err(|e| e.to_string())?;
    fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn privacy_no_instance_id() {
        let h = Health { version: "0.8.0", updater_present: true, backup_present: false,
            old_backup_present: false, pending_present: false,
            last_update_status: None, last_update_error: None };
        let data = r#"{"Graphics":[{"Name":"GPU","PNPDeviceID":"SERIAL_SECRET"}],"Memory":[{"PartNumber":"RAM-PART"}]}"#;
        let summary = summary(&[], data, "", &h).to_string();
        assert!(!summary.contains("SERIAL_SECRET"));
        assert!(summary.contains("RAM-PART"));
    }
}
