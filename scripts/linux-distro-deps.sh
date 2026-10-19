#!/bin/sh
# Provision only a disposable Fedora/Rocky build environment, as root.
set -eu
if [ ! -e /.dockerenv ] && [ ! -e /run/.containerenv ]; then
    echo 'Provision dependencies only inside a disposable build container.' >&2
    exit 1
fi
task_distro=${1:?Usage: linux-distro-deps.sh fedora|rocky8}
. /etc/os-release
case "$task_distro:$ID:$VERSION_ID" in
    fedora:fedora:44) ;;
    rocky8:rocky:8.*)
        dnf -y install dnf-plugins-core
        dnf config-manager --set-enabled powertools
        ;;
    *) echo 'Use Fedora 44 or Rocky Linux 8 for the matching release target.' >&2; exit 1 ;;
esac
dnf -y install gcc gcc-c++ make pkgconf-pkg-config git curl ca-certificates \
    tar gzip diffutils findutils which rpm-build desktop-file-utils \
    hicolor-icon-theme libX11-devel libXi-devel libXrandr-devel libXcursor-devel \
    libXinerama-devel mesa-libEGL-devel mesa-libGL-devel wayland-devel \
    libxkbcommon-devel xorg-x11-server-Xvfb xorg-x11-xauth dbus-daemon
