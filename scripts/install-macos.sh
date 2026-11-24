#!/bin/sh
# Build NoteCraft.app and install it in /Applications (or the folder given as argument).
# Usage: scripts/install-macos.sh [--dmg] [destination]
#   --dmg   also write target/release/NoteCraft.dmg (drag-and-drop installer)
set -eu
cd "$(dirname "$0")/.."
DMG=0
if [ "${1:-}" = "--dmg" ]; then DMG=1; shift; fi
DEST="${1:-/Applications}"

scripts/bundle-macos.sh
APP=target/NoteCraft.noindex/NoteCraft.app

# ad-hoc signature: enough for a locally built app, avoids the damaged-app warning
codesign --force --deep --sign - --identifier local.notecraft.app "$APP" 2>/dev/null || true

if [ "$DMG" = 1 ]; then
  # writable image first, so Finder can lay the window out (icons, size), then compress it
  RW=$(mktemp -u).dmg
  rm -f target/release/NoteCraft.dmg
  hdiutil create -quiet -size 80m -fs HFS+ -volname NoteCraft -ov "$RW"
  MOUNT=$(hdiutil attach -nobrowse -noverify -noautoopen "$RW" | awk -F'\t' '/\/Volumes\// {print $NF}' | head -1)
  cp -R "$APP" "$MOUNT/"
  ln -s /Applications "$MOUNT/Applications"
  # Finder layout is cosmetic: a failure must not stop the build
  osascript >/dev/null 2>&1 <<OSA || echo "note: could not lay the DMG window out"
tell application "Finder"
  tell disk "NoteCraft"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {200, 140, 760, 460}
    set opts to the icon view options of container window
    set arrangement of opts to not arranged
    set icon size of opts to 112
    set position of item "NoteCraft.app" of container window to {140, 150}
    set position of item "Applications" of container window to {420, 150}
    close
    open
    update without registering applications
    delay 1
    close
  end tell
end tell
OSA
  sync
  hdiutil detach -quiet "$MOUNT" || hdiutil detach -force -quiet "$MOUNT"
  hdiutil convert -quiet "$RW" -format UDZO -imagekey zlib-level=9 -o target/release/NoteCraft.dmg
  rm -f "$RW"
  echo "Built target/release/NoteCraft.dmg"
fi

rm -rf "$DEST/NoteCraft.app"
cp -R "$APP" "$DEST/"
echo "Installed $DEST/NoteCraft.app"
