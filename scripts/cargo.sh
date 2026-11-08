#!/bin/sh
# Runs cargo in crates/ once the workspace has a member. Cargo refuses to format, build, lint, test or audit an empty
# virtual workspace, so until the first crate lands these lanes skip instead of failing.
set -eu
cd "$(dirname "$0")/../crates"
if ! ls ./*/Cargo.toml >/dev/null 2>&1; then
  echo "crates: no workspace members yet, skipping cargo $1"
  exit 0
fi
exec cargo "$@"
