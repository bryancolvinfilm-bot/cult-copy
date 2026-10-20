#!/bin/sh
# Both backends run against private display servers and temporary app profiles.
set -eu
task_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$task_root"
task_backend=${1:?Usage: test-linux-lifecycle.sh x11|wayland}
case "$task_backend" in x11|wayland) ;; *) exit 1 ;; esac
if [ -n "${CULT_COPY_TEST_BINARY:-}" ]; then
    task_binary=$CULT_COPY_TEST_BINARY
else
    cargo build --locked --features ui-test
    task_binary="${CARGO_TARGET_DIR:-target}/debug/duet-clipboard"
fi
task_tmp=$(mktemp -d "${TMPDIR:-/tmp}/cult-copy-lifecycle.XXXXXXXX")
task_compositor=''
trap 'if [ -n "$task_compositor" ]; then kill "$task_compositor" 2>/dev/null || true; wait "$task_compositor" 2>/dev/null || true; fi; rm -rf -- "$task_tmp"' EXIT HUP INT TERM
mkdir -m 700 "$task_tmp/runtime"
export XDG_RUNTIME_DIR="$task_tmp/runtime"
unset DISPLAY WAYLAND_DISPLAY WAYLAND_SOCKET
if [ "$task_backend" = wayland ]; then
    case "$(weston --help 2>&1)" in
        *--renderer=*) task_renderer=--renderer=pixman ;;
        *) task_renderer=--use-pixman ;;
    esac
    XDG_RUNTIME_DIR="$task_tmp/runtime" weston --backend=headless-backend.so \
        "$task_renderer" --socket=wayland-cult-copy --idle-time=0 --no-config \
        --log="$task_tmp/weston.log" &
    task_compositor=$!
    task_wait=0
    while [ ! -S "$task_tmp/runtime/wayland-cult-copy" ]; do
        task_wait=$((task_wait + 1))
        if ! kill -0 "$task_compositor" 2>/dev/null || [ "$task_wait" -gt 100 ]; then
            cat "$task_tmp/weston.log" >&2
            exit 1
        fi
        sleep 0.1
    done
    export WAYLAND_DISPLAY=wayland-cult-copy
fi
for task_mode in normal background; do
    task_report="$task_tmp/$task_mode.json"
    if [ "$task_mode" = background ]; then set -- --background; else set --; fi
    # Xvfb also supplies a private fallback clipboard for compositors without
    # data-control support. The report must confirm the requested window backend.
    timeout 90s dbus-run-session -- xvfb-run -a env \
        LIBGL_ALWAYS_SOFTWARE=1 CULT_COPY_LIFECYCLE_TEST_TO="$task_report" \
        "$task_binary" --data-dir "$task_tmp/profile-$task_mode" "$@"
    python3 - "$task_report" "$task_backend" "$task_mode" <<'PY'
import json, sys
with open(sys.argv[1]) as stream:
    report = json.load(stream)
assert report['passed'], report
assert report['backend'] == sys.argv[2], report
assert report['background'] == (sys.argv[3] == 'background'), report
print(f"{sys.argv[2]} {sys.argv[3]}: close/reopen, hidden sync/tray updates, instance activation, disabled version, and tray Quit passed.")
PY
done
# Calling the exported menu catches failures hidden by invoking our handler
# directly, and restarting the same profile checks that the instance lock clears.
timeout 45s dbus-run-session -- xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 \
    python3 scripts/test-linux-tray.py "$task_binary" "$task_tmp/profile-tray"
