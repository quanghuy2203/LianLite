# LianLite v0.7.1 — Rust Native (Windows 11)

Lightweight GUI in Rust/egui. **No OpenRGB dependency.**

## Hardware layout

- Lian Li TL Wireless LCD: group of 3 + group of 2
- Lian Li TL Wireless non-LCD: group of 3
- 2 Lian Li Strimer Wireless cables
- ASUS ROG Strix B650E-F Gaming WiFi (Aura)
- ZOTAC RTX 4070 Ti Trinity OC (SPECTRA 2.0)
- Lexar ARES V2 DDR5-6000 CL28

## Native hardware inventory (v0.6 + v0.7)

- Enumerate USB HID for **exact** ASUS Aura `VID_0B05 PID_19AF` and LIANLI `VID_1A86 PID_2107`.
- Deduplicate identical HID descriptions; read product/interface/usage metadata.
- Windows CIM query for GPU model/PNP ID and RAM module manufacturer/part number/speed.
- Export a combined JSON report to `%APPDATA%\LianLite\native-hardware-v06.json`.
- Existing Lian Li HID detection and process check from v0.4 remain.
- Profiles and RGB/speed sliders are **preview only**, not applied to hardware.

**Safety boundary:** No direct HID writes, SMBus/PCI/I2C access, firmware updates, fan changes or LCD writes. Hardware detection does not establish that a compatible RGB protocol is available.

### Why RGB writes are not enabled yet

ASUS Aura `0B05:19AF` has documented HID ancestry, but protocol features vary by board/firmware. A generic 65-byte command cannot safely be assumed correct. Confirm the specific motherboard's HID report and zone mapping first.

ZOTAC's SPECTRA 2.0 GPU control transport for the exact 4070 Ti Trinity OC is not validated; older Zotac I2C findings must not be applied blindly to this card.

Lexar ARES V2 RGB controller and a safe Windows SMBus transport are not validated. Do not probe DIMM SMBus addresses by writing; that may interfere with memory SPD/controller behavior.

The Lian Li SLV3H HID interface has not been proven to be the Wireless controller. Keep L-Connect services running for fan and LCD management.

## Build

Install Rust MSVC toolchain and Visual Studio C++ Build Tools on Windows 11.

```powershell
cargo build --release --bins
```

Outputs: `target\release\lianlite.exe` and `target\release\lianlite-updater.exe`.

GitHub Actions builds automatically for pushes to `main`. If the build succeeds, download the `LianLite-Windows` workflow artifact. This is **not** a GitHub Release; the built-in update checker only downloads versions published in GitHub Releases. Do not publish a version tag until the build and updater have been verified.

## Hardware diagnostic steps

1. Run v0.6 and click **Detect ASUS + Lian Li HID**.
2. Click **Detect GPU + RAM**.
3. Click **Export native report**.
4. Open `%APPDATA%\LianLite\native-hardware-v06.json` and review device IDs before sharing. The report may include PnP IDs/part numbers.

## Protocol references

- ASUS hardware: https://rog.asus.com/motherboards/rog-strix/rog-strix-b650e-f-gaming-wifi-model/spec/
- ASUS Aura HID: https://openrgb-wiki.readthedocs.io/en/latest/asus/ASUS-Aura-USB/
- ZOTAC card: https://www.zotac.com/product/graphics_card/zotac-gaming-geforce-rtx-4070-ti-trinity-oc

## Updater

The two-executable GitHub Releases updater remains from v0.4, including backup and checksum validation. It has not been fully tested for Windows recovery; keep a manually working copy until verified. No release tag is automatically published on pushes to `main`.

## v0.7: ASUS Aura diagnostics and no CMD window

- Both `src/main.rs` and `src/bin/lianlite-updater.rs` start with `#![windows_subsystem = "windows"]` to prevent a console window when double-clicking an executable.
- The updater now writes failure details to `%LOCALAPPDATA%\LianLite\updater-last-error.txt` since the console is hidden.
- New **Inspect ASUS Aura (read-only)** button retrieves Windows Plug and Play information for exact `VID_0B05&PID_19AF` using Windows built-in PowerShell `Get-PnpDevice` / `Get-PnpDeviceProperty`; this is not a HID report descriptor and does not prove the Aura command protocol.
- **Export ASUS report** saves `%APPDATA%\LianLite\asus-aura-pnp-v07.json`. It may contain Windows device InstanceIds or location information; review/redact before sharing.
- This version does **not** send RGB colors to the ASUS controller, GPU, RAM or Lian Li devices. It does not change fan speeds or LCD contents.

No OpenRGB required. The existing GitHub Releases updater still requires **both** `lianlite.exe` and `lianlite-updater.exe` alongside each other. For first-time setup, use the `LianLite-Portable-Windows-x64.zip` artifact containing both executables. A GitHub Actions artifact is not itself a published update; use the explicit Publish GitHub Release workflow option after the build succeeds.

### Windows validation

Before publishing v0.7: verify the GitHub Actions build and tests are green; on Windows 11 launch the GUI directly and confirm no CMD opens; check that "Inspect ASUS Aura (read-only)" returns data or a meaningful error; review any exported identifiers before sharing; confirm the updater and restore path using a disposable copy of the application.

## v0.7.1 updater recovery

If the app closes after **Install & restart** but does not relaunch, open `%LOCALAPPDATA%\LianLite\updater-last-status.txt` and (if present) `updater-last-error.txt`. The prior updater might not produce a log. Look for `lianlite.exe.backup` and `lianlite.exe.pending` next to your installed executable, but **do not delete either before diagnosing**. If `lianlite.exe` still exists, try opening it directly.

The v0.7.1 helper preserves an earlier backup as `lianlite.exe.backup.previous` rather than silently failing because `lianlite.exe.backup` already exists. It checks that the newly launched program is still running after three seconds; if the new executable exits immediately, it attempts to restore and relaunch the old one. It retains backups for manual recovery. This does not catch later crashes and does not establish end-to-end recovery reliability. The helper cannot automatically update itself through the existing EXE-only update package, so a user who already has an older helper may need to manually replace `lianlite-updater.exe` once with the latest helper shipped in the GitHub Release or portable bundle.

Do not test updates in a directory containing irreplaceable files. The app should live in a writable application folder and stay with its updater executable.
