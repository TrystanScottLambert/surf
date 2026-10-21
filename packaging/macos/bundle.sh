#!/usr/bin/env bash
# Builds dist/Surf.app from one or more compiled `surf` binaries (several are
# merged into a universal binary) and zips it.
#
#   cargo build --release -p surf-gui
#   packaging/macos/bundle.sh 0.1.0 target/release/surf
#
# Signing is optional and driven by the environment:
#   SURF_SIGN_IDENTITY     "Developer ID Application: Name (TEAMID)" in the
#                          keychain; unset means an ad-hoc signature.
#   SURF_NOTARY_APPLE_ID, SURF_NOTARY_TEAM_ID, SURF_NOTARY_PASSWORD
#                          (an app-specific password): when all are set the
#                          zip is notarized and the ticket stapled to the app.
set -euo pipefail
cd "$(dirname "$0")/../.."

# Values pasted into CI secrets often carry a trailing newline.
for var in SURF_SIGN_IDENTITY SURF_NOTARY_APPLE_ID SURF_NOTARY_TEAM_ID SURF_NOTARY_PASSWORD; do
  printf -v "$var" '%s' "$(printf '%s' "${!var:-}" | tr -d '\r\n')"
done

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

ZIP=dist/Surf-macos-universal.zip
if [ -n "${SURF_SIGN_IDENTITY:-}" ]; then
  # Notarization requires the hardened runtime and a secure timestamp.
  codesign --force --options runtime --timestamp --sign "$SURF_SIGN_IDENTITY" "$APP"
  codesign --verify --strict --verbose=2 "$APP"
else
  # Ad-hoc signature (no Apple Developer ID): first launch needs
  # System Settings → Privacy & Security → Open Anyway.
  codesign --force --deep --sign - "$APP"
fi
(cd dist && ditto -c -k --keepParent Surf.app Surf-macos-universal.zip)

if [ -n "${SURF_SIGN_IDENTITY:-}" ] && [ -n "${SURF_NOTARY_APPLE_ID:-}" ] \
   && [ -n "${SURF_NOTARY_TEAM_ID:-}" ] && [ -n "${SURF_NOTARY_PASSWORD:-}" ]; then
  xcrun notarytool submit "$ZIP" --wait \
    --apple-id "$SURF_NOTARY_APPLE_ID" \
    --team-id "$SURF_NOTARY_TEAM_ID" \
    --password "$SURF_NOTARY_PASSWORD"
  xcrun stapler staple "$APP"
  # Re-zip so the shipped app carries the stapled ticket.
  rm "$ZIP"
  (cd dist && ditto -c -k --keepParent Surf.app Surf-macos-universal.zip)
fi
echo "Built $APP"
