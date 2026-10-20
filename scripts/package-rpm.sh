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
task_rpmroot=$(mktemp -d "${TMPDIR:-/tmp}/cult-copy-rpm.XXXXXXXX")
trap 'rm -rf -- "$task_rpmroot"' EXIT HUP INT TERM
task_payload="$task_rpmroot/cult-copy-$task_version"
mkdir -p "$task_payload" "$task_rpmroot/SOURCES" "$task_rpmroot/RPMS"
cp "${CARGO_TARGET_DIR:-target}/release/cult-copy" "$task_payload/"
cp LICENSE NOTICE.md \
    THIRD_PARTY_NOTICES.txt packaging/RUST-STDLIB-NOTICES.html "$task_payload/"
cp packaging/cult-copy.desktop "$task_payload/cult-copy.desktop"
cp packaging/cult-copy.svg "$task_payload/cult-copy.svg"
cp packaging/icons/icon-256.png "$task_payload/cult-copy.png"
cp packaging/io.github.bryancolvinfilm_bot.CultCopy.metainfo.xml "$task_payload/"
tar -czf "$task_rpmroot/SOURCES/cult-copy-$task_version.tar.gz" \
    -C "$task_rpmroot" "cult-copy-$task_version"
task_name="CultCopy-$task_version-$task_distro-$task_arch.rpm"
rpmbuild -bb packaging/cult-copy.spec \
    --define "_topdir $task_rpmroot" --define "app_version $task_version" \
    --define "dist $task_dist" --define "_rpmfilename $task_name"
mkdir -p dist
cp "$task_rpmroot/RPMS/$task_name" "dist/$task_name"
rpm --checksig "dist/$task_name"
printf 'RPM: %s/dist/%s\n' "$task_root" "$task_name"
