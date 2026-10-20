#!/usr/bin/env bash
# Regenerates the app's logo assets from the originals in logos/.
# Requires ImageMagick (`magick`) and, for the macOS icon, `iconutil` (macOS).
set -euo pipefail
cd "$(dirname "$0")/.."

SRC_SQUARE=logos/logo_waves_square.png        # "WAVES" text on the left, wave on the right
SRC_WORDMARK=logos/logo_waves_rectangle.png   # wave + "WAVES / Wide Area Vista Extragalactic Survey"
OUT=crates/surf-gui/assets
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# PNG32: forces RGBA output; otherwise a white-only canvas is saved as
# grayscale and drags the composited icon to grayscale with it.

# The wave alone: the square logo minus the vertical "WAVES" text (x < ~360).
magick "$SRC_SQUARE" -crop 1417x1417+430+0 +repage -trim +repage "$TMP/wave.png"

# Small wave for the header (2x for HiDPI).
magick "$TMP/wave.png" -resize 128x128 -background none -gravity center -extent 128x128 "$OUT/wave.png"

# Wordmark for the dark WAVES theme: black text recoloured to the logo's foam.
magick "$SRC_WORDMARK" -resize 1200x -fuzz 25% -fill '#DEECEF' -opaque black "$OUT/wordmark.png"

# App icon: the wave on a white rounded square, in macOS icon proportions
# (824px tile with ~185px corners on a 1024px canvas).
magick -size 1024x1024 xc:none -fill white -draw "roundrectangle 100,100 923,923 185,185" "PNG32:$TMP/tile.png"
magick "$TMP/wave.png" -resize 620x620 "$TMP/wave620.png"
magick "$TMP/tile.png" "$TMP/wave620.png" -gravity center -geometry +12+0 -composite "PNG32:$TMP/icon1024.png"
magick "$TMP/icon1024.png" -resize 512x512 "PNG32:$OUT/icon.png"   # window icon (Linux), dock icon when not bundled (macOS)

if command -v iconutil >/dev/null; then
  SET="$TMP/Surf.iconset"
  mkdir -p "$SET"
  for s in 16 32 128 256 512; do
    magick "$TMP/icon1024.png" -resize ${s}x${s} "PNG32:$SET/icon_${s}x${s}.png"
    magick "$TMP/icon1024.png" -resize $((s * 2))x$((s * 2)) "PNG32:$SET/icon_${s}x${s}@2x.png"
  done
  iconutil -c icns "$SET" -o packaging/macos/Surf.icns
fi
echo "Assets written to $OUT and packaging/macos/Surf.icns"
