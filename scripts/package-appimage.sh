#!/bin/sh
set -eu
task_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$task_root"
. "$task_root/scripts/version.sh"
case "$(uname -m)" in
  x86_64)
    task_arch=x64; task_machine=x86_64
    task_tool_sha=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
    task_runtime_sha=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d ;;
  aarch64)
    task_arch=arm64; task_machine=aarch64
    task_tool_sha=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
    task_runtime_sha=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444 ;;
  *) echo 'AppImage packaging supports x64 and ARM64.' >&2; exit 1 ;;
esac
mkdir -p .tools/appimage
# Versioned upstream releases with pinned hashes; never execute a moving download.
task_tool="$task_root/.tools/appimage/appimagetool-1.9.1-$task_machine.AppImage"
task_runtime="$task_root/.tools/appimage/runtime-20251108-$task_machine"
curl --proto '=https' --tlsv1.2 -fLsS --retry 3 "https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-$task_machine.AppImage" -o "$task_tool"
curl --proto '=https' --tlsv1.2 -fLsS --retry 3 "https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-$task_machine" -o "$task_runtime"
printf '%s  %s\n%s  %s\n' "$task_tool_sha" "$task_tool" "$task_runtime_sha" "$task_runtime" | sha256sum -c -
chmod +x "$task_tool"
task_appdir="$task_root/dist/CultCopy-$task_arch.AppDir"
mkdir -p "$task_appdir/usr/bin" "$task_appdir/usr/share/icons/hicolor/256x256/apps" "$task_appdir/usr/share/doc/duet-clipboard"
cp target/release/duet-clipboard "$task_appdir/usr/bin/"
cp packaging/AppRun "$task_appdir/"
cp packaging/cult-copy.desktop "$task_appdir/duet-clipboard.desktop"
cp packaging/icons/icon-256.png "$task_appdir/duet-clipboard.png"
cp packaging/icons/icon-256.png "$task_appdir/.DirIcon"
cp packaging/icons/icon-256.png "$task_appdir/usr/share/icons/hicolor/256x256/apps/duet-clipboard.png"
cp LICENSE NOTICE.md THIRD_PARTY_NOTICES.txt packaging/RUST-STDLIB-NOTICES.html "$task_appdir/usr/share/doc/duet-clipboard/"
cp packaging/APPIMAGE-RUNTIME-LICENSE.txt "$task_appdir/usr/share/doc/duet-clipboard/"
chmod +x "$task_appdir/AppRun" "$task_appdir/usr/bin/duet-clipboard"
# No FUSE dependency on build runners. The result supports normal FUSE mounting
# and --appimage-extract-and-run on desktops without FUSE.
ARCH="$task_machine" VERSION="$task_version" APPIMAGE_EXTRACT_AND_RUN=1 "$task_tool" --runtime-file "$task_runtime" "$task_appdir" "dist/CultCopy-$task_version-linux-$task_arch.AppImage"
