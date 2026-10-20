#!/bin/sh
# Real package installation is confined to the disposable GitHub runner.
set -eu
test "${GITHUB_ACTIONS:-}" = true
task_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$task_root"
. scripts/version.sh
task_old='/Applications/Duet Clipboard.app'
task_new='/Applications/Cult Copy.app'
test ! -e "$task_old"
test ! -e "$task_new"
sudo mkdir -p "$task_old/Contents"
sudo cp packaging/Info.plist "$task_old/Contents/Info.plist"
sudo /usr/libexec/PlistBuddy -c 'Set :CFBundleVersion 0.1.5' "$task_old/Contents/Info.plist"
sudo touch "$task_old/legacy-upgrade-marker"
sudo installer -pkg "dist/CultCopy-$task_version-macos-universal.pkg" -target /
test ! -e "$task_old"
test -d "$task_new"
test ! -e "$task_new/legacy-upgrade-marker"
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$task_new/Contents/Info.plist")" = io.duetclipboard.app
test "$("$task_new/Contents/MacOS/cult-copy" --version)" = "Cult Copy $task_version"
codesign --verify --strict "$task_new"
pkgutil --pkg-info io.duetclipboard.app
printf '%s\n' 'Mac PKG upgraded the legacy bundle to Cult Copy with the same identity.'
