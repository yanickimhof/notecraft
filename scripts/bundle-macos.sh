#!/bin/sh
# Build NoteCraft.app (release) with its icon generated from assets/icon.svg.
# Usage: scripts/bundle-macos.sh   → target/NoteCraft.noindex/NoteCraft.app
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p notecraft-app -p notecraft-mcp -p notecraft-cli
APP=target/NoteCraft.noindex/NoteCraft.app
rm -rf "$APP" target/release/NoteCraft.app
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/notecraft-app "$APP/Contents/MacOS/NoteCraft"
# the MCP server and the CLI live next to the app so the Settings screen can show a working command
cp target/release/notecraft-mcp "$APP/Contents/MacOS/notecraft-mcp"
# macOS is case-insensitive: the CLI cannot be called "notecraft" next to the "NoteCraft" executable
cp target/release/notecraft "$APP/Contents/MacOS/notecraft-cli"
# the .icns is rendered from assets/icon.svg at build time and embedded in the binary
"$APP/Contents/MacOS/NoteCraft" --export-icns "$APP/Contents/Resources/NoteCraft.icns"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>NøteCraft</string>
  <key>CFBundleDisplayName</key><string>NøteCraft</string>
  <key>CFBundleIdentifier</key><string>local.notecraft.app</string>
  <key>CFBundleExecutable</key><string>NoteCraft</string>
  <key>CFBundleIconFile</key><string>NoteCraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSMicrophoneUsageDescription</key><string>NoteCraft records the microphone only when you start a meeting. The audio is transcribed on this Mac and is not kept.</string>
  <key>NSAudioCaptureUsageDescription</key><string>NoteCraft records what this Mac plays, for example the other people in a video call, only when you choose it in a meeting block. The audio is transcribed on this Mac and is not kept.</string>
  <key>NSHumanReadableCopyright</key><string>Copyright © 2026 Yanick Imhof. Free for non-commercial use (PolyForm Noncommercial 1.0.0).</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleLocalizations</key><array><string>en</string><string>fr</string></array>
</dict>
</plist>
PLIST
# text shown under the version in the standard "About NoteCraft" panel
cat > "$APP/Contents/Resources/Credits.rtf" <<'RTF'
{\rtf1\ansi\ansicpg1252\cocoartf2700
{\fonttbl\f0\fswiss\fcharset0 Helvetica;}
\pard\qc\f0\fs22 Notes, databases and private pages, kept as plain files in a folder you choose.\
No account, no tracking.\
\
\fs20 An independent personal project, built with Claude.\
\
{\field{\*\fldinst HYPERLINK "https://github.com/yanickimhof/notecraft"}{\fldrslt github.com/yanickimhof/notecraft}}
}
RTF
# A stable local signature keeps the Keychain permission across rebuilds. Without the "NoteCraft Local"
# certificate the app stays ad-hoc signed and macOS asks for Keychain access after every rebuild.
if security find-identity -v -p codesigning 2>/dev/null | grep -q "NoteCraft Local"; then
  codesign --force --deep --sign "NoteCraft Local" --identifier local.notecraft.app "$APP"
  echo "Signed with NoteCraft Local"
fi

echo "Built $APP"
