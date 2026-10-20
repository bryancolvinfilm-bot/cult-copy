#!/bin/bash
# Runs only after all native build jobs succeed. Requires GitHub CLI and pwsh.
set -euo pipefail
task_tag="v$APP_VERSION"
task_repo="$GITHUB_REPOSITORY"
task_sha="$GITHUB_SHA"
task_run="https://github.com/$task_repo/actions/runs/$GITHUB_RUN_ID"
task_remote_sha=$(gh api "repos/$task_repo/git/ref/heads/main" --jq '.object.sha')
if [ "$task_remote_sha" != "$task_sha" ]; then
  echo 'main changed during the build. Publish the current main commit instead.' >&2
  exit 1
fi
# Tags are immutable. A failed upload can resume only its own draft.
if git rev-parse --verify "refs/tags/$task_tag" >/dev/null 2>&1; then
  test "$(git rev-list -n 1 "$task_tag")" = "$task_sha" || { echo 'Release tag points at a different commit.' >&2; exit 1; }
else
  git -c user.name=github-actions -c user.email=github-actions@users.noreply.github.com tag -a "$task_tag" "$task_sha" -m "Cult Copy $APP_VERSION"
  git push origin "refs/tags/$task_tag"
fi
cat > release-notes.md <<EOF
Cult Copy shares text, images, files, and folders privately between paired computers on a local network.

This update completes the Cult Copy naming in Linux package metadata, executables, launchers, icons, startup entries, and public project descriptions. The Linux RPM replaces the earlier package; existing Linux profiles migrate after the previous app has quit. Disable clipboard sharing in overlapping display or keyboard/mouse tools so Cult Copy can manage clipboard transfers.

Cult Copy keeps its original icon, device identity, pairings, settings, and history through supported upgrades. When upgrading on Mac from the previous app name, quit it and use the PKG installer so the old app bundle is replaced by Cult Copy.app. New installs can use the DMG.

| Platform | Processor | Downloads |
| --- | --- | --- |
| Windows | Intel/AMD x64 | [Setup](https://github.com/$task_repo/releases/download/$task_tag/CultCopy-$APP_VERSION-windows-x64-setup.exe) |
| Windows | ARM64 | [Setup](https://github.com/$task_repo/releases/download/$task_tag/CultCopy-$APP_VERSION-windows-arm64-setup.exe) |
| macOS | Apple Silicon + Intel, universal | [DMG](https://github.com/$task_repo/releases/download/$task_tag/CultCopy-$APP_VERSION-macos-universal.dmg) / [PKG](https://github.com/$task_repo/releases/download/$task_tag/CultCopy-$APP_VERSION-macos-universal.pkg) |
| Linux | Intel/AMD x64 | [AppImage](https://github.com/$task_repo/releases/download/$task_tag/CultCopy-$APP_VERSION-linux-x64.AppImage) / [Installer](https://github.com/$task_repo/releases/download/$task_tag/CultCopy-$APP_VERSION-linux-x64.run) |
| Linux | ARM64 | [AppImage](https://github.com/$task_repo/releases/download/$task_tag/CultCopy-$APP_VERSION-linux-arm64.AppImage) / [Installer](https://github.com/$task_repo/releases/download/$task_tag/CultCopy-$APP_VERSION-linux-arm64.run) |
| Fedora 44 | Intel/AMD x64 / ARM64 | [x64 RPM](https://github.com/$task_repo/releases/download/$task_tag/CultCopy-$APP_VERSION-fedora-x64.rpm) / [ARM64 RPM](https://github.com/$task_repo/releases/download/$task_tag/CultCopy-$APP_VERSION-fedora-arm64.rpm) |
| Rocky Linux 8 | Intel/AMD x64 / ARM64 | [x64 RPM](https://github.com/$task_repo/releases/download/$task_tag/CultCopy-$APP_VERSION-rocky8-x64.rpm) / [ARM64 RPM](https://github.com/$task_repo/releases/download/$task_tag/CultCopy-$APP_VERSION-rocky8-arm64.rpm) |

Mac: open the DMG and drag the app into Applications, or use the PKG. Quit the existing app before replacing it. Linux: make the AppImage executable before opening it; alternatively run the per-user installer with sh. AppImages use the Ubuntu 24.04 baseline and need a compatible desktop and system graphics libraries. Fedora 44 and Rocky Linux 8 users can install their matching RPM with dnf. Rocky 8 RPMs are compiled on Rocky 8's glibc 2.28 baseline, independently of Ubuntu/Fedora builds. See the README for RPM migration, FUSE, and Wayland details.

These are unsigned development builds. The Mac application is ad-hoc signed and is not Apple-notarized; operating systems may require approval before opening it. See the README for receiving consent, local cache limitations, and Linux Wayland compatibility.

Source commit: [$task_sha](https://github.com/$task_repo/commit/$task_sha)
Native tests and builds: [$task_run]($task_run)

CHECKSUMS.sha256 covers every package and BUILD-INFO.json. BUILD-INFO.json identifies their version, source commit, sizes, and hashes.
EOF
if gh release view "$task_tag" --repo "$task_repo" --json isDraft --jq '.isDraft' > existing-release.txt 2>/dev/null; then
  test "$(cat existing-release.txt)" = true || { echo 'Refusing to modify an already-published release.' >&2; exit 1; }
else
  gh release create "$task_tag" --repo "$task_repo" --verify-tag --target "$task_sha" --draft --title "Cult Copy $APP_VERSION" --notes-file release-notes.md
fi
gh release edit "$task_tag" --repo "$task_repo" --notes-file release-notes.md
gh release upload "$task_tag" release-assets/* --repo "$task_repo" --clobber
# Verify the actual uploaded bytes before making the draft public.
mkdir -p verified-downloads
gh release download "$task_tag" --repo "$task_repo" --dir verified-downloads
pwsh -NoProfile -File scripts/release-assets.ps1 -Mode Verify -Directory verified-downloads -Version "$APP_VERSION" -SourceSha "$task_sha" -Repository "$task_repo"
test "$(gh api "repos/$task_repo/git/ref/heads/main" --jq '.object.sha')" = "$task_sha" || { echo 'main changed before publication.' >&2; exit 1; }
gh release edit "$task_tag" --repo "$task_repo" --draft=false --latest
printf 'Public downloads: https://github.com/%s/releases/tag/%s\n' "$task_repo" "$task_tag"
