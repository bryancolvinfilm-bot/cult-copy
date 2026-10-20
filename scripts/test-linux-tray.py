"""Exercise the real Linux tray D-Bus menu on a disposable display/session."""
import os
from pathlib import Path
import re
import subprocess
import sys
import time


def call(destination, path, method, *args):
    return subprocess.check_output(
        ["gdbus", "call", "--session", "--dest", destination,
         "--object-path", path, "--method", method, "--", *args],
        universal_newlines=True, stderr=subprocess.STDOUT, timeout=5,
    )


binary, profile = sys.argv[1:]
Path(profile).mkdir(parents=True, exist_ok=True)
for mode in ["background", "normal", "background"]:
    with open(Path(profile) / "tray-test.log", "w+") as log:
        process = subprocess.Popen(
            [binary, "--data-dir", profile] + (["--background"] if mode == "background" else []),
            env={**os.environ, "CULT_COPY_TRAY_TEST": "1"}, stdout=log, stderr=log,
        )
        try:
            deadline = time.monotonic() + 10
            while True:
                assert process.poll() is None, "App exited before registering its tray"
                names = call("org.freedesktop.DBus", "/org/freedesktop/DBus",
                             "org.freedesktop.DBus.ListNames")
                name = re.search(rf"org\.kde\.StatusNotifierItem-{process.pid}-[^'\s]+", names)
                if name:
                    break
                assert time.monotonic() < deadline, "Tray did not register"
                time.sleep(0.05)
            layout = call(name[0], "/MenuBar", "com.canonical.dbusmenu.GetLayout", "0", "-1", "[]")
            quit_item = re.search(r"\((\d+), \{[^{}]*'label': <'Quit'>", layout)
            assert quit_item, f"Tray has no Quit menu item: {layout}"
            try:
                call(name[0], "/MenuBar", "com.canonical.dbusmenu.Event",
                     quit_item[1], "clicked", "<0>", "0")
            except subprocess.CalledProcessError as error:
                # Quit can unregister the bus service before its reply arrives.
                # A successful process exit below is still required.
                if "org.freedesktop.DBus.Error.NoReply" not in error.output:
                    raise
            assert process.wait(timeout=8) == 0, "Tray Quit failed"
        except BaseException:
            process.kill()
            process.wait()
            log.seek(0)
            print(log.read(), file=sys.stderr)
            raise
    print(f"{mode}: real D-Bus tray Quit exited; same profile can restart")
