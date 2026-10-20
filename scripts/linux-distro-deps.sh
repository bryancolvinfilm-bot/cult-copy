#!/bin/sh
# Provision only a disposable Fedora/Rocky build environment, as root.
set -eu
if [ ! -e /.dockerenv ] && [ ! -e /run/.containerenv ]; then
    echo 'Provision dependencies only inside a disposable build container.' >&2
    exit 1
fi
task_distro=${1:?Usage: linux-distro-deps.sh fedora|rocky8}
# Container images and mirrors can retain metadata for superseded RPMs. Refresh
# before resolving packages, and retry once with the cached metadata removed.
task_install() {
    if dnf -y --refresh install "$@"; then
        return 0
    fi
    dnf clean metadata
    dnf -y --refresh install "$@"
}
. /etc/os-release
case "$task_distro:$ID:$VERSION_ID" in
    fedora:fedora:44)
        task_install weston
        ;;
    rocky8:rocky:8.*)
        task_install dnf-plugins-core
        dnf config-manager --set-enabled powertools
        ;;
    *) echo 'Use Fedora 44 or Rocky Linux 8 for the matching release target.' >&2; exit 1 ;;
esac
task_install gcc gcc-c++ make pkgconf-pkg-config git curl ca-certificates \
    tar gzip diffutils findutils which rpm-build desktop-file-utils \
    hicolor-icon-theme libX11-devel libXi-devel libXrandr-devel libXcursor-devel \
    libXinerama-devel mesa-libEGL-devel mesa-libGL-devel mesa-dri-drivers wayland-devel \
    libxkbcommon-devel xorg-x11-server-Xvfb xorg-x11-xauth dbus-daemon glib2 python3
