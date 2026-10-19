#!/bin/sh
set -eu
task_destination="${XDG_DATA_HOME:-$HOME/.local/share}/duet-clipboard"
task_apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
task_icon_theme="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
task_tmp=$(mktemp -d "${TMPDIR:-/tmp}/duet-clipboard.XXXXXXXX")
trap 'rm -rf -- "$task_tmp"' EXIT HUP INT TERM
task_payload=$(awk '/^__DUET_CLIPBOARD_PAYLOAD__$/ { print NR + 1; exit }' "$0")
tail -n "+$task_payload" "$0" | tar -xz -C "$task_tmp"
mkdir -p "$task_destination" "$task_apps" "$task_icon_theme/scalable/apps" "$task_icon_theme/256x256/apps"
install -m 755 "$task_tmp/duet-clipboard" "$task_destination/duet-clipboard"
install -m 644 "$task_tmp/LICENSE" "$task_destination/LICENSE"
install -m 644 "$task_tmp/NOTICE.md" "$task_destination/NOTICE.md"
install -m 644 "$task_tmp/THIRD_PARTY_NOTICES.txt" "$task_destination/THIRD_PARTY_NOTICES.txt"
install -m 644 "$task_tmp/RUST-STDLIB-NOTICES.html" "$task_destination/RUST-STDLIB-NOTICES.html"
install -m 644 "$task_tmp/duet-clipboard.svg" "$task_icon_theme/scalable/apps/duet-clipboard.svg"
install -m 644 "$task_tmp/duet-clipboard.png" "$task_icon_theme/256x256/apps/duet-clipboard.png"
# Desktop Entry Exec quoting is different from shell quoting.
task_exec=$(printf '%s' "$task_destination/duet-clipboard" | sed 's/\\/\\\\/g; s/"/\\"/g; s/`/\\`/g; s/\$/\\$/g; s/%/%%/g')
while IFS= read -r task_line || [ -n "$task_line" ]; do
    case "$task_line" in
        Exec=*) printf 'Exec="%s"\n' "$task_exec" ;;
        *) printf '%s\n' "$task_line" ;;
    esac
done < "$task_tmp/duet-clipboard.desktop" > "$task_apps/duet-clipboard.desktop"
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$task_icon_theme" >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$task_apps" >/dev/null 2>&1 || true
fi
printf '%s\n' 'Installed Duet Clipboard for your user.' 'Open Duet Clipboard from your applications menu, then pair your computers.'
exit 0
__DUET_CLIPBOARD_PAYLOAD__
