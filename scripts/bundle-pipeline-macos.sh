#!/bin/sh
# Separate native inspector demo, backed by the disposable server on 8184.
set -eu
cd "$(dirname "$0")/.."
./scripts/cargo-macos.sh build
app='target/Shrine Goo Pipeline.app'
mkdir -p "$app/Contents/MacOS"
cp target/debug/shrine-egui "$app/Contents/MacOS/shrine-surfaces.next"
mv -f "$app/Contents/MacOS/shrine-surfaces.next" "$app/Contents/MacOS/shrine-surfaces"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleExecutable</key><string>shrine-surfaces</string><key>CFBundleIdentifier</key><string>dev.shrine.pipeline-egui</string><key>CFBundleName</key><string>Shrine Goo Pipeline</string><key>CFBundleVersion</key><string>1</string><key>CFBundlePackageType</key><string>APPL</string><key>NSHighResolutionCapable</key><true/><key>LSEnvironment</key><dict><key>GOO_SERVER</key><string>http://127.0.0.1:8184</string><key>GOO_PATH</key><string>/0x11/app/ios_demo</string></dict></dict></plist>
PLIST
/usr/bin/codesign --force --sign - "$app"
