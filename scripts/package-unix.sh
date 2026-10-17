#!/bin/sh
set -eu
task_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$task_root"
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
    cp LICENSE "$task_app/Resources/LICENSE"
    cp THIRD_PARTY_NOTICES.txt packaging/RUST-STDLIB-NOTICES.html "$task_app/Resources/"
    codesign --force --deep --sign - "dist/macos-root/Duet Clipboard.app"
    pkgbuild --root dist/macos-root --identifier io.duetclipboard.app --version 0.1.1 --install-location /Applications dist/DuetClipboard-0.1.1-macos-universal.pkg
    ;;
  Linux)
    cargo build --release --locked
    task_arch=$(uname -m)
    mkdir -p dist/linux-payload
    cp target/release/duet-clipboard packaging/duet-clipboard.svg LICENSE THIRD_PARTY_NOTICES.txt packaging/RUST-STDLIB-NOTICES.html dist/linux-payload/
    tar -czf dist/linux-payload.tar.gz -C dist/linux-payload duet-clipboard duet-clipboard.svg LICENSE THIRD_PARTY_NOTICES.txt RUST-STDLIB-NOTICES.html
    cat packaging/linux-installer.sh > "dist/DuetClipboard-0.1.1-linux-$task_arch.run"
    cat dist/linux-payload.tar.gz >> "dist/DuetClipboard-0.1.1-linux-$task_arch.run"
    chmod +x "dist/DuetClipboard-0.1.1-linux-$task_arch.run"
    ;;
  *) printf '%s\n' 'Build this installer on macOS or Linux.' >&2; exit 1 ;;
esac
