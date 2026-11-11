#![windows_subsystem = "windows"]
use std::{fs, path::{Path, PathBuf}, process::Command, thread, time::Duration};

fn state_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("LianLite")
}
fn log(message: &str) {
    let dir = state_dir();
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(dir.join("updater-last-status.txt"), message);
}
fn fail(message: &str) {
    let dir = state_dir();
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(dir.join("updater-last-error.txt"), message);
    log(message);
}
fn main() {
    log("Updater started. Waiting for the old application to close.");
    if let Err(error) = run() {
        fail(&format!("LianLite update failed: {error}"));
        std::process::exit(1);
    }
}
fn restore(backup: &Path, target: &Path) -> Result<(), String> {
    if target.exists() {
        fs::remove_file(target).map_err(|e| format!("Cannot remove failed new executable: {e}"))?;
    }
    fs::rename(backup, target).map_err(|e| format!("Cannot restore backup: {e}"))
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 4 || args[1] != "--apply" {
        return Err("Usage: lianlite-updater --apply <app.exe> <staged>".into());
    }
    let target = PathBuf::from(&args[2]);
    let staged = PathBuf::from(&args[3]);
    if !target.is_absolute()
        || !staged.is_absolute()
        || staged != target.with_extension("exe.pending")
        || !target.file_name().and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("lianlite.exe"))
    {
        return Err("Invalid update paths".into());
    }
    if !staged.is_file() { return Err("Downloaded update (.pending) not found".into()); }
    let dir = target.parent().ok_or("Missing install directory")?;
    let backup = target.with_extension("exe.backup");
    let previous = target.with_extension("exe.backup.previous");

    // Never discard an existing backup. Keep an older copy separately.
    if backup.exists() {
        if previous.exists() {
            return Err(format!("Both backup files exist ({}, {}). Review them before updating.", backup.display(), previous.display()));
        }
        fs::rename(&backup, &previous)
            .map_err(|e| format!("Cannot preserve existing backup: {e}"))?;
        log("Previous backup preserved as lianlite.exe.backup.previous.");
    }

    // Windows does not allow an executing EXE to be renamed until it has exited.
    let mut ready = false;
    for _ in 0..120 {
        match fs::rename(&target, &backup) {
            Ok(()) => { ready = true; break; }
            Err(_) => thread::sleep(Duration::from_millis(500)),
        }
    }
    if !ready { return Err("Timeout waiting for LianLite to close (60s)".into()); }

    if let Err(e) = fs::rename(&staged, &target) {
        let rollback = restore(&backup, &target);
        return Err(format!("Could not install new EXE: {e}. Rollback: {rollback:?}"));
    }

    let mut child = match Command::new(&target).current_dir(dir).spawn() {
        Ok(child) => child,
        Err(e) => {
            let rollback = restore(&backup, &target);
            if rollback.is_ok() { let _ = Command::new(&target).current_dir(dir).spawn(); }
            return Err(format!("Cannot start new EXE: {e}. Rollback: {rollback:?}"));
        }
    };
    // Catch an immediate process crash, but keep a manual backup for later crashes.
    thread::sleep(Duration::from_secs(3));
    match child.try_wait() {
        Ok(Some(status)) => {
            let rollback = restore(&backup, &target);
            if rollback.is_ok() { let _ = Command::new(&target).current_dir(dir).spawn(); }
            Err(format!("New version exited immediately ({status}). Rollback: {rollback:?}"))
        }
        Ok(None) => {
            let error_file = state_dir().join("updater-last-error.txt");
            let _ = fs::remove_file(error_file);
            log("Update installed; new LianLite process is still running after 3 seconds. Backup retained.");
            Ok(())
        }
        Err(e) => {
            log(&format!("Update installed; launch status could not be checked: {e}. Backup retained."));
            Ok(())
        }
    }
}
