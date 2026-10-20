# Cult Copy

<img src="packaging/icons/icon-256.png" alt="Cult Copy: overlapping white and green boxes" width="96" height="96">

An open-source Rust tray/menu bar app that shares text, images, files, and folders between paired computers on a local network. Each installation sends and receives. There is no central server, account, or cloud service. Licensed under the [MIT License](LICENSE).

This is an early release. Native build, clipboard, installer, and window checks
run on the supported platforms. See [validation details](docs/VALIDATION.md)
for tested behavior and remaining device checks.

## Downloads

Get the download for your computer from the
[latest public release](https://github.com/bryancolvinfilm-bot/cult-copy/releases/latest).
The matrix below applies to v0.1.5 and newer; earlier releases have fewer downloads.
Downloads use the Cult Copy name and `CultCopy-` filenames.

| System | Processor | Recommended download | Alternative |
| --- | --- | --- | --- |
| Windows | Intel / AMD, x64 | `windows-x64-setup.exe` | |
| Windows | ARM64 (Snapdragon, etc.) | `windows-arm64-setup.exe` | |
| macOS 12+ | Apple Silicon **and** Intel | `macos-universal.dmg` | `macos-universal.pkg` |
| Linux | Intel / AMD, x64 | `linux-x64.AppImage` | `linux-x64.run` |
| Linux | ARM64 / aarch64 | `linux-arm64.AppImage` | `linux-arm64.run` |
| Fedora 44 | Intel / AMD, x64 | `fedora-x64.rpm` | |
| Fedora 44 | ARM64 / aarch64 | `fedora-arm64.rpm` | |
| Rocky Linux 8 | Intel / AMD, x64 | `rocky8-x64.rpm` | |
| Rocky Linux 8 | ARM64 / aarch64 | `rocky8-arm64.rpm` | |

Each filename starts with `CultCopy-<version>-`. **x64** means Intel/AMD
64-bit; **ARM64** means 64-bit Arm. The Mac universal app contains both.
Every release built from this matrix includes all twelve packages plus `CHECKSUMS.sha256` and
`BUILD-INFO.json` with the source commit and package hashes.

On Mac, quit the running app, open the DMG, and drag **Cult Copy** onto
**Applications**. The PKG remains available for people who prefer an installer.
If upgrading from the previous name on Mac, quit the old app and use the **PKG**:
it moves the existing bundle to **Cult Copy.app** and replaces it in place. A
DMG drag between different bundle names does not remove the old copy.
On Linux, make the AppImage executable and open it. Without FUSE, run
`./CultCopy-<version>-linux-<arch>.AppImage --appimage-extract-and-run`.
Keep an AppImage in a permanent location before enabling **Start at login**.
For applications-menu integration, run the `.run` alternative with `sh`.
Linux downloads use the Ubuntu 24.04/glibc 2.39 baseline and system desktop and
graphics libraries; AppImage packaging does not remove those requirements.

Fedora 44 and Rocky Linux 8 also have native RPMs, built separately inside each
distro for x64 and ARM64. Install the matching download with
`sudo dnf install ./CultCopy-<version>-<fedora|rocky8>-<arch>.rpm`.
The Rocky 8 build uses its glibc 2.28 baseline; the Fedora RPM targets Fedora 44.
DNF installs the desktop library dependencies, launcher, and icons system-wide.
When switching from a per-user `.run` installation, quit Cult Copy and remove the old
`~/.local/share/applications/cult-copy.desktop` (or the equivalent under
`$XDG_DATA_HOME`) so it does not override the RPM's launcher. Clipboard settings
and history use the same per-user profile.

Installers are development builds without public release signing/notarization.
For the agent workflow that pushes, checks, builds all platforms, and publishes
verified downloads, see [publish.md](publish.md).

### Upgrading existing installations

Cult Copy continues the same application under a new name. Install over the
existing version; keep the same Linux package format when upgrading. Device
keys, pairings, settings, and clipboard history are preserved through supported
upgrades. The icon is unchanged. The network protocol remains compatible with
earlier releases, subject to the existing folder-transfer requirements below.

Linux packages, launchers, icons, startup entries, and the executable use
`cult-copy`. The RPM replaces the previous package during an upgrade. Existing
Linux profiles migrate to the Cult Copy data directory after the previous app
has quit, preserving pairings, settings, and clipboard history.

## Use

1. Install the app on each computer and open **Devices**.
2. Choose a nearby computer, compare the code on both screens, and approve pairing on both computers.
3. Copy normally. Supported clipboard changes go to connected, paired computers.
4. Click the tray/menu bar icon to inspect recent text, image thumbnails, file names, and transfers.
5. Enable **Start at login** in Settings to keep it available after signing in.

The white-and-green box logo stays in the tray/menu bar, with a small status dot for incoming copies. Its tooltip shows text/image/file and status. A small **New clipboard** desktop notification shows only the type icon and readiness/approval status, never a content snippet or filename. Copies requiring consent notify immediately; automatic copies notify when ready. Pending approvals remain marked on the tray icon and Clipboard tab. Desktop notification visibility also follows the OS notification/Focus settings.

Expired discovery announcements disappear from Devices. A device is identified
by its key, so two real computers with the same name stay separate and must pair
independently. Connection errors clear on reconnection, expire after about
30 seconds, or can be dismissed immediately.

Each computer has its own receiving settings:

| Setting | Behavior |
| --- | --- |
| Automatic download limit | Logarithmic slider from **500 KB to 1 GB**, default **10 MB**. The total size of a multi-file selection determines whether approval is needed. |
| Ask before receiving anything | Off by default. When enabled, every incoming text, image, or file waits for approval. Outgoing copies still share normally. |
| Pause sharing | Stops sending new offers and receiving new copies; cancels active transfers. |

Above the automatic limit, only an offer is sent. Each recipient independently accepts or declines. Accepted transfers show circular percentage progress, transferred/total bytes, speed, estimated remaining time, cancellation, and retry. Files become pasteable after all bytes and hashes have been verified. A later local copy is preserved if it happens during a download; use **Copy to clipboard** to select the completed item deliberately.

### Mac-to-Mac clipboard setup

Apple's Universal Clipboard can compete with Cult Copy on Macs signed in to the same
Apple Account. In Cult Copy's **Settings → Mac clipboard setup**, click **Open Handoff
settings**, then turn off **Allow Handoff between this Mac and your iCloud devices**.
Repeat on every Mac using Cult Copy. Turning Handoff off also disables handing off
activities such as Safari pages between Apple devices.

Cult Copy checks the stored Handoff preferences without changing them. The status is
best effort: missing, mixed, or unreadable values show **Couldn't confirm** rather
than guessing. It checks again when you return to Cult Copy, while its window is focused,
or when you click **Check again**. Always verify the actual switch in System Settings.
If the link cannot reach the pane, go to **System Settings → General → AirDrop &
Handoff** (or **AirDrop & Continuity**). On Monterey, use **System Preferences → General**.

Cult Copy skips clipboard announcements marked as arriving through Apple's Handoff
before reading their contents. Items delivered by Cult Copy are written with Apple's
`currentHostOnly` option so Universal Clipboard does not share them again. These
safeguards do not disable Apple's sharing of new copies made in other apps while
Handoff remains on; macOS provides no supported app-level exclusive clipboard mode.

## Compatibility

| Platform | Backend and requirements |
| --- | --- |
| Windows 10/11, x64 | Native clipboard formats and change notifications. Per-user installer; normal operation does not need administrator privileges. Allow the app on your private network if Windows Firewall asks. |
| macOS 12+, Apple Silicon and Intel | AppKit pasteboard; menu bar app. Installer packaging creates a universal binary. Approve clipboard/local-network access when macOS requests it. |
| Linux X11 | Native clipboard selection and file-manager formats. Only the normal copy/paste clipboard is shared; the primary/middle-click selection is left alone. |
| Linux Wayland | Requires a compositor implementing the data-control protocols used by `wl-clipboard-rs`. If unavailable, the adapter may fall back to XWayland, which does **not** guarantee full access to native Wayland apps. The app reports this limitation in Settings. GNOME Wayland may need an additional desktop integration or an X11 session. |

All computers need a reachable IP network; a display/USB connection by itself is insufficient. Discovery uses mDNS on the local network. Guest Wi-Fi, multicast filtering, VPNs, and firewalls may block it. **Connect by address** accepts the other computer's LAN address and the port displayed in Devices. Current discovery/listening uses IPv4.

The implementation supports plain text, HTML/RTF alternatives, copied raster images, and files or folders with their nested contents, including hidden files and empty directories. Each selection can contain up to 256 files and directories in total, including the selected roots, with at most 32 path components. Very long path metadata may reach the control-frame limit sooner. Names are made portable across operating systems, with collisions renamed. Symbolic links, Windows junctions, and special files are rejected; permissions and other filesystem metadata are not transferred. Application-private clipboard objects are not implemented. Text/image payloads are capped at 64 MiB as a resource bound; the automatic-download slider does not remove that bound. Large regular files stream in bounded chunks rather than being loaded into memory. Folder transfers require updated apps on both computers; regular-file transfers retain their existing wire format.

### History and storage limits

History targets 30 recent entries; the current clipboard and active downloads are protected from eviction. There is currently no age expiry or total cache-size budget. Received files and images are cached on disk, so large retained items can use substantial storage. The per-copy automatic-download limit is a consent setting, not a total storage limit. Clear history in Settings removes eligible entries while preserving the current clipboard and pending/active downloads.

Known cleanup issue: cache deletion skipped while an outgoing transfer is active is not retried later, and interrupted operations can leave stale cache directories. A byte budget, age-based expiry, and more complete cache cleanup are planned improvements.

Other clipboard synchronization tools can independently overwrite the same system clipboard. Use this app's approval/pause settings or disable overlapping sync tools if they compete. The networking does not intercept keys, change proxy/DNS settings, or require a fixed application port.

## Build and installers

Use the current stable Rust toolchain. Windows needs the normal MSVC build tools, or a compatible LLVM/MinGW toolchain. Linux development packages include a C compiler, `pkg-config`, X11, EGL/OpenGL, Wayland, and xkbcommon headers.

```sh
cargo build --release --locked
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
```

The Windows PowerShell helper uses Cargo from PATH, or a project-local toolchain if one has been configured:

```powershell
.\scripts\dev.ps1 build --release --locked
.\scripts\dev.ps1 test --all-targets --locked
.\scripts\dev.ps1 -CargoArgs @('clippy', '--all-targets', '--locked', '--', '-D', 'warnings')
.\scripts\package-windows.ps1
```

Windows packaging uses Inno Setup and produces one `setup.exe`.

The LLVM/MinGW packaging path bundles its required `libunwind.dll` and license in that installer; the native MSVC build uses a static C runtime. Inno Setup's own license applies to use of its compiler. The compiler used for the original local installers was licensed for noncommercial use.

Build macOS or Linux installers on their native OS:

For a Mac build with prerequisite setup, use [the Mac build helper](docs/MAC-BUILD.md).

```sh
sh scripts/package-unix.sh
```

This produces a macOS universal `.dmg` and `.pkg`, or a native-architecture Linux
`.AppImage` and per-user `.run` installer. Artifacts go into `dist/`. Linux
packaging downloads hash-pinned versions of the official AppImage tool and
runtime. Certificates and Apple notarization are not configured.

To reproduce a Fedora 44 or Rocky Linux 8 RPM on any Linux host with Podman
(or Docker), use the isolated distro builder. It copies only build sources,
installs dependencies in a disposable container, compiles with stable Rust,
runs the tests and an Xvfb clipboard round trip, renders a diagnostic app window,
and checks RPM installation and removal. It builds for the host CPU architecture
and writes the RPM to `dist/`:

```sh
bash scripts/build-linux-distro.sh fedora
bash scripts/build-linux-distro.sh rocky8
# To select Docker explicitly:
bash scripts/build-linux-distro.sh rocky8 docker
```

For an already provisioned native Fedora 44 or Rocky Linux 8 development
environment, run `sh scripts/package-rpm.sh fedora` or
`sh scripts/package-rpm.sh rocky8`. The dependency setup and install/uninstall
checks are intended for disposable build containers, not a working desktop.

The original Linux x64 cross-build targets glibc 2.28 or newer. A Windows cross-build can be reproduced with the Linux Rust target installed and `scripts/cross-linux.ps1 -Zig PATH_TO_ZIG_EXE`. Refresh bundled dependency notices after changing the lockfile with `scripts/generate-notices.ps1`.

See [validation results](docs/VALIDATION.md) for what was actually tested and which native desktop checks remain.

The native CI workflow builds/tests on Windows, macOS, Ubuntu, Fedora 44, and Rocky Linux 8, and creates installers in their respective environments. Fedora/Rocky RPMs use native x64 or ARM64 runners with matching distro containers. AppImage and `.run` builds use Ubuntu 24.04. A requested publish runs all platforms and makes verified installers available on GitHub Releases; see [publish.md](publish.md). Native clipboard tests are explicitly ignored in ordinary local test runs because they replace the real clipboard. Run them only in a disposable desktop session:

```sh
cargo test --test native_clipboard -- --ignored --test-threads=1
```

CLI options: `--background`, `--data-dir PATH`, `--version`, `--help`. `--data-dir` is useful for isolated test profiles. Only one instance runs per profile.

Opening the app again activates the existing instance through an authenticated loopback-only endpoint. This also provides a way to reopen the window on Linux desktops without a tray host.

Closing the window keeps Cult Copy running in the tray, including on native Linux Wayland. Clipboard syncing, notifications, and tray status updates continue while the window is closed. Use **Open clipboard** in the tray menu or launch the app again to reopen it. Choose **Quit** in the tray menu to exit. A disabled **Version** label at the bottom of that menu shows the running build's version. Linux's per-user installer installs PNG/SVG launcher icons and a desktop entry matching the window's `cult-copy` app ID.

## Design

- mDNS service `_duetclip._tcp.local.` advertises discovery metadata. The OS assigns the application's TCP port.
- Noise XX (`25519 / ChaChaPoly / BLAKE2s`) encrypts sessions. Both screens compare a session-bound 48-bit verification code, and both devices must approve before clipboard exchange. Permanent device public keys are pinned locally.
- Offers have an event UUID, a Lamport version with a stable device tie-break, and per-part BLAKE3 hashes. Each download attempt has a separate transfer UUID so late chunks from a cancelled attempt cannot corrupt a retry.
- Incoming writes carry a private clipboard marker and are checked against the locally observed content signature to suppress echo. Re-copying A after B is a fresh event.
- Control messages have priority over the bounded file-data queue. Receivers validate names, counts, lengths, offsets, and hashes before publishing files to the clipboard. Files are copied; the original is not moved or deleted.
- Recent history, settings, trust, and cached downloads stay in the current user's application-data directory. There is no analytics or content logging. History targets 30 items, with live clipboard/active downloads protected from eviction; the storage limitations above apply. Clipboard history and file caches contain private data in local files; network encryption does not encrypt that local cache.
- Common clipboard sensitivity markers are respected when present. Unmarked secrets cannot be reliably distinguished from ordinary copied text.

Platform API references: [clipboard-rs](https://github.com/ChurchTao/clipboard-rs), [Wayland clipboard protocols](https://docs.rs/crate/wl-clipboard-rs/latest), [tray-icon](https://docs.rs/tray-icon/latest/tray_icon/), [mdns-sd](https://docs.rs/mdns-sd/latest/mdns_sd/), [Noise](https://noiseprotocol.org/noise.html).

## Artwork

The white-and-green overlapping boxes come from
[`packaging/cult-copy.svg`](packaging/cult-copy.svg). Windows EXE/setup,
Mac app bundles, Linux launchers/AppImages, and the tray use the same design.
Run `scripts/generate-artwork.ps1` on Windows to regenerate the committed PNG,
ICO, ICNS, and [repository preview image](docs/social-preview.png).
To set the image shown when sharing the repository link, upload that PNG in
**GitHub repository Settings → Social preview**. A README image alone does not
set GitHub's social preview. See [GitHub's instructions](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/customizing-your-repositorys-social-media-preview).
