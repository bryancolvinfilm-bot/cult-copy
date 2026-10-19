#!/bin/sh
set -eu
task_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$task_root"
. "$task_root/scripts/version.sh"
mkdir -p dist
case "$(uname -s)" in
  Darwin)
    export MACOSX_DEPLOYMENT_TARGET=12.0
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    cargo build --release --locked --target aarch64-apple-darwin
    cargo build --release --locked --target x86_64-apple-darwin
    task_app="dist/macos-root/Cult Copy.app/Contents"
    mkdir -p "$task_app/MacOS" "$task_app/Resources"
    lipo -create target/aarch64-apple-darwin/release/duet-clipboard target/x86_64-apple-darwin/release/duet-clipboard -output "$task_app/MacOS/duet-clipboard"
    cp packaging/Info.plist "$task_app/Info.plist"
    /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $task_version" "$task_app/Info.plist"
    /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $task_version" "$task_app/Info.plist"
    cp LICENSE NOTICE.md "$task_app/Resources/"
    cp THIRD_PARTY_NOTICES.txt packaging/RUST-STDLIB-NOTICES.html "$task_app/Resources/"
    cp packaging/icons/cult-copy.icns "$task_app/Resources/"
    codesign --force --deep --sign - "dist/macos-root/Cult Copy.app"
    codesign --verify --strict "dist/macos-root/Cult Copy.app"
    lipo "$task_app/MacOS/duet-clipboard" -verify_arch arm64 x86_64
    pkgbuild --root dist/macos-root --component-plist packaging/macos-components.plist \
      --scripts packaging/macos-scripts --identifier io.duetclipboard.app \
      --version "$task_version" --install-location /Applications "dist/CultCopy-$task_version-macos-universal.pkg"
    pkgutil --payload-files "dist/CultCopy-$task_version-macos-universal.pkg" > dist/mac-package-files.txt
    # A drag-to-Applications download, alongside the existing per-system package.
    task_dmg_root=$(mktemp -d "${TMPDIR:-/tmp}/cult-copy-dmg.XXXXXXXX")
    # Keep the staging directory in TMPDIR if interrupted; do not recursively
    # remove a path that might have been changed by another process.
    # ditto preserves the signature and resource metadata in the copied bundle.
    ditto 'dist/macos-root/Cult Copy.app' "$task_dmg_root/Cult Copy.app"
    ln -s /Applications "$task_dmg_root/Applications"
    hdiutil create -volname 'Cult Copy' -srcfolder "$task_dmg_root" -format UDZO -ov "dist/CultCopy-$task_version-macos-universal.dmg"
    hdiutil verify "dist/CultCopy-$task_version-macos-universal.dmg"
    ;;
  Linux)
    cargo build --release --locked
    case "$(uname -m)" in
      x86_64) task_arch=x64 ;;
      aarch64) task_arch=arm64 ;;
      *) echo 'Linux packaging supports x64 and ARM64.' >&2; exit 1 ;;
    esac
    mkdir -p dist/linux-payload
    cp target/release/duet-clipboard LICENSE NOTICE.md THIRD_PARTY_NOTICES.txt packaging/RUST-STDLIB-NOTICES.html dist/linux-payload/
    cp packaging/cult-copy.svg dist/linux-payload/duet-clipboard.svg
    cp packaging/cult-copy.desktop dist/linux-payload/duet-clipboard.desktop
    cp packaging/icons/icon-256.png dist/linux-payload/duet-clipboard.png
    tar -czf dist/linux-payload.tar.gz -C dist/linux-payload duet-clipboard duet-clipboard.svg duet-clipboard.png duet-clipboard.desktop LICENSE NOTICE.md THIRD_PARTY_NOTICES.txt RUST-STDLIB-NOTICES.html
    cat packaging/linux-installer.sh > "dist/CultCopy-$task_version-linux-$task_arch.run"
    cat dist/linux-payload.tar.gz >> "dist/CultCopy-$task_version-linux-$task_arch.run"
    chmod +x "dist/CultCopy-$task_version-linux-$task_arch.run"
    sh scripts/package-appimage.sh
    ;;
  *) printf '%s\n' 'Build this installer on macOS or Linux.' >&2; exit 1 ;;
esac
