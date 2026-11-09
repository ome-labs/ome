#!/bin/sh
# omed: Ome's daemon, a Node program, from a release archive (lib/omed beside this bin/). Runs it with the first of:
# OME_NODE, the Node this install was set up with (written below by install.sh or the Homebrew formula), Ome's own
# runtime in $OME_HOME/runtime, and node on PATH. Services see no PATH, so the installers fill in node_default.
set -eu

node_default=""

self=$0
while [ -L "$self" ]; do
  link=$(readlink "$self")
  case $link in
    /*) self=$link ;;
    *) self=$(dirname "$self")/$link ;;
  esac
done
root=$(cd "$(dirname "$self")/.." && pwd -P)
daemon="$root/lib/omed/node_modules/@ome-labs/daemon/dist/omed.js"

for node in "${OME_NODE:-}" "$node_default" "${OME_HOME:-$HOME/.ome}/runtime/bin/node" "$(command -v node || true)"; do
  if [ -n "$node" ] && [ -x "$node" ]; then
    exec "$node" "$daemon" "$@"
  fi
done
echo "omed: Node 24 or later was not found. Install it, or set OME_NODE to its path." >&2
exit 1
