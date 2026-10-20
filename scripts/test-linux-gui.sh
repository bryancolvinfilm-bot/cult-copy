#!/bin/sh
# Render an actual eframe window on a private X11 display. The diagnostic build
# uses the same desktop dependencies and exits after saving its first frame.
set -eu
if [ ! -e /.dockerenv ] && [ ! -e /run/.containerenv ]; then
    echo 'Run the GUI smoke check only inside a disposable container.' >&2
    exit 1
fi
task_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$task_root"
cargo build --locked --features ui-test
task_tmp=$(mktemp -d "${TMPDIR:-/tmp}/cult-copy-gui.XXXXXXXX")
trap 'rm -rf -- "$task_tmp"' EXIT HUP INT TERM
# CULT_COPY_SCREENSHOT_TO also disables LAN discovery for this isolated test profile.
timeout 45s dbus-run-session -- xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 \
    CULT_COPY_SCREENSHOT_TO="$task_tmp/window.png" \
    "${CARGO_TARGET_DIR:-target}/debug/cult-copy" --data-dir "$task_tmp/profile"
test -s "$task_tmp/window.png"
printf '%s\n' 'Linux window rendered and exited on the private X11 display.'
