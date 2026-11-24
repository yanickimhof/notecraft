#!/bin/sh
# Regenerate the pictures in docs/screenshots from a fresh sample workspace.
# Usage: scripts/screenshots.sh   (needs target/release/notecraft-app, see scripts/bundle-macos.sh)
set -eu
cd "$(dirname "$0")/.."
BIN=target/release/notecraft-app
OUT=docs/screenshots
mkdir -p "$OUT"
shot() { name=$1; shift; "$BIN" --screenshot "$OUT/$name.png" "$@"; echo "wrote $OUT/$name.png"; }
shot page --open "Getting started"
shot page-dark --open "Getting started" --dark
shot table --open Projects --view table
shot board --open Projects --view board
shot calendar --open Projects --view calendar
shot ai --open "Getting started" --panel ai
shot meeting --open "Weekly sync"
shot tabs --open "Getting started" --panel tabs
shot present --open "Getting started" --panel present:0
shot reminder --open "Getting started" --panel remind
