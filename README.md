# LianLite v0.5 — Ultra Minimal (Windows 11)

Rust desktop app for the user's hardware layout:
- TL Wireless LCD: group of 3 fans and group of 2 fans
- TL Wireless non-LCD: group of 3 fans
- 2 Strimer Wireless cables

**Hardware controls are NOT implemented.** Fan-speed, RGB, and Strimer controls in the UI are local previews only. The HID scanner performs read-only enumeration of USB VID `1A86`, PID `2107`, based on a user-supplied inventory. It does not prove that this is the L-Wireless Controller. Keep L-Connect services active so the fans and LCD remain managed.

## Build

Install Rust (MSVC target) and Visual Studio C++ Build Tools. From the repository root:

```powershell
cargo build --release --bins
```

The two outputs are `target\release\lianlite.exe` and `target\release\lianlite-updater.exe`.

You can also download the GitHub Actions artifact after the workflow succeeds. Install both programs in a **writable** folder (e.g., `%LOCALAPPDATA%\LianLite`), not in Program Files, and launch `lianlite.exe` normally.

## GitHub Actions

The `Windows release` workflow compiles with a Windows runner, uploads an artifact and, for a tag matching `v*`, creates a GitHub Release with:
- `lianlite-windows-x64.zip` (the main executable only)
- `lianlite-windows-x64.zip.sha256`
- `lianlite-updater.exe` (needed on initial install)

Workflow status: https://github.com/quanghuy2203/LianLite/actions

Do not create a release tag until the build has passed and the code has been reviewed.

## Auto Update

The app checks `quanghuy2203/LianLite` through GitHub's `releases/latest` API. It compares the release tag to the `Cargo.toml` version; a higher version enables the Download button. It verifies a SHA-256 checksum, checks ZIP contents, stages the new executable, and launches the standalone updater on user confirmation. The updater waits for the GUI to close, moves the previous executable to a backup, replaces the executable, then starts it.

**Limitations:** The release checksum protects against download corruption, not a compromised publishing account. The release should eventually use cryptographic signatures. This updater does not replace itself; distributing a new updater helper requires a separate installation method. Starting a new process does not guarantee the new process remains healthy. If an existing `lianlite.exe.backup` exists, the helper refuses to update, to avoid overwriting a potentially necessary recovery copy. Remove it manually only after verifying the current version works. No automatic service stopping or firmware/HID writes are performed.

The app and updater have not been tested on a Windows machine by the assistant. The build must be verified in GitHub Actions before relying on the release.

## v0.5 optional RGB synchronization (motherboard / GPU / RAM)

Install [OpenRGB](https://openrgb.org/) separately and check its hardware support. Start OpenRGB and enable **SDK Server** at `127.0.0.1:6742`. Make `OpenRGB.exe` available in your PATH or place it next to `lianlite.exe`. In LianLite click **Detect OpenRGB LED devices**, select the specific devices you want to control, choose a color, then press **Apply static color to SELECTED devices**. Nothing is sent until you explicitly click the Apply button.

OpenRGB must recognize each device: motherboard, GPU and RAM detection is not guaranteed. Avoid concurrent LED control by Armoury Crate/Aura Sync or other applications. Device indexes may change after a scan; always review the list. The adapter relies on OpenRGB's command-line client, not direct USB/SMBus access, and sends one explicitly chosen device at a time. It does not synchronize animated effects or Lian Li Wireless hardware yet.

**New USB/PnP scan:** "Scan all Windows USB/PnP" exports `%APPDATA%\LianLite\usb-pnp-inventory.json`. This may contain hardware instance identifiers; review/redact any sensitive data before sharing. The earlier HID-only scanner remains available. Never disable the L-Connect services for testing.

The initial v0.5 app must be installed using the GitHub Actions artifact. A proper version-tagged Release only exists once a release tag is published. Updating from v0.4 to v0.5 from inside the app requires a v0.5 GitHub Release and both binaries co-located.
