#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
./scripts/cargo-macos.sh build
mkdir -p 'target/Shrine Explorer.app/Contents/MacOS'
cp target/debug/shrine-egui 'target/Shrine Explorer.app/Contents/MacOS/shrine-explorer'
cat > 'target/Shrine Explorer.app/Contents/Info.plist' <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleExecutable</key><string>shrine-explorer</string><key>CFBundleIdentifier</key><string>dev.shrine.explorer-egui</string><key>CFBundleName</key><string>Shrine Explorer</string><key>CFBundleVersion</key><string>1</string><key>CFBundlePackageType</key><string>APPL</string><key>NSHighResolutionCapable</key><true/></dict></plist>
PLIST
/usr/bin/codesign --force --sign - 'target/Shrine Explorer.app'
