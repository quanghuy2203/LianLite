//! Safe local updater installation diagnostics (no device writes).
use serde::Serialize;
use std::{fs, path::PathBuf};
#[derive(Serialize)]
pub struct Health {
    pub version: &'static str,
    pub updater_present: bool,
    pub backup_present: bool,
    pub old_backup_present: bool,
    pub pending_present: bool,
    pub last_update_status: Option<String>,
    pub last_update_error: Option<String>,
}
fn local_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir).join("LianLite")
}
fn read_bounded(path: PathBuf) -> Option<String> {
    let raw = fs::read(path).ok()?;
    if raw.len() > 4096 { return None; }
    String::from_utf8(raw).ok()
}
pub fn inspect() -> Result<Health, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("No executable parent directory")?;
    let logs = local_dir();
    Ok(Health {
        version: env!("CARGO_PKG_VERSION"),
        updater_present: dir.join("lianlite-updater.exe").is_file(),
        backup_present: exe.with_extension("exe.backup").is_file(),
        old_backup_present: exe.with_extension("exe.backup.previous").is_file(),
        pending_present: exe.with_extension("exe.pending").is_file(),
        last_update_status: read_bounded(logs.join("updater-last-status.txt")),
        last_update_error: read_bounded(logs.join("updater-last-error.txt")),
    })
}
