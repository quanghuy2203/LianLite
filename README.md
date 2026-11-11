# LianLite v0.9.0 — Native Rust dashboard for Windows 11

LianLite is a native Rust GUI for **read-only diagnostics** and **local configuration previews**. No OpenRGB installation is required.

## Target setup
- Lian Li TL Wireless LCD — 3-fan group + 2-fan group
- Lian Li TL Wireless non-LCD — 3-fan group
- 2 Strimer Wireless cables
- ASUS ROG Strix B650E-F Gaming WiFi — AURA HID 0B05:19AF (MI_02), plus MI_00 WinUSB-compatible interface
- Zotac RTX 4070 Ti Trinity OC
- Lexar ARES V2 DDR5-6000 CL28 (2 × 16 GB)

## New in v0.8
1. Preview-only fan presets **Silent 35%**, **Balanced 65%**, **Performance 85%**. These percentages do not affect fan RPM.
2. Global RGB color preview applied to all three fan groups with one click.
3. Brightness preview shared across all fan groups and both Strimer representations.
4. Local profile persistence via `%APPDATA%\LianLite\config.json`. Values are validated on load.
5. **Check install / updater** checks presence of helper, backup, older backup and pending update.
6. **Open LianLite reports folder** opens Explorer directly so Windows usernames with spaces aren't a problem.
7. **Export privacy-safe diagnostic summary** writes `%APPDATA%\LianLite\diagnostic-summary-v08.json`, removing PnP Instance IDs and hardware serials. It displays only device type flags, GPU name, RAM part number, ASUS interface presence and updater file presence.
8. Structured ASUS read-only diagnostic: recognizes MI_00, MI_02 and WinUSB-compatible descriptors from PnP output, but **does not send USB packets**.
9. Updater can archive repeated older `.backup` files into numbered backups (up to 64 slots) instead of stopping at the second update; remains subject to helper being updated separately.
10. Added Rust unit tests for profile safety, PnP descriptor summary and redaction.

## Build and release
GitHub Actions builds automatically **once after the changes are merged to main**; feature branches do not run release jobs. The workflow tests, builds, uploads an artifact and publishes a GitHub Release only when the Cargo package version is new. Existing release tags are not overwritten. Windows build artifacts are not themselves in-app updates.

To build manually on Windows 11 with MSVC Build Tools:
```powershell
cargo test --bins
cargo build --release --bins
```
Executables: `target\release\lianlite.exe`, `target\release\lianlite-updater.exe`. The GUI and helper use the Windows GUI subsystem, so they do not show a CMD window on double-click.

## Existing diagnostics
- **Detect ASUS + Lian Li HID** enumerates the exact USB HID VID/PID candidates.
- **Detect GPU + RAM** uses Windows CIM in read-only mode.
- **Inspect ASUS Aura (read-only)** queries matching Windows PnP properties; **Export ASUS report** saves a detailed JSON which **does contain** device instance IDs. Review before sharing.
- **Export privacy-safe diagnostic summary** is recommended for sharing instead of full raw PnP inventory.
- **Check Lian Li processes** reads process names; never stops them.

## Safety and limits
**This is NOT yet a replacement for L-Connect 3.** All fan speed, RGB and brightness controls are local previews only. The HID interface `0B05:19AF` is detected, but its real LED protocol is not verified. No USB HID writes, WinUSB transfers, SMBus/I2C GPU/RAM access, LCD control, firmware flashing, process killing or service disabling are performed. Keep the official L-Connect services running for cooling and LCD functionality.

The auto-update client currently updates the main executable only. It does not upgrade `lianlite-updater.exe` in-place. If the installed helper is old, use the portable ZIP or `lianlite-updater.exe` from the release to replace the helper **while LianLite is closed**. Backups are retained for manual recovery; don't delete them without verifying that the newest version is stable. A process remaining alive for three seconds does not guarantee long-term health.

## Privacy
The privacy-safe summary excludes PnP Instance IDs, serials and filesystem paths. GPU names and RAM part numbers remain. Raw `asus-aura-pnp-v07.json` and `native-hardware-v06.json` may contain sensitive machine identifiers; review these files before sharing.

## Next engineering steps
- Establish an exact compatible ASUS Aura protocol for `0B05:19AF` with read-only verification and device-specific tests before writes.
- Determine whether Lian Li `SLV3H 1A86:2107` is related to the wireless controller.
- Research GPU SPECTRA and Lexar RAM RGB controllers without guessing SMBus writes.
- Design a signed, transactional installer/updater capable of safely replacing the helper too.
- Measure actual Windows memory use after build (no RAM reduction is asserted without benchmarks).

## v0.8.1 — updater self-upgrade fix

**Problem:** Previous versions download and replace only `lianlite.exe`. The old helper `lianlite-updater.exe` stays installed and can fail on pre-existing backups. The application may close without restarting.

**Fix:** From v0.8.1 onward, the update client downloads `LianLite-Portable-Windows-x64.zip` **plus** its `.sha256` checksum from the public GitHub Release. The ZIP must contain exactly `lianlite.exe` and `lianlite-updater.exe`, both recognizable as Windows executable files. The app extracts the helper to a versioned folder outside the installation directory and stages the new GUI beside the current executable. On user confirmation it launches this newly downloaded helper. This helper safely rotates existing backups, replaces the main app, starts it, checks for an immediate exit, and then replaces the installed updater while preserving the old copy. If launching the new GUI fails immediately, it attempts to restore the previous GUI.

All updates are user-initiated; no background installer, USB writes or admin permissions required. Release SHA-256 validates download integrity but is **not a digital signature** and does not protect against a compromised GitHub publishing account.

**One-time migration:** An already-installed v0.8.0 or older app uses its old update logic. Updating the main executable to v0.8.1 using the legacy helper may still fail if old backups exist. In that case, the safest bootstrap is to exit LianLite, back up the working folder, download the `LianLite-Portable-Windows-x64.zip` **from the official v0.8.1 GitHub Release**, and extract both executables into a fresh writable folder, for example `%LOCALAPPDATA%\LianLite-App`. Open `lianlite.exe` from there. Existing profile settings remain in `%APPDATA%\LianLite\config.json`. Do not delete the old working folder or backups until the new install has been tested.

Once v0.8.1 is installed, later releases can upgrade the installed helper as well as the GUI. Existing old backups remain preserved under numbered file names. Status logs (new helper) are in `%LOCALAPPDATA%\LianLite\updater-last-status.txt` and `updater-last-error.txt`. The app cannot guarantee long-term health based only on checking that the new process stays running for three seconds.

**Note:** Do not describe this as an end-to-end tested updater until a real Windows 11 upgrade test completes successfully; GitHub Actions only verifies compilation/unit tests and package generation.

## v0.9: redesigned dashboard & ASUS HID descriptor

### UI redesign
- Native egui GUI; dark navy design with cyan accents, sidebar navigation, device cards and a bottom status bar.
- **Overview**: grouped counters for 5 TL Wireless LCD fans (3+2), 3 non-LCD fans, 2 Strimers, three local profile presets; preview-only RGB/speed/brightness sliders and Save local profile.
- **Devices**: ASUS/Lian Li HID scan, GPU/RAM inventory, PnP inspection and the new ASUS HID descriptor inspection.
- **Diagnostics**: export ASUS descriptor or ASUS PnP report, privacy-safe diagnostic summary, open report folder, process health.
- **Updates**: version check, download and install via GitHub Releases; updater and backup diagnostics.
- GUI uses `#![windows_subsystem = "windows"]` and does not open a console window when launched normally.

### ASUS descriptor (read-only)
The new **Read HID descriptor** button only opens the exact ASUS Aura interface with VID 0B05, PID 19AF, interface 2, usage page FF72 and usage 00A1. It calls HIDAPI's `get_report_descriptor()` and **never sends a HID feature/output report, WinUSB transfer or RGB command**. On Windows this may be a *reconstructed* descriptor rather than a raw physical USB descriptor. Opening the device may fail if another application holds an exclusive handle. A failure is reported and does not trigger any retry with write access.

After clicking **Read HID descriptor** on the Devices tab, open Diagnostics and use **Export HID descriptor** to save `%APPDATA%\LianLite\aura-hid-descriptor-v09.json`. This contains descriptor data and device/product metadata, not device serial number. Verify the content before sharing.

Controller `0B05:19AF` can behave differently from earlier `0B05:18F3` Aura USB products: successful descriptor retrieval **does not verify** the LED write protocol or zone mapping.

Reference context:
- https://docs.rs/hidapi/latest/hidapi/struct.HidDevice.html
- https://gitlab.com/CalcProgrammer1/OpenRGB/-/issues/1390
- https://gitlab.com/CalcProgrammer1/OpenRGB/-/issues/3929

### Build and release
Development takes place on a dedicated feature branch, so CI does not automatically publish each incremental change. Merge triggers one main-branch Windows Actions job. Build/test success and publication need explicit verification. **No fan, LCD, ASUS, ZOTAC or Lexar hardware write support is enabled.** Continue using L-Connect's background service.

The in-app updater uses the verified portable ZIP pair from v0.8.1; it needs testing on the user's Windows 11 machine for recovery across multiple upgrades.
