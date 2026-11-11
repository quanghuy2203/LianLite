//! Single privacy-conscious support snapshot built from multiple read-only surveys.
use crate::{diagnostics, discovery::DiscoveryReport, health::Health,
    native::NativeDevice, wireless_research::ResearchReport};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

pub fn compose(
    hid: &[NativeDevice],
    gpu_ram: &str,
    aura_pnp: &str,
    health: &Health,
    wireless: Option<&DiscoveryReport>,
    schema: Option<&ResearchReport>,
) -> Value {
    let hardware = diagnostics::summary(hid, gpu_ram, aura_pnp, health);
    let adapters = wireless.map(|report| report.dongles.iter().filter_map(|dongle| {
        // Even if the underlying model string ever becomes externally sourced,
        // export only canonical allowlisted model labels.
        match dongle.model.as_str() {
            "L-Wireless V1 TX" => Some("L-Wireless V1 TX"),
            "L-Wireless V1 RX" => Some("L-Wireless V1 RX"),
            "L-Wireless V2 TX" => Some("L-Wireless V2 TX"),
            "L-Wireless V2 RX" => Some("L-Wireless V2 RX"),
            _ => None,
        }
    }).collect::<Vec<_>>()).unwrap_or_default();
    json!({
        "ReportVersion": 12,
        "ReadOnly": true,
        "Hardware": hardware,
        "Wireless": {
            "AdapterTypes": adapters,
            "SavedRecordCount": schema.map(|r| r.typed_records).unwrap_or(0),
            "SavedSettingTypes": schema.map(|r| &r.record_types),
            "SavedFieldStructure": schema.map(|r| &r.fields),
            "DataIncomplete": wireless.is_none() || schema.is_none(),
            "LiveBoundGroupsVerified": false,
            "Note": "Offline saved settings are not live wireless devices; no RF query sent."
        },
        "Privacy": "No DeviceID, USB path, MAC, PnP instance ID, serial or raw setting values exported."
    })
}
pub fn export(snapshot: &Value) -> Result<PathBuf,String> {
    let base = std::env::var_os("APPDATA").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("LianLite");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("lianlite-support-snapshot-v012.json");
    fs::write(&path, serde_json::to_vec_pretty(snapshot).map_err(|e|e.to_string())?)
        .map_err(|e|e.to_string())?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::{DongleEvidence, DiscoveryReport};
    #[test] fn snapshot_excludes_unknown_adapter_identity() {
        let health = Health {
            version:"0.12.0", updater_present:true, backup_present:false,
            old_backup_present:false, pending_present:false,
            last_update_status:Some("PRIVATE_DEVICE_ID".into()),
            last_update_error:None
        };
        let wireless = DiscoveryReport {
            groups:vec![], dongles:vec![DongleEvidence {
                model:"PRIVATE_ADAPTER_MODEL".into(), detected:true
            }], saved_records_checked:0, warnings:vec![],
            live_rf_groups_verified:false
        };
        let output=compose(&[], "{}", "", &health, Some(&wireless), None).to_string();
        assert!(!output.contains("PRIVATE_ADAPTER_MODEL"));
        assert!(!output.contains("PRIVATE_DEVICE_ID"));
        assert!(!output.contains("serial-token"));
        assert!(output.contains("LiveBoundGroupsVerified"));
    }
}
