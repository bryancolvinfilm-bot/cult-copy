#!/bin/sh
set -eu
task_destination="${XDG_DATA_HOME:-$HOME/.local/share}/cult-copy"
task_apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
task_icon_theme="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
task_metadata="${XDG_DATA_HOME:-$HOME/.local/share}/metainfo"
task_startup="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
task_tmp=$(mktemp -d "${TMPDIR:-/tmp}/cult-copy.XXXXXXXX")
trap 'rm -rf -- "$task_tmp"' EXIT HUP INT TERM
task_payload=$(awk '/^__CULT_COPY_CLIPBOARD_PAYLOAD__$/ { print NR + 1; exit }' "$0")
tail -n "+$task_payload" "$0" | tar -xz -C "$task_tmp"
mkdir -p "$task_destination" "$task_apps" "$task_metadata" "$task_icon_theme/scalable/apps" "$task_icon_theme/256x256/apps"
install -m 755 "$task_tmp/cult-copy" "$task_destination/cult-copy"
install -m 644 "$task_tmp/LICENSE" "$task_destination/LICENSE"
install -m 644 "$task_tmp/NOTICE.md" "$task_destination/NOTICE.md"
install -m 644 "$task_tmp/THIRD_PARTY_NOTICES.txt" "$task_destination/THIRD_PARTY_NOTICES.txt"
install -m 644 "$task_tmp/RUST-STDLIB-NOTICES.html" "$task_destination/RUST-STDLIB-NOTICES.html"
install -m 644 "$task_tmp/cult-copy.svg" "$task_icon_theme/scalable/apps/cult-copy.svg"
install -m 644 "$task_tmp/cult-copy.png" "$task_icon_theme/256x256/apps/cult-copy.png"
install -m 644 "$task_tmp/io.github.bryancolvinfilm_bot.CultCopy.metainfo.xml" "$task_metadata/"
# Desktop Entry Exec quoting is different from shell quoting.
task_exec=$(printf '%s' "$task_destination/cult-copy" | sed 's/\\/\\\\/g; s/"/\\"/g; s/`/\\`/g; s/\$/\\$/g; s/%/%%/g')
while IFS= read -r task_line || [ -n "$task_line" ]; do
    case "$task_line" in
        Exec=*) printf 'Exec="%s"\n' "$task_exec" ;;
        *) printf '%s\n' "$task_line" ;;
    esac
done < "$task_tmp/cult-copy.desktop" > "$task_apps/cult-copy.desktop"
# Preserve enabled login startup while removing the obsolete launcher and binary.
if [ -f "$task_startup/duet-clipboard.desktop" ]; then
    if ! grep -q '^X-GNOME-Autostart-enabled=false$' "$task_startup/duet-clipboard.desktop"; then
        mkdir -p "$task_startup"
        sed 's/^Exec=.*/& --background/' "$task_apps/cult-copy.desktop" > "$task_startup/cult-copy.desktop"
    fi
    rm -f -- "$task_startup/duet-clipboard.desktop"
fi
rm -f -- "$task_apps/duet-clipboard.desktop" \
    "$task_icon_theme/scalable/apps/duet-clipboard.svg" \
    "$task_icon_theme/256x256/apps/duet-clipboard.png"
task_legacy="${XDG_DATA_HOME:-$HOME/.local/share}/duet-clipboard"
if [ -f "$task_legacy/duet-clipboard" ]; then
    for task_file in duet-clipboard LICENSE NOTICE.md THIRD_PARTY_NOTICES.txt RUST-STDLIB-NOTICES.html; do
        rm -f -- "$task_legacy/$task_file"
    done
    rmdir -- "$task_legacy" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$task_icon_theme" >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$task_apps" >/dev/null 2>&1 || true
fi
printf '%s\n' 'Installed Cult Copy for your user.' 'Open Cult Copy from your applications menu, then pair your computers.'
exit 0
__CULT_COPY_CLIPBOARD_PAYLOAD__
