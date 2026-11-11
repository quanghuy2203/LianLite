//! Conservatively prune only LianLite-generated historical backups.
//! Keep current main rollback (.exe.backup) and the newest updater backup.
//! Never remove unknown filenames, .pending, the installed executables or logs.
use std::{fs, path::{Path, PathBuf}, time::SystemTime};

#[derive(Debug, Default)]
pub struct Cleanup {
    pub deleted: usize,
    pub retained: usize,
    pub failed: Vec<String>,
}

fn is_old_main(name: &str) -> bool {
    name == "lianlite.exe.backup.previous" ||
        name.strip_prefix("lianlite.exe.backup.previous.")
            .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}
fn is_updater_backup(name: &str) -> bool {
    name == "lianlite-updater.exe.backup" ||
        name.strip_prefix("lianlite-updater.exe.backup.")
            .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}
fn modified(path: &Path) -> SystemTime {
    fs::metadata(path).and_then(|v| v.modified()).unwrap_or(SystemTime::UNIX_EPOCH)
}
pub fn cleanup(dir: &Path) -> Result<Cleanup, String> {
    let mut old_main = Vec::<PathBuf>::new();
    let mut updater = Vec::<PathBuf>::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if !kind.is_file() || kind.is_symlink() { continue; }
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else { continue; };
        if is_old_main(name) { old_main.push(entry.path()); }
        else if is_updater_backup(name) { updater.push(entry.path()); }
    }
    // Retain the newest updater fallback; do not remove the current main rollback.
    updater.sort_by_key(|p| (modified(p), p.clone()));
    let newest_updater = updater.pop();
    let mut report = Cleanup {
        deleted: 0,
        retained: usize::from(dir.join("lianlite.exe.backup").is_file()) +
            usize::from(newest_updater.is_some()),
        failed: vec![],
    };
    for path in old_main.into_iter().chain(updater) {
        match fs::remove_file(&path) {
            Ok(()) => report.deleted += 1,
            Err(e) => report.failed.push(format!("{}: {e}",path.display())),
        }
    }
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn only_known_older_backup_names() {
        assert!(is_old_main("lianlite.exe.backup.previous.1"));
        assert!(is_old_main("lianlite.exe.backup.previous.3"));
        assert!(!is_old_main("lianlite.exe.backup"));
        assert!(!is_old_main("lianlite.exe.backup.previous.notes"));
        assert!(is_updater_backup("lianlite-updater.exe.backup.23"));
        assert!(!is_updater_backup("lianlite-updater.exe.backup.old"));
        assert!(!is_updater_backup("another-updater.exe.backup.1"));
    }
}
