# Duet Clipboard

A third-party, open-source Rust tray/menu bar app that shares text, images, and ordinary files between paired computers on a local network. Each installation sends and receives. There is no central server, account, or cloud service. Licensed under the [MIT License](LICENSE).

**Independent project: this app has no relation to Duet Display. It is not affiliated with, endorsed by, sponsored by, or maintained by Duet Display or its developers.** It provides separate LAN clipboard sharing that can be useful alongside display-sharing software. Duet Display is not required, and this app does not integrate with or modify it. See [NOTICE.md](NOTICE.md).

This is an early release. Mac-to-PC clipboard sharing has been confirmed working by the user who built and tested it, and the Windows close-to-tray fix has been verified on Windows. Linux desktop validation remains outstanding. See [validation details](docs/VALIDATION.md).

## Use

1. Install the app on each computer and open **Devices**.
2. Choose a nearby computer, compare the code on both screens, and approve pairing on both computers.
3. Copy normally. Supported clipboard changes go to connected, paired computers.
4. Click the tray/menu bar icon to inspect recent text, image thumbnails, file names, and transfers.
5. Enable **Start at login** in Settings to keep it available after signing in.

Each incoming copy updates the tray/menu bar to a text/image/file icon. A small **New clipboard** desktop notification shows only the type icon and readiness/approval status, never a content snippet or filename. Copies requiring consent notify immediately; automatic copies notify when ready. Pending approvals remain marked on the tray icon and Clipboard tab. Desktop notification visibility also follows the OS notification/Focus settings.

Each computer has its own receiving settings:

| Setting | Behavior |
| --- | --- |
| Automatic download limit | Logarithmic slider from **500 KB to 1 GB**, default **10 MB**. The total size of a multi-file selection determines whether approval is needed. |
| Ask before receiving anything | Off by default. When enabled, every incoming text, image, or file waits for approval. Outgoing copies still share normally. |
| Pause sharing | Stops sending new offers and receiving new copies; cancels active transfers. |

Above the automatic limit, only an offer is sent. Each recipient independently accepts or declines. Accepted transfers show circular percentage progress, transferred/total bytes, speed, estimated remaining time, cancellation, and retry. Files become pasteable after all bytes and hashes have been verified. A later local copy is preserved if it happens during a download; use **Copy to clipboard** to select the completed item deliberately.

## Compatibility

| Platform | Backend and requirements |
| --- | --- |
| Windows 10/11, x64 | Native clipboard formats and change notifications. Per-user installer; normal operation does not need administrator privileges. Allow the app on your private network if Windows Firewall asks. |
| macOS 12+, Apple Silicon and Intel | AppKit pasteboard; menu bar app. Installer packaging creates a universal binary. Approve clipboard/local-network access when macOS requests it. |
| Linux X11 | Native clipboard selection and file-manager formats. Only the normal copy/paste clipboard is shared; the primary/middle-click selection is left alone. |
| Linux Wayland | Requires a compositor implementing the data-control protocols used by `wl-clipboard-rs`. If unavailable, the adapter may fall back to XWayland, which does **not** guarantee full access to native Wayland apps. The app reports this limitation in Settings. GNOME Wayland may need an additional desktop integration or an X11 session. |

All computers need a reachable IP network; a display/USB connection by itself is insufficient. Discovery uses mDNS on the local network. Guest Wi-Fi, multicast filtering, VPNs, and firewalls may block it. **Connect by address** accepts the other computer's LAN address and the port displayed in Devices. Current discovery/listening uses IPv4.

The implementation supports plain text, HTML/RTF alternatives, copied raster images, and selections of up to 256 regular files. Folder copying and application-private clipboard objects are not implemented. Text/image payloads are capped at 64 MiB as a resource bound; the automatic-download slider does not remove that bound. Large regular files stream in bounded chunks rather than being loaded into memory.

### History and storage limits

History targets 30 recent entries; the current clipboard and active downloads are protected from eviction. There is currently no age expiry or total cache-size budget. Received files and images are cached on disk, so large retained items can use substantial storage. The per-copy automatic-download limit is a consent setting, not a total storage limit. Clear history in Settings removes eligible entries while preserving the current clipboard and pending/active downloads.

Known cleanup issue: cache deletion skipped while an outgoing transfer is active is not retried later, and interrupted operations can leave stale cache directories. A byte budget, age-based expiry, and more complete cache cleanup are planned improvements.

Other clipboard synchronization tools can independently overwrite the same system clipboard. Use this app's approval/pause settings or disable overlapping sync tools if they compete. The networking does not hook Duet, intercept keys, change proxy/DNS settings, or require a fixed application port.

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

This produces one macOS universal `.pkg`, or one per-user Linux `.run` installer. Run the Linux installer with `sh ./DuetClipboard-0.1.1-linux-x86_64.run` and open the app from your applications menu. Artifacts go into `dist/`. Certificates and Apple notarization are not configured; public releases need the appropriate signing setup.

The original Linux x64 cross-build targets glibc 2.28 or newer. A Windows cross-build can be reproduced with the Linux Rust target installed and `scripts/cross-linux.ps1 -Zig PATH_TO_ZIG_EXE`. Refresh bundled dependency notices after changing the lockfile with `scripts/generate-notices.ps1`.

See [validation results](docs/VALIDATION.md) for what was actually tested and which native desktop checks remain.

The native CI workflow builds/tests on Windows, macOS, and Linux, and creates installers on their respective operating systems. Native clipboard tests are explicitly ignored in ordinary local test runs because they replace the real clipboard. Run them only in a disposable desktop session:

```sh
cargo test --test native_clipboard -- --ignored --test-threads=1
```

CLI options: `--background`, `--data-dir PATH`, `--version`, `--help`. `--data-dir` is useful for isolated test profiles. Only one instance runs per profile.

Opening the app again activates the existing instance through an authenticated loopback-only endpoint. This also provides a way to reopen the window on Linux desktops without a tray host.

## Design

- mDNS service `_duetclip._tcp.local.` advertises discovery metadata. The OS assigns the application's TCP port.
- Noise XX (`25519 / ChaChaPoly / BLAKE2s`) encrypts sessions. Both screens compare a session-bound 48-bit verification code, and both devices must approve before clipboard exchange. Permanent device public keys are pinned locally.
- Offers have an event UUID, a Lamport version with a stable device tie-break, and per-part BLAKE3 hashes. Each download attempt has a separate transfer UUID so late chunks from a cancelled attempt cannot corrupt a retry.
- Incoming writes carry a private clipboard marker and are checked against the locally observed content signature to suppress echo. Re-copying A after B is a fresh event.
- Control messages have priority over the bounded file-data queue. Receivers validate names, counts, lengths, offsets, and hashes before publishing files to the clipboard. Files are copied; the original is not moved or deleted.
- Recent history, settings, trust, and cached downloads stay in the current user's application-data directory. There is no analytics or content logging. History targets 30 items, with live clipboard/active downloads protected from eviction; the storage limitations above apply. Clipboard history and file caches contain private data in local files; network encryption does not encrypt that local cache.
- Common clipboard sensitivity markers are respected when present. Unmarked secrets cannot be reliably distinguished from ordinary copied text.

Platform API references: [clipboard-rs](https://github.com/ChurchTao/clipboard-rs), [Wayland clipboard protocols](https://docs.rs/crate/wl-clipboard-rs/latest), [tray-icon](https://docs.rs/tray-icon/latest/tray_icon/), [mdns-sd](https://docs.rs/mdns-sd/latest/mdns_sd/), [Noise](https://noiseprotocol.org/noise.html).
