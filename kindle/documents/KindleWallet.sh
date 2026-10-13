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
    # The last run'"'"'s log stays, for when something went wrong.
    [ -e log.txt ] && mv -f log.txt log.prev.txt
    ./bin/kindlewallet > log.txt 2>&1
    echo "exit $?" >> log.txt
    # After a crash with the phone link on: the firewall as it was, Wi-Fi off.
    if [ -e /tmp/kindlewallet.iptables ] || [ -e /tmp/kindlewallet.ip6tables ]; then
        [ -e /tmp/kindlewallet.iptables ] && iptables-restore < /tmp/kindlewallet.iptables && rm -f /tmp/kindlewallet.iptables
        [ -e /tmp/kindlewallet.ip6tables ] && ip6tables-restore < /tmp/kindlewallet.ip6tables && rm -f /tmp/kindlewallet.ip6tables
        lipc-set-prop com.lab126.cmd wirelessEnable 0
    fi
    if [ -e /tmp/kindlewallet.ipv6 ]; then
        while read -r name value; do echo "$value" > "/proc/sys/net/ipv6/conf/$name/disable_ipv6"; done < /tmp/kindlewallet.ipv6
        rm -f /tmp/kindlewallet.ipv6
    fi
    lipc-set-prop com.lab126.powerd preventScreenSaver 0
    if ! status lab126_gui | grep -q running; then
        eips -c
        start lab126_gui
    fi
' > /dev/null 2>&1 < /dev/null &
