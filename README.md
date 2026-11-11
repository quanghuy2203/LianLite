# LianLite v0.16.0 — Safe offline L-Connect IPC reference correlation

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

## v0.10: Stop fabricating the number of wireless fans

The previous dashboard permanently displayed **5 LCD fans / 3 TL fans / 2 Strimers**, derived from an example configuration, not from the PC. This was misleading. v0.10 removes those fixed hardware counts and no longer creates fan groups from example data in a fresh profile.

On startup, LianLite now runs a **read-only** background discovery pass:

1. Query Windows PnP for known L-Wireless TX/RX adapter VID:PID pairs (V1 0416:8040/8041, V2 1A86:E304/E305). Deduplicate multiple Windows interface nodes to one *adapter type*. The SLV3H HID 1A86:2107 is NOT identified as a wireless TX/RX dongle.
2. Inspect `%PROGRAMDATA%\Lian-Li\L-Connect 3\device` **read-only**, with bounded recursion/file size/decompressed output, for gzip/JSON records structured as `{DeviceID, Type, Data}`. Collect explicit `FanQuantity` settings only; never infer number of fans from USB enumeration. These settings can be wired, stale, or otherwise unrelated to the wireless binding list, and are **not live wireless groups**. Raw DeviceID values are intentionally omitted from the exported report.
3. The dashboard labels these separately as **wireless adapter types**, **saved fan group settings**, **saved quantity**, and **live RF groups: unknown**. Buttons refresh the inventory without restarting the app.
4. The `Diagnostics` tab can export `%APPDATA%\LianLite\wireless-discovery-v010.json`, a sanitized device inventory with no raw controller IDs. The existing manually saved preview zones remain usable but are clearly labeled as previews, not detected hardware.

### Why this cannot yet show the precise 2 / 3 / 3 live groups

A screenshot of L-Connect shows five bound *wireless channels*: two Strimer and three fan groups. The number of individual fans (2/3/3) is separate from the number of channels. L-Connect's wireless receiver protocol exposes this data through radio discovery, but **USB PnP alone cannot tell us the number of fans per bound channel**.

LianLite currently does **not** send the RF `GetDev` discovery request because L-Connect services may be actively using the controller and the implementation is not verified on this exact Windows device. We will need an explicitly validated **read-only radio discovery path** or an officially supported read-only L-Connect service interface before presenting an exact live group map. Running two device owners concurrently could affect cooling; do not stop L-Connect to test this experimental inventory.

Documentation:
- Lian Li Wireless manual: https://lian-li.com/l-connect3/l-connect3-manual-l-wireless-sync/
- Independent technical notes: https://github.com/qdellp/wirelesscanvas-rgb/blob/main/TECHNICAL.md

The control surface remains preview-only. No cooling control, LCD writes, RF requests, HID feature/output transfers or L-Connect service changes have been implemented.

## v0.11: offline inspection of L-Connect wireless settings

The L-Wireless V1 TX/RX pair (0416:8040 / 0416:8041) is confirmed by Windows PnP on the user's system. LianLite v0.10 examined 16 saved configuration records but identified no `FanQuantity` groups. Instead of falsely showing 2/3/3 as live hardware, v0.11 adds a **read-only investigation of the saved settings types**.

Go to **Diagnostics → Analyze saved setting types**; a background worker scans the existing `%PROGRAMDATA%\\Lian-Li\\L-Connect 3\\device` tree, counts decoded JSON/gzip entries and summarizes the `Type` field and the `Data` value shape (object/array/string/number etc.). The **Export schema summary (no IDs)** button creates `%APPDATA%\\LianLite\\wireless-settings-schema-v011.json`. This report contains only the setting-type strings and aggregate counts, **not raw values, controller MACs, DeviceID, serialized state, or file paths**. Unknown or malformed Type strings are redacted. The walker limits directory depth, file count, compressed input size and decompressed output size.

This schema map helps identify which saved setting categories might describe bound wireless channels on this installed L-Connect version. A category that contains “wireless” or “bind” is marked as a **candidate only**, never a verified live channel. No direct USB GetDev/RF query is attempted while L-Connect controls WinUSB. In existing third-party implementations that perform live queries, the L-Connect service must relinquish the device first, so this release does not attempt that transition automatically.

The existing v0.10 Windows PnP device discovery, manual preview-only profiles, privacy-safe hardware diagnostics and app/updater auto-update remain. Real fan RGB, PWM, LCD and L-Wireless channel control remain disabled.

References: https://github.com/qdellp/wirelesscanvas-rgb/blob/main/TECHNICAL.md and https://github.com/lewisgibson/FanControl.LianLi/blob/main/docs/lighting.md

## v0.11.1: Keep the installation directory clean

Previously, every update kept a new `lianlite.exe.backup.previous.N` and `lianlite-updater.exe.backup.N` forever. This is why a portable folder grows by ~8 MB with every release.

Now after the new GUI has remained running through the updater's three-second immediate-exit check **and** the installed updater has been successfully replaced, LianLite automatically prunes only superseded backups. It preserves:

- The previous GUI as `lianlite.exe.backup` for manual rollback.
- The most recent previous updater as `lianlite-updater.exe.backup.N`.

It removes older app `.backup.previous` / `.backup.previous.N` and older updater `.backup.N` archives, using a strict filename allowlist, never deleting the installed `.exe`, the current GUI backup, staged updates, logs or unknown filenames. Cleanup failures are recorded in updater status; they do not mark a successfully installed application as failed. During a failed update, existing backups are never pruned.

On the **Updates** page, **Clean old update backups...** displays an explicit confirmation before cleaning older backup archives in the running app's install folder. This also handles older archives accumulated before v0.11.1. The operation cannot recover disk space in any other folder and does not erase the two most recent rollback copies.

**Remaining limitation:** A process still running after three seconds can still fail later. Backups provide a manual recovery path but are not a guarantee of successful long-term operation. Do not remove the two retained rollback files before verifying the current release.

### Wireless notes from v0.11 diagnostic reports

The user's sanitized settings report contained 16 decoded records. They were classified as `Case` (1), `Fan` (2), `GroupLCD` (1), `Lighting` (1), `MergeLightingEffectSetting` (1), `OpenRgbLighting` (6), `Pump` (2), `SingleLCD` (1), and `Strimer` (1). All had JSON object values. These are saved categories, not live wireless channels or fan-group counts. No binding group topology has been verified. The existing safe offline schema export is retained.

## v0.12 — Combined research + detection, without extra USB traffic

### What was added
- On startup, the **offline discovery survey** now runs both Windows USB PnP inspection and a bounded read of L-Connect's saved settings in one worker. **Refresh offline discovery** refreshes both sources.
- For physical TL Wireless LCD modules, Windows PnP checks the root USB device instance for exact **VID 1CBE PID 0006** and counts unique root instances; USB subinterfaces are ignored. An LCD receiver count may be unavailable or incomplete depending on Windows/USB connectivity. **This count is not wireless group topology** (it cannot distinguish which LCD fans are paired as groups of 2 and 3).
- The dashboard now reports evidence-based **wireless adapter types**, **LCD USB receivers**, **saved configuration categories**, and **live RF groups unknown** instead of hard-coded numbers.
- A new deep offline schema inspector describes nested JSON object field **names**, value types, and stored-array lengths for known L-Connect setting categories. It scans at most 512 files, 5 levels of subfolders, 512 KiB per file and 1 MiB decoded JSON per file; object/array traversal is also capped. Unknown setting types and unknown field names are **redacted**. It exports no DeviceID, paths or raw values. Saved array lengths are *not* live fan counts.
- **Devices → Scan PC hardware in one click** checks ASUS/Lian Li HID, GPU/RAM and ASUS PnP in a single background task. Partial results survive when one step fails.
- **Diagnostics → Export combined support snapshot (recommended)** creates `%APPDATA%\LianLite\lianlite-support-snapshot-v012.json` combining redacted hardware and wireless data, omitting serial numbers, MACs, raw PnP paths and raw L-Connect values. You can also export `wireless-settings-schema-v012.json` with nested structural summaries and `wireless-discovery-v012.json`.
- **Overview → Clear manual preview zones...** explicitly lets you remove the stale manually-saved zone placeholders after confirmation; it never touches actual fans or L-Connect.
- Set the native egui dark palette once at app startup rather than reapplying it every frame.
- GitHub Actions runs `cargo test --release --bins` before `cargo build --release --bins` so tests and packaging share compiled release-profile dependencies. This may reduce duplicate compiler work; actual CI timing still needs measurement.
- Retains v0.11.1's updater archive cleanup, backup recovery, and GUI subsystem (no console window).

### Known constraints
L-Connect's official UI knows its live Wireless RF groups. USB PnP and saved configuration records **do not provide that mapping**. LianLite does not try the community-documented WinUSB GetDev exchange while L-Connect controls the USB receiver; it must not race the active cooling service. Thus v0.12 can identify the presence of V1/V2 USB TX/RX and count exposed LCD USB receiver roots but cannot yet prove the 3 fan RF groups or 2 Strimer groups. No hardware RGB/PWM/LCD commands are enabled.

### Suggested validation
1. Confirm GitHub Windows Actions tests, build and release have succeeded.
2. Update to v0.12 through the app; check **Updates → Check updater and backups**.
3. Open **Overview**, confirm that USB LCD receivers are measured (if available) rather than hard-coded.
4. Choose **Devices → Scan PC hardware in one click**, then **Diagnostics → Export combined support snapshot (recommended)**.
5. If studying saved grouping, use **Diagnostics → Export v0.12 nested schema (no IDs)** after the automatic scan.

### Reference research
- [LIAN LI TL Wireless LCD troubleshooting](https://lian-li.com/faq/sl-tl-wireless-lcd/) says Device Manager can confirm the LCD USB receiver devices, including 2+3 configurations.
- [Hypercolor hardware notes](https://hyperb1iss.github.io/hypercolor/hardware/compatibility/) document TL Wireless LCD USB receivers at VID 1CBE PID 0006, typically one USB receiver per LCD fan.
- [Community L-Wireless transport study](https://github.com/qdellp/wirelesscanvas-rgb/blob/main/TECHNICAL.md) describes GetDev but uses active WinUSB I/O; that is **not implemented** here.

## v0.13.0: Native receiver-side RF topology reader (experimental)

This release implements an actual **V1 0416:8041 RX GetDev** reader, not another offline settings report. The new Rust module `wireless_protocol.rs` parses the receiver's `0x10` response with 42-byte device records. It validates page length and 0x1C record markers, excludes master records and implausible fan quantities, and classifies Lian Li fan families (TL, SL and SL Infinity) as well as Strimer types from the documented device/fan-type bytes. For fan devices it reports a 0–4 fan count, per-fan RPM, and RF channel. It never exports the device/master MAC addresses. Unit tests cover mixed 2/3-fan group records, Strimer, malformed payloads, duplicate devices and unverified binding.

The V1 Windows backend `wireless_winusb.rs` uses Windows SetupAPI / native WinUSB and sends exactly **one GetDev query to endpoint 0x01**, reading the RX response on 0x81 with a bounded timeout. This is a single USB *discovery request*, not a fan speed, lighting, LCD, or firmware command. No driver installation, firmware reset, USB reset, L-Connect service changes or automatic polling. It only tries the **exact 0416:8041 V1 receiver**, requires a single matching device and exclusive open, and checks the active Windows process list before touching hardware and again before sending the packet.

### How to test

Open **Devices → Direct Wireless V1 receiver discovery (experimental) → Probe actual RF groups (guarded)**. A response, if available, appears on Devices and Overview with channel, family, group count and RPM. If L-Connect's service or watchdog is running, the probe deliberately refuses and displays a warning. **Do not stop your L-Connect service solely to test this while it is responsible for cooling.** The default hardware profile remains controlled by L-Connect. No operation is performed in the background without a click.

**Limitations to be resolved:** This is a first Windows WinUSB implementation and has not been hardware-validated. The generic `GUID_DEVINTERFACE_USB_DEVICE` SetupAPI path may not expose the vendor-specific WinUSB interface; in that case the probe fails safely. The local TX master MAC is not yet queried, so a receiver record is not proof that the corresponding group is *bound to this master*. V1 response parsing is independently documented, but RX firmware variations may require further work. The application makes no claim of safe PWM control, RGB output or LCD streaming. Under ordinary L-Connect operation the explicit service guard prevents a live query. Do not report unverified topology as actual bound devices.

Protocol reference (independent reverse engineering):
https://github.com/sgtaziz/lian-li-linux/blob/main/crates/lianli-devices/src/wireless/discovery.rs
https://github.com/qdellp/wirelesscanvas-rgb/blob/main/TECHNICAL.md

This release keeps the improved v0.12 USB LCD receiver inventory, fully offline L-Connect saved configuration research, combined support snapshot and v0.11.1 backup retention.

## v0.14.0 — Test without taking over cooling hardware

**New on Devices:** `Test parser + group UI (simulation only)` loads a fully synthetic receiver page with fan counts **2, 3, 3** and **two Strimer records**. It exercises the same 42-byte GetDev decoder used for real data, without USB I/O, stopping L-Connect, changing RGB or altering PWM. The simulation is **always explicitly labeled** and is never stored in `live_topology`, so the dashboard's real `RX REPORTED GROUPS` stays `?` when live discovery is unavailable. `Clear simulated groups` removes the demo.

**Safety and reliability fixes:** The native WinUSB backend now opens the device **synchronously** to match its synchronous `WinUsb_ReadPipe`/`WinUsb_WritePipe` calls; previously it used `FILE_FLAG_OVERLAPPED` with null OVERLAPPED pointers, which was incompatible. Process ownership checks recognize additional L-Connect GUI names and remain fail-closed for the known service/watchdog; exclusive USB open is still required. The offline Windows PnP survey hides its PowerShell console. Offline diagnostics no longer incorrectly say that GetDev does not exist.

**Decoder correctness:** A zeroed MAC/identity field is no longer used to deduplicate distinct records. The test suite now checks synthetic topology, zero-ID handling, invalid headers and oversized claims, plus the existing group, RPM, Strimer and truncated-response cases.

**Not yet solved:** No real RF discovery is possible while L-Connect's service owns the V1 RX. This release **does not** expose fan/RGB/LCD writes and **does not** disable or stop L-Connect. The simulated 2+3+3+Strimer groups are test fixtures, **not evidence of actual device grouping**.

### Validation after GitHub Windows CI succeeds
1. Update to v0.14.0 using **Updates**.
2. Open **Devices**, click **Test parser + group UI (simulation only)**. Expect 5 *synthetic* groups, fan counts 2,3,3,0,0, with a prominent simulation warning.
3. Check **Overview → RX REPORTED GROUPS** remains `?` (not fake 5).
4. Click **Clear simulated groups** and verify test groups disappear.
5. Leave L-Connect services running. A direct GetDev probe should still be refused by the safety guard.

This validation demonstrates the decoder and UI pipeline only; it is not hardware verification.

## v0.15.0 — Look for service-side discovery without disturbing cooling

The machine has demonstrated that the L-Connect service/watchdog is running and the direct WinUSB GetDev probe correctly refuses to take the controller while it is in use. Instead of asking users to disable their cooling service, the new **Devices → L-Connect service interfaces → Find service communication interfaces** action passively inspects Windows process and endpoint metadata for potential service-owned IPC.

The action uses a background PowerShell query to:
- Look for exact L-Connect service/watchdog/GUI executable names in the Windows process table.
- Correlate local TCP listening sockets (127.0.0.1 or ::1 only) to those process IDs, and show listener port numbers.
- Count named pipes with names resembling Lian Li/L-Connect. Pipes are not opened or connected to; name match alone is not proof of ownership.
- Redact process IDs, command lines, executable installation paths, and individual pipe names from the UI.

This is read-only **metadata discovery, not a confirmed API**. If a socket or pipe exists, we still need to identify the message schema, verify it is safe, and determine whether device topology can be read without disturbing L-Connect. If none is found, this does not prove no IPC exists: it might use memory sharing, a private or differently named pipe, or a different API.

The UI explicitly shows **Live group API verified: no**; it will never populate actual groups from guessed endpoints or the synthetic test fixture. No request to L-Connect, service shutdown, task disablement, process injection, USB reset, fan PWM, lighting or LCD writes are performed.

### Validation
1. Confirm Windows PR validation is green before merging into main and publishing.
2. Update to v0.15.0 and leave L-Connect fully running.
3. Open **Devices → Find service communication interfaces**. You should see a process count and the number of service-owned loopback listener ports and pipe-name candidates; zeros may be legitimate.
4. If an error occurs, copy just the bottom STATUS text. Never disable cooling services merely to get a nonzero result.

### Still missing
An authenticated/validated read-only L-Connect service protocol. v0.15 does not yet enumerate the three actual fan RF groups or two bound Strimer channels, nor does it provide live RGB, fan-speed or LCD control. This remains an explicit prerequisite for safe replacement of the official app.

## v0.16 — inspect actual L-Connect files without opening named pipes

The user's v0.15 scan revealed three running L-Connect processes (service, watchdog and GUI), three named-pipe *name candidates* and no L-Connect-owned loopback TCP listeners. The number of candidate names alone cannot establish which process owns them or whether they contain a read-only group-list API.

This version adds **Devices → Check pipe references in L-Connect files**. The background read-only operation enumerates the same candidate names and exact L-Connect process executable paths locally, then searches the on-disk `.exe` bytes for each candidate's ASCII or UTF-16LE name. Exports **only counts and process-family labels**, never executable paths, individual pipe names, MACs or device serials. Limits: 12 processes, 64 candidate names, 32 MiB per executable, 64 KiB PowerShell JSON output. It does not load vendor DLLs, connect to pipes/sockets, open the wireless USB controller or send any message to L-Connect.

A literal match is **weak static evidence** that a process binary refers to a possible pipe name, *not* proof of ownership or a supported API. Zero matches are inconclusive because code may be in a DLL, construct names at runtime or use unrelated endpoint names. This is not yet a functioning interface to live bound RF groups.

Windows PR validation runs `cargo test --release --bins` and `cargo build --release --bins` on the feature branch before publishing. No fan, RGB, LCD or pump writes are enabled; L-Connect keeps exclusive hardware ownership.

### Test
Leave L-Connect and its services running, then choose **Devices → Check pipe references in L-Connect files**. Send just the counts shown and bottom STATUS, not any raw pipe names or executable files. A zero count is valid, and the current app will continue to report real live groups as unknown.
