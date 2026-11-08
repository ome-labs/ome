#!/bin/sh
# Builds the core crates for WebAssembly, the promise that lets ome-wasm and the browser use them. Core crates join
# this check as they land; until one exists there is nothing to build.
set -eu
cd "$(dirname "$0")/../crates"

packages=""
for crate in api crypto wire protocol signer; do
  if [ -f "$crate/Cargo.toml" ]; then
    packages="$packages -p ome-$crate"
  fi
done

if [ -z "$packages" ]; then
  echo "crates: no core crates yet, skipping the wasm32-unknown-unknown build"
  exit 0
fi

# shellcheck disable=SC2086 # one -p flag per crate
exec cargo build --target wasm32-unknown-unknown --locked $packages
