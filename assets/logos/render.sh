#!/bin/sh
# Each official monochrome SVG → a 112 px grayscale PNG for the e-ink screen: margins
# trimmed so every mark fills the same 100 px box (black on white).
# Needs resvg (cargo install resvg) and ImageMagick.
cd "$(dirname "$0")"
tmp=$(mktemp -d)
for f in *.svg; do
    resvg -w 1000 "$f" "$tmp/big.png"
    magick "$tmp/big.png" -alpha extract -negate -trim +repage \
        -resize 100x100 -background white -gravity center -extent 112x112 \
        -colorspace Gray -depth 8 "${f%.svg}.png"
done
rm -r "$tmp"
