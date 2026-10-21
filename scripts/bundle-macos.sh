#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
./scripts/cargo-macos.sh build
mkdir -p 'target/Shrine Goo.app/Contents/MacOS'
rm -f 'target/Shrine Goo.app/Contents/MacOS/shrine-egui'
cp target/debug/shrine-egui 'target/Shrine Goo.app/Contents/MacOS/shrine-egui'
cat > 'target/Shrine Goo.app/Contents/Info.plist' <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleExecutable</key><string>shrine-egui</string><key>CFBundleIdentifier</key><string>dev.shrine.goo-egui</string><key>CFBundleName</key><string>Shrine Goo</string><key>CFBundleVersion</key><string>1</string><key>CFBundlePackageType</key><string>APPL</string><key>NSHighResolutionCapable</key><true/></dict></plist>
PLIST

/usr/bin/codesign --force --sign - "target/Shrine Goo.app"
