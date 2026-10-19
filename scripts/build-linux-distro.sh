#!/usr/bin/env bash
# Reproduce a release build on the host architecture using Podman or Docker.
set -euo pipefail
task_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$task_root"
task_distro=${1:?Usage: build-linux-distro.sh fedora|rocky8 [podman|docker]}
case "$task_distro" in
    fedora) task_image=quay.io/fedora/fedora:44 ;;
    rocky8) task_image=docker.io/library/rockylinux:8 ;;
    *) echo 'Choose fedora or rocky8.' >&2; exit 1 ;;
esac
task_engine=${2:-}
if [ -z "$task_engine" ]; then
    if command -v podman >/dev/null 2>&1; then task_engine=podman; else task_engine=docker; fi
fi
mkdir -p dist
# An explicit source list keeps local profiles, Git authentication, caches,
# build outputs, and clipboard data out of the container's build context.
tar -cf - Cargo.toml Cargo.lock build.rs src tests packaging scripts \
    LICENSE NOTICE.md THIRD_PARTY_NOTICES.txt |
    "$task_engine" run --rm -i --security-opt label=disable \
        -v "$task_root/dist:/output" -e DUET_DISTRO="$task_distro" "$task_image" \
        bash -euc '
            mkdir -p /build
            tar -xf - -C /build
            cd /build
            sh scripts/linux-distro-deps.sh "$DUET_DISTRO"
            curl --proto "=https" --tlsv1.2 -fLsS https://sh.rustup.rs -o /tmp/duet-rustup.sh
            sh /tmp/duet-rustup.sh -y --profile minimal --default-toolchain stable
            . "$HOME/.cargo/env"
            rustup component add rustfmt clippy
            cargo fmt --all -- --check
            cargo clippy --locked --all-targets --all-features -- -D warnings
            cargo test --locked --all-targets
            dbus-run-session -- xvfb-run -a cargo test --locked --test native_clipboard -- --ignored --test-threads=1
            sh scripts/package-rpm.sh "$DUET_DISTRO"
            sh scripts/test-linux-rpm.sh "$DUET_DISTRO"
            cp dist/*.rpm /output/
        '
