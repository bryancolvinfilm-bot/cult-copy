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
    task_app="dist/macos-root/Duet Clipboard.app/Contents"
    mkdir -p "$task_app/MacOS" "$task_app/Resources"
    lipo -create target/aarch64-apple-darwin/release/duet-clipboard target/x86_64-apple-darwin/release/duet-clipboard -output "$task_app/MacOS/duet-clipboard"
    cp packaging/Info.plist "$task_app/Info.plist"
    /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $task_version" "$task_app/Info.plist"
    /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $task_version" "$task_app/Info.plist"
    cp LICENSE NOTICE.md "$task_app/Resources/"
    cp THIRD_PARTY_NOTICES.txt packaging/RUST-STDLIB-NOTICES.html "$task_app/Resources/"
    codesign --force --deep --sign - "dist/macos-root/Duet Clipboard.app"
    codesign --verify --strict "dist/macos-root/Duet Clipboard.app"
    lipo -verify_arch arm64 x86_64 "$task_app/MacOS/duet-clipboard"
    pkgbuild --root dist/macos-root --identifier io.duetclipboard.app --version "$task_version" --install-location /Applications "dist/DuetClipboard-$task_version-macos-universal.pkg"
    pkgutil --payload-files "dist/DuetClipboard-$task_version-macos-universal.pkg" > dist/mac-package-files.txt
    ;;
  Linux)
    cargo build --release --locked
    task_arch=$(uname -m)
    mkdir -p dist/linux-payload
    cp target/release/duet-clipboard packaging/duet-clipboard.svg LICENSE NOTICE.md THIRD_PARTY_NOTICES.txt packaging/RUST-STDLIB-NOTICES.html dist/linux-payload/
    tar -czf dist/linux-payload.tar.gz -C dist/linux-payload duet-clipboard duet-clipboard.svg LICENSE NOTICE.md THIRD_PARTY_NOTICES.txt RUST-STDLIB-NOTICES.html
    cat packaging/linux-installer.sh > "dist/DuetClipboard-$task_version-linux-$task_arch.run"
    cat dist/linux-payload.tar.gz >> "dist/DuetClipboard-$task_version-linux-$task_arch.run"
    chmod +x "dist/DuetClipboard-$task_version-linux-$task_arch.run"
    ;;
  *) printf '%s\n' 'Build this installer on macOS or Linux.' >&2; exit 1 ;;
esac
