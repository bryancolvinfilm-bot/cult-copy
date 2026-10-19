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
task_package="dist/DuetClipboard-$task_version-$task_distro-$task_arch.rpm"
dnf -y install "$task_package"
test "$(duet-clipboard --version)" = "Duet Clipboard $task_version"
test "$(rpm -q --qf '%{ARCH}' duet-clipboard)" = "$(uname -m)"
rpm -V duet-clipboard
desktop-file-validate /usr/share/applications/duet-clipboard.desktop
test "$(sed -n 's/^Icon=//p' /usr/share/applications/duet-clipboard.desktop)" = duet-clipboard
test "$(sed -n 's/^StartupWMClass=//p' /usr/share/applications/duet-clipboard.desktop)" = duet-clipboard
cmp packaging/duet-clipboard.svg /usr/share/icons/hicolor/scalable/apps/duet-clipboard.svg
cmp packaging/icons/icon-256.png /usr/share/icons/hicolor/256x256/apps/duet-clipboard.png
dnf -y remove duet-clipboard
test ! -e /usr/bin/duet-clipboard
test ! -e /usr/share/applications/duet-clipboard.desktop
printf '%s\n' 'RPM install, dependencies, executable, icons, desktop entry, and uninstall passed.'
