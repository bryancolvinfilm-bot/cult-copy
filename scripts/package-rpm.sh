#!/bin/sh
# Build and package natively; do not relabel an Ubuntu-linked executable.
set -eu
task_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$task_root"
. "$task_root/scripts/version.sh"
task_distro=${1:?Usage: package-rpm.sh fedora|rocky8}
. /etc/os-release
case "$task_distro:$ID:$VERSION_ID" in
    fedora:fedora:44) task_dist=.fc44 ;;
    rocky8:rocky:8.*) task_dist=.el8 ;;
    *) echo 'Build the RPM inside its Fedora 44 or Rocky Linux 8 environment.' >&2; exit 1 ;;
esac
case "$(uname -m)" in
    x86_64) task_arch=x64; task_target=x86_64-unknown-linux-gnu ;;
    aarch64) task_arch=arm64; task_target=aarch64-unknown-linux-gnu ;;
    *) echo 'RPM releases support x64 and ARM64.' >&2; exit 1 ;;
esac
test "$(rustc -vV | sed -n 's/^host: //p')" = "$task_target"
cargo build --release --locked
task_rpmroot=$(mktemp -d "${TMPDIR:-/tmp}/duet-rpm.XXXXXXXX")
trap 'rm -rf -- "$task_rpmroot"' EXIT HUP INT TERM
task_payload="$task_rpmroot/duet-clipboard-$task_version"
mkdir -p "$task_payload" "$task_rpmroot/SOURCES" "$task_rpmroot/RPMS"
cp "${CARGO_TARGET_DIR:-target}/release/duet-clipboard" "$task_payload/"
cp packaging/duet-clipboard.desktop packaging/duet-clipboard.svg LICENSE NOTICE.md \
    THIRD_PARTY_NOTICES.txt packaging/RUST-STDLIB-NOTICES.html "$task_payload/"
cp packaging/icons/icon-256.png "$task_payload/duet-clipboard.png"
tar -czf "$task_rpmroot/SOURCES/duet-clipboard-$task_version.tar.gz" \
    -C "$task_rpmroot" "duet-clipboard-$task_version"
task_name="DuetClipboard-$task_version-$task_distro-$task_arch.rpm"
rpmbuild -bb packaging/duet-clipboard.spec \
    --define "_topdir $task_rpmroot" --define "app_version $task_version" \
    --define "dist $task_dist" --define "_rpmfilename $task_name"
mkdir -p dist
cp "$task_rpmroot/RPMS/$task_name" "dist/$task_name"
rpm --checksig "dist/$task_name"
printf 'RPM: %s/dist/%s\n' "$task_root" "$task_name"
