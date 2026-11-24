#!/bin/bash
# Render docs/brand/features.html to PNG with headless Chrome (macOS).
set -euo pipefail
cd "$(dirname "$0")/../docs/brand"
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
for f in features; do
  "$CHROME" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=2 \
    --window-size=1600,900 --allow-file-access-from-files --screenshot="$PWD/$f.png" "file://$PWD/$f.html" >/dev/null 2>&1
  echo "rendered $f.png"
done
