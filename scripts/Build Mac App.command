#!/bin/bash
# Run on a Mac. Builds an installer; does not replace the user's clipboard.
set -euo pipefail

task_root=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$task_root"

task_finish() {
    task_status=$?
    if [ "$task_status" -ne 0 ]; then
        printf '\nBuild stopped. The messages above explain why.\n'
        printf 'Build log: %s\n' "$task_root/dist/mac-build.log"
    fi
    if [ -t 0 ] && [ "${CULT_COPY_BUILD_NO_PAUSE:-0}" != 1 ]; then
        printf '\nPress Return to close this window. '
        read -r task_answer || true
    fi
    exit "$task_status"
}
trap task_finish EXIT

if [ "$(uname -s)" != Darwin ]; then
    printf '%s\n' 'Run this helper on macOS, not Windows or Linux.' >&2
    exit 1
fi

mkdir -p dist
exec > >(tee -a "$task_root/dist/mac-build.log") 2>&1
printf '\nCult Copy Mac build - %s\n' "$(date)"
printf '%s\n' 'This builds a universal Mac installer. It does not install the app or change your clipboard.'

task_major=$(sw_vers -productVersion | cut -d . -f 1)
if [ "$task_major" -lt 12 ]; then
    printf '%s\n' 'Cult Copy requires macOS 12 or newer.' >&2
    exit 1
fi

if ! xcode-select -p >/dev/null 2>&1 || ! xcrun --find clang >/dev/null 2>&1; then
    printf '%s\n' 'Apple Command Line Tools are required. Accept the Apple installation dialog, let it finish, then run this helper again.'
    xcode-select --install || true
    exit 1
fi

# Use the existing Rust installation when available. Otherwise install the
# official minimal toolchain into the user's Rust directory, without editing PATH.
task_cargo_home="${CARGO_HOME:-$HOME/.cargo}"
export PATH="$task_cargo_home/bin:$PATH"
if ! command -v rustup >/dev/null 2>&1; then
    printf '%s\n' 'Installing the official Rust toolchain in your user account...'
    task_temp=$(mktemp -d "${TMPDIR:-/tmp}/cult-copy-rustup.XXXXXXXX")
    if ! curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$task_temp/rustup-init.sh"; then
        rm -f -- "$task_temp/rustup-init.sh"
        rmdir -- "$task_temp"
        exit 1
    fi
    if sh "$task_temp/rustup-init.sh" -y --profile minimal --no-modify-path; then
        rm -f -- "$task_temp/rustup-init.sh"
        rmdir -- "$task_temp"
    else
        rm -f -- "$task_temp/rustup-init.sh"
        rmdir -- "$task_temp"
        exit 1
    fi
fi
rustup toolchain install stable --profile minimal
export RUSTUP_TOOLCHAIN=stable
export MACOSX_DEPLOYMENT_TARGET=12.0
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"

printf '\nRunning the automated checks (native clipboard-writing tests remain skipped)...\n'
cargo test --locked --all-targets
printf '\nBuilding Apple Silicon and Intel binaries, then packaging...\n'
sh scripts/package-unix.sh

. "$task_root/scripts/version.sh"
task_package="$task_root/dist/CultCopy-$task_version-macos-universal.dmg"
shasum -a 256 "$task_package" > dist/mac-package.sha256
printf '\nInstaller ready: %s\n' "$task_package"
printf '%s\n' 'Open the disk image and drag Cult Copy into Applications, then pair it with Windows in Devices.'
open -R "$task_package"
