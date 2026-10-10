#!/bin/sh
# Launched from KUAL. ZKindle stops Amazon's interface while it runs, so it is
# detached from KUAL (which that interface hosts) and logs to the user partition.
cd "$(dirname "$0")"
mkdir -p /mnt/us/zkindle
nohup ./bin/zkindle > /mnt/us/zkindle/log.txt 2>&1 &
