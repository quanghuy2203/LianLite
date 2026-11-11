# LianLite v0.8.0 — Ultra Minimal Rust Native (Windows 11)

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
