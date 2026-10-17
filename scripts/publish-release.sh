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
  git -c user.name=github-actions -c user.email=github-actions@users.noreply.github.com tag -a "$task_tag" "$task_sha" -m "Duet Clipboard $APP_VERSION"
  git push origin "refs/tags/$task_tag"
fi
cat > release-notes.md <<EOF
Independent, third-party open-source clipboard sharing on a local network. Not affiliated with, endorsed by, or maintained by Duet Display.

Download the Windows x64 setup.exe, macOS universal .pkg (Apple Silicon and Intel), or Linux x86_64 .run installer below. On Linux, run the installer with sh.

These are unsigned development builds. The Mac application is ad-hoc signed and is not Apple-notarized; operating systems may require approval before opening it. See the README for receiving consent, local cache limitations, and Linux Wayland compatibility.

Source commit: [$task_sha](https://github.com/$task_repo/commit/$task_sha)
Native tests and builds: [$task_run]($task_run)

CHECKSUMS.sha256 covers the three installers and BUILD-INFO.json. BUILD-INFO.json identifies their version, source commit, sizes, and hashes.
EOF
if gh release view "$task_tag" --repo "$task_repo" --json isDraft --jq '.isDraft' > existing-release.txt 2>/dev/null; then
  test "$(cat existing-release.txt)" = true || { echo 'Refusing to modify an already-published release.' >&2; exit 1; }
else
  gh release create "$task_tag" --repo "$task_repo" --verify-tag --target "$task_sha" --draft --title "Duet Clipboard $APP_VERSION" --notes-file release-notes.md
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
