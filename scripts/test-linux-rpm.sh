#!/bin/sh
# Install/uninstall only inside the disposable distro build container.
set -eu
if [ ! -e /.dockerenv ] && [ ! -e /run/.containerenv ]; then
    echo 'Run RPM install/uninstall checks only inside a disposable container.' >&2
    exit 1
fi
task_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$task_root"
. "$task_root/scripts/version.sh"
task_distro=${1:?Usage: test-linux-rpm.sh fedora|rocky8}
case "$(uname -m)" in
    x86_64) task_arch=x64 ;;
    aarch64) task_arch=arm64 ;;
    *) exit 1 ;;
esac
task_package="dist/CultCopy-$task_version-$task_distro-$task_arch.rpm"
task_legacy=$(mktemp -d "${TMPDIR:-/tmp}/cult-copy-upgrade.XXXXXXXX")
trap 'rm -rf -- "$task_legacy"' EXIT HUP INT TERM
curl --proto '=https' --tlsv1.2 -fLsS \
    "https://github.com/bryancolvinfilm-bot/cult-copy/releases/download/v0.1.5/DuetClipboard-0.1.5-$task_distro-$task_arch.rpm" \
    -o "$task_legacy/legacy.rpm"
dnf -y install "$task_legacy/legacy.rpm"
test "$(rpm -q --qf '%{VERSION}' duet-clipboard)" = 0.1.5
dnf -y install "$task_package"
test ! -e /usr/bin/duet-clipboard
test ! -e /usr/share/applications/duet-clipboard.desktop
test "$(rpm -q --qf '%{NAME}' cult-copy)" = cult-copy
test -e /usr/share/metainfo/io.github.bryancolvinfilm_bot.CultCopy.metainfo.xml
test "$(rpm -q --qf '%{VERSION}' cult-copy)" = "$task_version"
test "$(grep '^Name=' /usr/share/applications/cult-copy.desktop)" = 'Name=Cult Copy'
test "$(cult-copy --version)" = "Cult Copy $task_version"
test "$(rpm -q --qf '%{ARCH}' cult-copy)" = "$(uname -m)"
rpm -V cult-copy
desktop-file-validate /usr/share/applications/cult-copy.desktop
test "$(sed -n 's/^Icon=//p' /usr/share/applications/cult-copy.desktop)" = cult-copy
test "$(sed -n 's/^StartupWMClass=//p' /usr/share/applications/cult-copy.desktop)" = cult-copy
cmp packaging/cult-copy.svg /usr/share/icons/hicolor/scalable/apps/cult-copy.svg
cmp packaging/icons/icon-256.png /usr/share/icons/hicolor/256x256/apps/cult-copy.png
sh scripts/test-linux-gui.sh
sh scripts/test-linux-lifecycle.sh x11
if [ "$task_distro" = fedora ]; then
    sh scripts/test-linux-lifecycle.sh wayland
fi
dnf -y remove cult-copy
test ! -e /usr/bin/cult-copy
test ! -e /usr/share/applications/cult-copy.desktop
printf '%s\n' 'RPM install, dependencies, executable, icons, desktop entry, and uninstall passed.'
