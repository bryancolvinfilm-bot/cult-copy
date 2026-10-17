#!/bin/sh
# Source this from a script that has already set task_root.
task_version=$(sed -n 's/^version = "\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)"[[:space:]]*$/\1/p' "$task_root/Cargo.toml" | head -n 1)
if [ -z "$task_version" ]; then
  printf '%s\n' 'Cargo.toml must contain a stable major.minor.patch package version.' >&2
  exit 1
fi
