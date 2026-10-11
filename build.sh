#!/bin/sh
# Builds the Kindle binary (static, ARMv7 hard-float) into the KUAL extension and
# renders the desktop preview of every screen.
set -e
cd "$(dirname "$0")"
cargo zigbuild --release --target armv7-unknown-linux-musleabihf
cp target/armv7-unknown-linux-musleabihf/release/kindlewallet kual/kindlewallet/bin/kindlewallet
cargo run --release -q -- --preview preview
echo "Copy kual/kindlewallet to the Kindle's extensions/ folder."
