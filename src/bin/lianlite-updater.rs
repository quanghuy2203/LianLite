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
// This executable is launched from a version-specific staging directory,
// never from the installed directory. That permits replacing the installed
// helper after successfully starting the updated main application.
fn install_new_helper(target: &Path) -> Result<(), String> {
    let running_helper = std::env::current_exe().map_err(|e| e.to_string())?;
    let installed = target.with_file_name("lianlite-updater.exe");
    let pending = target.with_file_name("lianlite-updater.exe.new");
    let archive = (1..=64)
        .map(|i| target.with_file_name(format!("lianlite-updater.exe.backup.{i}")))
        .find(|path| !path.exists())
        .ok_or("Too many old updater backups")?;
    if pending.exists() { fs::remove_file(&pending).map_err(|e| e.to_string())?; }
    fs::copy(&running_helper, &pending).map_err(|e| format!("Copy staged updater: {e}"))?;
    let had_old = installed.exists();
    if had_old {
        if let Err(e) = fs::rename(&installed, &archive) {
            let _ = fs::remove_file(&pending);
            return Err(format!("Preserve installed updater: {e}"));
        }
    }
    if let Err(e) = fs::rename(&pending, &installed) {
        if had_old { let _ = fs::rename(&archive, &installed); }
        return Err(format!("Install new updater: {e}"));
    }
    Ok(())
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
    // Older installers blocked all future updates after two backups.
    // Rotate safely into a unique numbered backup without discarding copies.
    if backup.exists() {
        let archive = (1..=64)
            .map(|n| target.with_extension(format!("exe.backup.previous.{n}")))
            .find(|p| !p.exists())
            .ok_or("All 64 previous-backup slots are occupied; manual cleanup required")?;
        fs::rename(&backup, &archive)
            .map_err(|e| format!("Cannot preserve old backup at {}: {e}", archive.display()))?;
        log(&format!("Existing backup safely archived to {}.", archive.display()));
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
            match install_new_helper(&target) {
                Ok(()) => log("App and updater installed; app is running. Older backups retained."),
                Err(e) => log(&format!("Main app updated and running; updater replacement needs attention: {e}")),
            }
            Ok(())
        }
        Err(e) => {
            log(&format!("Update installed; launch status could not be checked: {e}. Backup retained."));
            Ok(())
        }
    }
}
