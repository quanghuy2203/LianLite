//! Non-invasive discovery of possible L-Connect IPC interfaces.
//! Does not connect to sockets or pipes, query vendor APIs, or stop services.
//! Only enumerates Windows process metadata, loopback listening endpoints owned
//! by known L-Connect processes, and names of matching pipe candidates.
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceDiscovery {
    pub service_processes: usize,
    pub service_name_families: Vec<String>,
    pub loopback_listening_ports: Vec<u16>,
    pub pipe_candidate_count: usize,
    pub live_group_api_verified: bool,
    pub note: String,
}

const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$names = @(
 'L-Connect-Service.exe',
 'L-Connect-Service-Watcher.exe',
 'LConnectService.exe',
 'L-Connect 3.exe',
 'L-Connect.exe',
 'LConnect3.exe'
)
$owners = @(Get-CimInstance -ClassName Win32_Process -ErrorAction Stop |
    Where-Object { $names -contains $_.Name })
$pidsFound = @($owners | Select-Object -ExpandProperty ProcessId)
$families = @($owners | ForEach-Object {
    switch -Regex ($_.Name) {
        '(?i)watcher' { 'L-Connect watchdog'; break }
        '(?i)service' { 'L-Connect service'; break }
        default { 'L-Connect GUI' }
    }
} | Sort-Object -Unique)
$ports = @()
try {
  $ports = @(Get-NetTCPConnection -State Listen -ErrorAction Stop |
    Where-Object {
      ($pidsFound -contains $_.OwningProcess) -and
      ($_.LocalAddress -in @('127.0.0.1', '::1'))
    } | Select-Object -ExpandProperty LocalPort | Sort-Object -Unique)
} catch { $ports = @() }
# Only enumerate pipe directory names. We do NOT open or connect to a pipe.
$pipeCandidates = @()
try {
  $pipeCandidates = @(Get-ChildItem -LiteralPath '\\.\pipe\' -ErrorAction Stop |
    Where-Object {
      $_.Name -match '(?i)(lian.?li|l.?connect)'
    })
} catch { $pipeCandidates = @() }
[ordered]@{
  service_processes = @($owners).Count
  service_name_families = @($families)
  loopback_listening_ports = @($ports)
  pipe_candidate_count = @($pipeCandidates).Count
  live_group_api_verified = $false
  note = 'Windows metadata only; endpoint presence does not establish an API or provide RF groups.'
} | ConvertTo-Json -Compress -Depth 5
"#;

fn validate(mut result: ServiceDiscovery) -> Result<ServiceDiscovery, String> {
    // Reject unusual / malicious data rather than echoing it in the GUI.
    if result.service_processes > 64 || result.pipe_candidate_count > 4096
        || result.loopback_listening_ports.len() > 128
        || result.service_name_families.len() > 6 {
        return Err("Service discovery response exceeds safe bounds".into());
    }
    for family in &result.service_name_families {
        if !matches!(family.as_str(),
            "L-Connect watchdog" | "L-Connect service" | "L-Connect GUI") {
            return Err("Unrecognized service process type".into());
        }
    }
    result.loopback_listening_ports.sort_unstable();
    result.loopback_listening_ports.dedup();
    result.live_group_api_verified = false; // never infer from listener metadata
    result.note = "Read-only Windows process/socket/pipe metadata. No live channel API verified.".into();
    Ok(result)
}
fn parse(response: &[u8]) -> Result<ServiceDiscovery, String> {
    if response.len() > 65536 {
        return Err("Service discovery output exceeds 64 KiB".into());
    }
    let raw: ServiceDiscovery = serde_json::from_slice(response)
        .map_err(|_| "Windows service discovery returned invalid JSON".to_string())?;
    validate(raw)
}
pub fn discover() -> Result<ServiceDiscovery, String> {
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let out = command.output()
        .map_err(|_| "Cannot start Windows read-only service discovery".to_string())?;
    if !out.status.success() {
        return Err("Windows denied or failed read-only process/endpoint enumeration".into());
    }
    parse(&out.stdout)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_service_metadata_does_not_claim_a_live_api() {
        let json = br#"{"service_processes":2,"service_name_families":["L-Connect service","L-Connect watchdog"],"loopback_listening_ports":[32100,32100,32101],"pipe_candidate_count":1,"live_group_api_verified":true,"note":"pretend API"}"#;
        let found = parse(json).unwrap();
        assert_eq!(found.loopback_listening_ports, vec![32100,32101]);
        assert!(!found.live_group_api_verified);
        assert!(!found.note.contains("pretend"));
    }
    #[test]
    fn unknown_service_labels_cannot_leak_to_report() {
        let json = br#"{"service_processes":1,"service_name_families":["C:\\private\\serial"],"loopback_listening_ports":[],"pipe_candidate_count":0,"live_group_api_verified":false,"note":"secret"}"#;
        assert!(parse(json).is_err());
    }
    #[test]
    fn huge_output_is_rejected() {
        assert!(parse(&vec![b'a';65537]).is_err());
    }
}
