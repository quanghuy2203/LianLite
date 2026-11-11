//! Offline string-reference correlation, not an IPC connection.
//! Reads on-disk L-Connect executables only. Does NOT open any pipe or socket.
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, process::Command};

const MAX_EXE: u64 = 32 * 1024 * 1024;
const MAX_PROCESSES: usize = 12;
const MAX_PIPES: usize = 64;

#[derive(Deserialize)]
struct ProbeProcess {
    image_path: String,
    family: String,
}
#[derive(Deserialize)]
struct Probe {
    processes: Vec<ProbeProcess>,
    pipe_names: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceReport {
    pub candidate_pipes: usize,
    pub readable_executables: usize,
    pub referenced_candidate_names: usize,
    pub matched_families: Vec<String>,
    pub api_verified: bool,
    pub note: &'static str,
}
const PS: &str = r#"
$ErrorActionPreference = 'Stop'
$names = @('L-Connect-Service.exe','L-Connect-Service-Watcher.exe',
           'LConnectService.exe','L-Connect 3.exe','L-Connect.exe','LConnect3.exe')
$proc = @(Get-CimInstance Win32_Process -ErrorAction Stop |
  Where-Object { $names -contains $_.Name } | Select-Object -First 12)
$items = @($proc | ForEach-Object {
  $family = if ($_.Name -match '(?i)watcher') { 'watchdog' }
            elseif ($_.Name -match '(?i)service') { 'service' }
            else { 'gui' }
  [ordered]@{image_path = [string]$_.ExecutablePath;family = $family}
})
$pipes = @()
try {
  $pipes = @(Get-ChildItem -LiteralPath '\\.\pipe\' -ErrorAction Stop |
    Where-Object { $_.Name -match '(?i)(lian.?li|l.?connect)' } |
    Select-Object -First 64 -ExpandProperty Name)
} catch { $pipes = @() }
[ordered]@{processes=$items;pipe_names=$pipes} | ConvertTo-Json -Compress -Depth 5
"#;

fn includes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() { return false; }
    haystack.windows(needle.len()).any(|window| window == needle)
}
fn referenced(bytes: &[u8], pipe_name: &str) -> bool {
    // A short generic name is not useful evidence of an IPC reference.
    if pipe_name.len() < 8 || pipe_name.len() > 128 || !pipe_name.is_ascii() {
        return false;
    }
    let ascii = pipe_name.as_bytes();
    let wide: Vec<u8> = pipe_name.encode_utf16()
        .flat_map(|unit| unit.to_le_bytes()).collect();
    includes(bytes, ascii) || includes(bytes, &wide)
}
fn analyze(probe: Probe) -> Result<ReferenceReport, String> {
    if probe.processes.len() > MAX_PROCESSES || probe.pipe_names.len() > MAX_PIPES {
        return Err("Too many process or pipe candidates".into());
    }
    let mut names = Vec::new();
    for name in probe.pipe_names {
        if name.len() < 8 || name.len() > 128 || !name.is_ascii()
            || name.chars().any(|c| c.is_control() || c == '/' || c == '\\')
            || !name.to_ascii_lowercase().contains("connect")
                && !name.to_ascii_lowercase().contains("lian") {
            continue;
        }
        names.push(name);
    }
    names.sort_unstable();
    names.dedup();
    let mut readable = 0;
    let mut matched = std::collections::BTreeSet::new();
    let mut families = std::collections::BTreeSet::new();
    for process in probe.processes {
        if !matches!(process.family.as_str(), "gui"|"watchdog"|"service") {
            continue;
        }
        let path = Path::new(&process.image_path);
        if path.extension().and_then(|v| v.to_str()).is_none_or(|v| !v.eq_ignore_ascii_case("exe")) {continue;}
        let Ok(metadata) = fs::metadata(path) else {continue;};
        if metadata.len() == 0 || metadata.len() > MAX_EXE || !metadata.is_file() {continue;}
        let Ok(bytes) = fs::read(path) else {continue;};
        readable += 1;
        for (index, name) in names.iter().enumerate() {
            if referenced(&bytes, name) {
                matched.insert(index);
                families.insert(process.family.clone());
            }
        }
    }
    Ok(ReferenceReport {
        candidate_pipes: names.len(),
        readable_executables: readable,
        referenced_candidate_names: matched.len(),
        matched_families: families.into_iter().collect(),
        api_verified: false,
        note: "On-disk EXE string match only, not proof of pipe ownership, supported API or live topology.",
    })
}
pub fn inspect() -> Result<ReferenceReport, String> {
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile","-NonInteractive","-Command",PS]);
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.output().map_err(|_| "Cannot start read-only binary reference inspection")?;
    if !output.status.success() || output.stdout.len() > 65536 {
        return Err("Windows process/pipe metadata unavailable".into());
    }
    let probe: Probe = serde_json::from_slice(&output.stdout)
        .map_err(|_| "Invalid Windows metadata response".to_string())?;
    analyze(probe)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn detects_ascii_and_utf16_references() {
        assert!(referenced(b"prefix LConnectPipeName suffix", "LConnectPipeName"));
        let utf16:Vec<u8> = "LConnectPipeName".encode_utf16()
            .flat_map(|u| u.to_le_bytes()).collect();
        assert!(referenced(&utf16,"LConnectPipeName"));
        assert!(!referenced(b"unrelated","LConnectPipeName"));
        assert!(!referenced(b"Connect","Connect"));
    }
    #[test] fn rejects_oversize_and_never_claims_api() {
        let report=analyze(Probe {processes:vec![],pipe_names:vec!["LConnectPipeName".into()]}).unwrap();
        assert_eq!(report.candidate_pipes,1);
        assert!(!report.api_verified);
        assert_eq!(report.referenced_candidate_names,0);
        assert!(analyze(Probe { processes:vec![],pipe_names:vec!["a".into();65] }).is_err());
    }
}
