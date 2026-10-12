#!/bin/sh
# Builds the Kindle binary (static, ARMv7 hard-float) and renders the desktop preview
# of every screen. kindle/ mirrors the Kindle's own storage: copy its two folders to
# the root of a jailbroken Kindle, then open "KindleWallet" from the library.
set -e
cd "$(dirname "$0")"
cargo zigbuild --release --target armv7-unknown-linux-musleabihf
mkdir -p kindle/kindlewallet/bin
cp target/armv7-unknown-linux-musleabihf/release/kindlewallet kindle/kindlewallet/bin/kindlewallet
rm -f preview/*.png
cargo run --release -q -- --preview preview
echo "Copy kindle/kindlewallet and kindle/documents to the Kindle's root."
