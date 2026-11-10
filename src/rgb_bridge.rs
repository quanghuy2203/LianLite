//! Optional OpenRGB adapter. Never writes to arbitrary USB/HID endpoints.
use std::{io, net::{SocketAddr, TcpStream}, path::PathBuf, process::Command, time::Duration};

fn openrgb_binary() -> PathBuf {
    let adjacent = std::env::current_exe().ok().and_then(|p| p.parent().map(|dir| dir.join("OpenRGB.exe")));
    if let Some(path) = adjacent { if path.is_file() { return path; } }
    // Only use explicit executable name, never cmd.exe or an arbitrary script.
    PathBuf::from("OpenRGB.exe")
}

fn server_available() -> Result<(), String> {
    let addr: SocketAddr = "127.0.0.1:6742".parse().map_err(|e: std::net::AddrParseError| e.to_string())?;
    TcpStream::connect_timeout(&addr, Duration::from_millis(800))
        .map(|_| ()).map_err(|_| "OpenRGB SDK server not detected on 127.0.0.1:6742. Open OpenRGB and enable its SDK Server first.".into())
}

fn invoke(args: &[&str]) -> Result<String, String> {
    server_available()?;
    let output = Command::new(openrgb_binary())
        .arg("--client").arg("127.0.0.1:6742")
        .args(args)
        .output()
        .map_err(|e| format!("OpenRGB.exe could not be started: {e}. Install OpenRGB and put OpenRGB.exe next to lianlite.exe, or in PATH."))?;
    if !output.status.success() {
        return Err(format!("OpenRGB exited with {}: {}", output.status, String::from_utf8_lossy(&output.stderr)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[derive(Clone, Debug)]
pub struct Device { pub index: usize, pub name: String }

pub fn list() -> Result<(String, Vec<Device>), String> {
    let output = invoke(&["--list-devices"])?;
    let mut devices = Vec::new();
    for line in output.lines() {
        if let Some((idx, name)) = line.trim().split_once(':') {
            if let Ok(index) = idx.trim().parse::<usize>() {
                let name = name.trim();
                if !name.is_empty() && index < 128 {
                    devices.push(Device { index, name: name.to_owned() });
                }
            }
        }
    }
    Ok((output, devices))
}

pub fn apply_color(device: usize, rgb: [u8; 3]) -> Result<String, String> {
    if device >= 128 { return Err("Invalid device index".into()); }
    let index = device.to_string();
    let hex = format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
    invoke(&["--device", &index, "--color", &hex])
}

// Read-only Windows PnP enumeration: independent of HID interface detection.
pub fn scan_windows_usb() -> Result<String, String> {
    let ps = r#"Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -match '^(USB|HID)\\' } | Select-Object -First 120 FriendlyName,Class,InstanceId,Status | ConvertTo-Json -Depth 3"#;
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps])
        .output().map_err(|e| format!("PowerShell could not start: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}
