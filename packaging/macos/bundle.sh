#!/usr/bin/env bash
# Builds dist/Surf.app from one or more compiled `surf` binaries (several are
# merged into a universal binary) and zips it.
#
#   cargo build --release -p surf-gui
#   packaging/macos/bundle.sh 0.1.0 target/release/surf
set -euo pipefail
cd "$(dirname "$0")/../.."

VERSION="$1"; shift
APP=dist/Surf.app
rm -rf "$APP" dist/Surf-macos-universal.zip
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

if [ "$#" -gt 1 ]; then
  lipo -create -output "$APP/Contents/MacOS/surf" "$@"
else
  cp "$1" "$APP/Contents/MacOS/surf"
fi
sed "s/__VERSION__/${VERSION}/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
cp packaging/macos/Surf.icns "$APP/Contents/Resources/Surf.icns"

# Ad-hoc signature (no Apple Developer ID): first launch needs
# System Settings → Privacy & Security → Open Anyway.
codesign --force --deep --sign - "$APP"
(cd dist && ditto -c -k --keepParent Surf.app Surf-macos-universal.zip)
echo "Built $APP"
