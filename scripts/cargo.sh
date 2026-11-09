#!/bin/sh
# Runs cargo in crates/, so package scripts work from anywhere in the repository.
set -eu
cd "$(dirname "$0")/../crates"
exec cargo "$@"
