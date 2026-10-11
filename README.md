# KindleWallet

A hardware wallet made from a jailbroken Kindle: offline, multi-coin, with its own e-ink screen. one 24-word phrase made on the
device gives Zcash shielded (Orchard), Bitcoin (BIP 84) and Ethereum addresses, with
QR codes on the e-ink screen. Sealed with a PIN (Argon2id + AES-256-GCM).

Needs the [zfirm](https://github.com/ZLabcash/zfirm) repo next to this one as `zfirm/` (shared Zcash core).
`./build.sh` makes the Kindle binary into `kual/kindlewallet` and renders `preview/`.
See PLAN.md.
