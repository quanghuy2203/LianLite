# LianLite v0.4 — Ultra Minimal (Windows 11)

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
