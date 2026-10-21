#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
./scripts/cargo-macos.sh build
mkdir -p 'target/Shrine Goo Surfaces.app/Contents/MacOS'
cp target/debug/shrine-egui 'target/Shrine Goo Surfaces.app/Contents/MacOS/shrine-surfaces.next'
mv -f 'target/Shrine Goo Surfaces.app/Contents/MacOS/shrine-surfaces.next' 'target/Shrine Goo Surfaces.app/Contents/MacOS/shrine-surfaces'
cat > 'target/Shrine Goo Surfaces.app/Contents/Info.plist' <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleExecutable</key><string>shrine-surfaces</string><key>CFBundleIdentifier</key><string>dev.shrine.surfaces-egui</string><key>CFBundleName</key><string>Shrine Goo Surfaces</string><key>CFBundleVersion</key><string>1</string><key>CFBundlePackageType</key><string>APPL</string><key>NSHighResolutionCapable</key><true/></dict></plist>
PLIST
/usr/bin/codesign --force --sign - 'target/Shrine Goo Surfaces.app'
