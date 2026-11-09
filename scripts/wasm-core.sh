#!/bin/sh
# Builds the core crates for WebAssembly, the promise that lets ome-wasm and the browser use them.
set -eu
cd "$(dirname "$0")/../crates"
exec cargo build --target wasm32-unknown-unknown --locked -p ome-api -p ome-crypto -p ome-wire -p ome-protocol -p ome-signer
