#!/bin/sh
# Launched from KUAL. KindleWallet stops Amazon's interface while it runs, so it is
# detached from KUAL (which that interface hosts) and logs to the user partition.
cd "$(dirname "$0")"
# Wallets made before the rename (as ZKindle) move to the new folder once.
if [ -d /mnt/us/zkindle ] && [ ! -e /mnt/us/kindlewallet ]; then
    mv /mnt/us/zkindle /mnt/us/kindlewallet
fi
mkdir -p /mnt/us/kindlewallet
nohup ./bin/kindlewallet > /mnt/us/kindlewallet/log.txt 2>&1 &
