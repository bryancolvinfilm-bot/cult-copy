#!/bin/sh
set -eu
task_destination="${XDG_DATA_HOME:-$HOME/.local/share}/duet-clipboard"
task_apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
task_icons="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
task_tmp=$(mktemp -d "${TMPDIR:-/tmp}/duet-clipboard.XXXXXXXX")
trap 'rm -rf -- "$task_tmp"' EXIT HUP INT TERM
task_payload=$(awk '/^__DUET_CLIPBOARD_PAYLOAD__$/ { print NR + 1; exit }' "$0")
tail -n "+$task_payload" "$0" | tar -xz -C "$task_tmp"
mkdir -p "$task_destination" "$task_apps" "$task_icons"
install -m 755 "$task_tmp/duet-clipboard" "$task_destination/duet-clipboard"
install -m 644 "$task_tmp/LICENSE" "$task_destination/LICENSE"
install -m 644 "$task_tmp/NOTICE.md" "$task_destination/NOTICE.md"
install -m 644 "$task_tmp/THIRD_PARTY_NOTICES.txt" "$task_destination/THIRD_PARTY_NOTICES.txt"
install -m 644 "$task_tmp/RUST-STDLIB-NOTICES.html" "$task_destination/RUST-STDLIB-NOTICES.html"
install -m 644 "$task_tmp/duet-clipboard.svg" "$task_icons/duet-clipboard.svg"
# Desktop Entry Exec quoting is different from shell quoting.
task_exec=$(printf '%s' "$task_destination/duet-clipboard" | sed 's/\\/\\\\/g; s/"/\\"/g; s/`/\\`/g; s/\$/\\$/g; s/%/%%/g')
printf '[Desktop Entry]\nType=Application\nName=Duet Clipboard\nComment=Share your clipboard with your paired computers\nExec="%s"\nIcon=duet-clipboard\nTerminal=false\nCategories=Utility;\n' "$task_exec" > "$task_apps/duet-clipboard.desktop"
printf '%s\n' 'Installed Duet Clipboard for your user.' 'Open Duet Clipboard from your applications menu, then pair your computers.'
exit 0
__DUET_CLIPBOARD_PAYLOAD__
