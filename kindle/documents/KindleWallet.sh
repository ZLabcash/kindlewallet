#!/bin/sh
# Name: KindleWallet
# Author: ZLab
# DontUseFBInk
# Opened from the library like a book. KindleWallet stops Amazon's interface while it
# runs, so it is detached from it and logs to /mnt/us/kindlewallet/log.txt. However it
# ends, even by crashing, Amazon's interface comes back.
D=/mnt/us/kindlewallet
# Wallets made before the rename (as ZKindle) move to the new folder once.
if [ -d /mnt/us/zkindle ] && [ ! -e "$D/wallet.zkv" ] && [ -e /mnt/us/zkindle/wallet.zkv ]; then
    mv /mnt/us/zkindle/wallet.zkv "$D/wallet.zkv"
fi
cd "$D" || exit 1
nohup setsid sh -c '
    ./bin/kindlewallet > log.txt 2>&1
    echo "exit $?" >> log.txt
    lipc-set-prop com.lab126.powerd preventScreenSaver 0
    if ! status lab126_gui | grep -q running; then
        eips -c
        start lab126_gui
    fi
' > /dev/null 2>&1 < /dev/null &
