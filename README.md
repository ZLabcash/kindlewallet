# ZKindle

An offline multi-coin wallet for a jailbroken Kindle: one 24-word phrase made on the
device gives Zcash shielded (Orchard), Bitcoin (BIP 84) and Ethereum addresses, with
QR codes on the e-ink screen. Sealed with a PIN (Argon2id + AES-256-GCM).

Needs the [zfirm](https://github.com/ZLabcash/zfirm) repo next to this one as `zfirm/` (shared Zcash core).
`./build.sh` makes the Kindle binary into `kual/zkindle` and renders `preview/`.
See PLAN.md.
