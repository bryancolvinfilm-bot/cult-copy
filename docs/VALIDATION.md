# Validation

## Native CI and publication in 0.1.2

The first hosted 0.1.1 workflow completed the Windows and Linux checks/builds.
Its Mac clipboard round trip caught an invalid custom pasteboard type: a MIME
string was used where macOS requires a UTI. Version 0.1.2 uses a reverse-DNS UTI
on macOS and retains the existing clipboard marker on Windows/Linux. The marker
is local clipboard metadata; network compatibility is unchanged.

The release workflow now gates publication on all three native jobs, validates
the version/source SHA, and verifies uploaded installer hashes before publishing.
See the linked workflow in each release's `BUILD-INFO.json` for that release's
actual native results; these builds do not replace full desktop testing on every
supported OS/compositor. The local release verifier tests also reject a wrong
source commit, modified installer bytes, and missing platform assets.

## Windows tray fix in 0.1.1

The 0.1.0 Windows close-to-tray failure was reproduced on the desktop: after
closing the window, another launch exited successfully but did not restore it.
The earlier hidden-window smoke check covered startup while hidden, and did not
catch the close-and-reopen failure.

Window requests now go through a dispatcher. On Windows it posts a native paint
message so eframe processes requests even when the window is hidden. Native tray
callbacks do not acquire egui's context lock. The Mac and Windows releases use
the same network protocol; a 0.1.0 Mac does not need updating for this PC fix.

The automated desktop lifecycle check repeats titlebar close, hidden engine/UI
updates, tray Open requests, and authenticated instance activation three times.
It also checks restoration from minimized state and process exit after Quit
while hidden. It uses an isolated profile and does not write to the clipboard.
The 13 existing automated tests and all-feature Clippy checks also pass.

To repeat the desktop check on Windows:

```powershell
.\scripts\dev.ps1 build --release --features ui-test --locked
.\scripts\test-windows-lifecycle.ps1
```

For a project-local GNU/LLVM build, keep the bundled `libunwind.dll` next to the
test executable, or include the LLVM toolchain's `bin` directory in PATH.
The production installer bundles this DLL automatically. Build without the
`ui-test` feature when packaging the production app.

## Original 0.1.0 checks

The app was developed on Windows. The checks below distinguish compiled code and simulated clipboard adapters from testing on an actual desktop.

| Check | Result |
| --- | --- |
| Windows x64 release build | Passed; required LLVM runtime is bundled with the installer. |
| Automated tests | 13 passed. This includes real encrypted loopback sessions between app engines using isolated in-memory clipboard adapters. |
| Formatting / Clippy | Passed, including all targets/features in the Windows toolchain. |
| Windows GUI | Actual rendered Settings and Clipboard windows inspected. Hidden-window repaint smoke check passed. |
| Single-instance activation | A second launch successfully activated the original background instance. |
| Windows installer | Installed into an isolated project directory, installed executable hash matched, launched with only system folders in PATH, and test installation uninstalled successfully. |
| Linux x64 release build | Successfully cross-compiled and linked with Zig targeting glibc 2.28. |
| Linux installer payload | Verified the self-extracting archive boundary, extracted its contents, and verified that the executable hash matches the release build. |
| Linux Rust type checking | Desktop code and test targets passed cross-target checking. |
| macOS Rust type checking | Core, configuration, and AppKit clipboard adapter passed Apple Silicon target checking. Native C objects were excluded for this type-only check because the Windows host lacks the Apple SDK. |
| Native clipboard round trips | Defined in `tests/native_clipboard.rs` and wired into native CI; deliberately not run against the user's real clipboard in this workspace. |
| macOS installer | User successfully built the universal `.pkg`, installed it, paired with the PC, and confirmed Mac close/reopen works. Not built or runtime-tested on this Windows host. |
| Mac build kit | Shell syntax and archive contents checked here; user confirmed the helper produced a `.pkg` on their Mac. |
| Linux desktop runtime | Not tested on a Linux desktop here. X11 and supported Wayland compositors still need native integration validation. |

The tests cover pairing before sharing; A → B → A text copies without echo; approval before even small text in manual mode; metadata-only large-file offers; preservation of a newer local clipboard; image transfer; declining; cancellation when manual mode is enabled; and rejecting late chunks from a cancelled transfer after retry.

Native CI is configured in `.github/workflows/build.yml` for Windows, macOS, and Linux. Current run results are available on GitHub Actions; the table above records the original local checks. Public release signing and Apple notarization are not configured.
# Version 0.1.3 changes

Added Windows ARM64 and Linux ARM64 to the native build matrix. Linux packages
include AppImage and per-user installers; Mac packages include DMG and PKG.
All filenames and mandatory release assets share one architecture manifest.
Windows embeds an ICO in the executable/setup, Mac embeds an ICNS, and Linux
uses the same white-and-green overlapping-box logo. Incoming copies retain the
logo with a status dot.

Discovery now handles mDNS service removal, removing expired nearby rows while
preserving paired identities and working connections. Transient connection
warnings clear on reconnection, expire after 30 seconds, and can be dismissed.
Diagnostic screenshot/lifecycle profiles do not advertise on the LAN.

Local verification: all 16 Rust tests pass (three regression tests cover these
device/notice failures), all-feature Clippy passes, formatting passes, and the
release verifier rejects tampering and missing packages for every architecture.
Native clipboard-writing tests remain reserved for CI desktops.

The [native check run for 037a965](https://github.com/bryancolvinfilm-bot/duet-clipboard/actions/runs/38994697797)
passed all five platforms, including Windows ARM64 and Linux ARM64 clipboard
round trips. Both Linux AppImages launched successfully without FUSE. Mac
packaging verified both CPU architectures, the app signature, the DMG, and the
embedded ICNS with Apple's iconutil. Both Windows installers built successfully.
