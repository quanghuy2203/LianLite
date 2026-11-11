#![windows_subsystem = "windows"]
use std::{fs, path::PathBuf, process::Command, thread, time::Duration};
fn main() {
    if let Err(e) = run() {
        // GUI subsystem has no terminal: write errors to a file the user can inspect.
        let base = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir());
        let dir = base.join("LianLite");
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(dir.join("updater-last-error.txt"), format!("LianLite update failed: {e}\n"));
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 4 || args[1] != "--apply" {
        return Err("Usage: lianlite-updater --apply <app.exe> <staged>".into());
    }
    let target = PathBuf::from(&args[2]);
    let staged = PathBuf::from(&args[3]);
    if !target.is_absolute() || !staged.is_absolute() || staged != target.with_extension("exe.pending")
        || !target.file_name().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("lianlite.exe")) {
        return Err("Invalid update paths".into());
    }
    let backup = target.with_extension("exe.backup");
    // Keep the backup until the new app is confirmed to have started.
    let mut renamed = false;
    for _ in 0..120 {
        if backup.exists() {
            return Err("Existing backup found; refusing to overwrite it".into());
        }
        match fs::rename(&target, &backup) {
            Ok(()) => { renamed = true; break; }
            Err(_) => thread::sleep(Duration::from_millis(500)),
        }
    }
    if !renamed { return Err("Timed out waiting for app to exit".into()); }
    if let Err(e) = fs::rename(&staged, &target) {
        let _ = fs::rename(&backup, &target);
        return Err(format!("Install failed (rollback attempted): {e}"));
    }
    match Command::new(&target).spawn() {
        Ok(_) => {
            // Keep backup for manual recovery if the newly launched app crashes later.
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&target);
            let _ = fs::rename(&backup, &target);
            let _ = Command::new(&target).spawn();
            Err(format!("New app could not start; restored previous version: {e}"))
        }
    }
}
