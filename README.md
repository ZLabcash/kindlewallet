# KindleWallet

A hardware wallet made from a jailbroken Kindle: offline, multi-coin, with its own e-ink
screen. One 24-word phrase made on the device gives twelve coins, each on its own screen
with a QR code:

| Coin | Derivation | Address |
|---|---|---|
| Bitcoin | BIP 84, m/84'/0'/0'/0/i (+ zpub watch key) | bc1q… |
| Ethereum, BNB Chain, Robinhood Chain, Base, Arbitrum | BIP 44, m/44'/60'/0'/0/i | one 0x… address |
| Zcash | ZIP 32 Orchard, shielded only (+ viewing key) | u1… |
| Solana | SLIP-10 ed25519, m/44'/501'/i'/0' | base58 |
| Monero | Trezor's: SLIP-10 m/44'/128'/0', subaddresses (+ view key, 25 Monero words) | 4… / 8… |
| Quantus | official wallet's: "Dilithium seed" tree m/44'/189189'/i'/0'/0', ML-DSA-87, Poseidon2, SS58 189 | qz… |

XRP (BIP 44 m/44'/144'/0'/0/i, r…) and Tron (BIP 44 m/44'/195'/0'/0/i, T…) complete
the twelve, on a home screen of logos three to a row: BTC ETH XRP / ZEC BNB SOL /
XMR TRX BASE / ARB HOOD QTC.

Each coin's whitepaper is readable on its screen: `papers/build.py` downloads them from
the projects' own sites and lays them out again for the 600 × 800 screen (PDFs
reflowed word by word, the way k2pdfopt does; Markdown and HTML set in Literata), into
`kindle/kindlewallet/papers/<TICKER>/NNN.png`. Base and Robinhood Chain publish none.

## Phone link (Zcash only)

On the Zcash screen, "Connect phone" switches the Kindle's Wi-Fi on behind a firewall
(in: only TCP 7700 from the local network; out: only the local network; IPv6 shut)
and shows a pairing QR for ZApp. The phone holds the viewing key, builds and proves
transactions; the Kindle checks each one, shows every payment, memo and the fee, and
signs only when a finger stays on "Hold to sign" for 1.5 s. Inside an encrypted
channel (X25519, HKDF-SHA256, ChaCha20-Poly1305) runs the same zfirm protocol the
Flipper speaks over Bluetooth. A paired phone reconnects without the QR; "Forget
phones" undoes that. Leaving Zcash, locking, sleeping or exiting puts the firewall
back and switches Wi-Fi off. Fixed-key vectors for the channel are in
`tests/link_vectors.json`. The other coins stay receive-only.

To try ZApp against a computer: `cargo run --release -- --serve DIR [--port 7700]
[--bind IP] [--pin PIN] [--auto-approve]` makes a test wallet in DIR (the BIP 39 test
phrase "abandon ×23 art", never a real one), prints the pairing QR's text, saves the
screen as `DIR/screen.png` and asks on the terminal before pairing or signing.

## zkSNARKs

The ZEC screen's "zkSNARKs" opens the whole collection offline: 10,000 portraits of
26 × 26 pixels, in gray, a grid of 30 per page, and "Mine" for those the phone says
your zilkroad account holds. `uv run --with pillow nft/build.py CACHE_DIR
kindle/kindlewallet` downloads them (api.zscan.cash, traits from zilkroad) and packs
them into `kindle/kindlewallet/nft/` (not kept here, like the whitepapers).

Each derivation is checked against its published vectors (`cargo test`). Sealed with a
PIN (Argon2id + AES-256-GCM). Logos: the coins' official monochrome marks, in
`assets/logos`.

Needs the [zfirm](https://github.com/ZLabcash/zfirm) repo next to this one as `zfirm/`
(shared Zcash core). `./build.sh` makes the Kindle binary into `kindle/` and renders
`preview/`. Copy `kindle/kindlewallet` and `kindle/documents` to the root of the Kindle,
then open "KindleWallet" from the library. See PLAN.md.

## Install

1. Jailbreak the Kindle (a Kindle 10th generation on 5.18.1 used
   [SpiderCat](https://kindlemodding.org/jailbreaking/SpiderCat/): a book to open, no ads needed).
2. `./build.sh`, then copy `kindle/kindlewallet` and `kindle/documents` to the root of the
   Kindle's storage (and `kindle/kindlewallet/papers` after `papers/build.py`,
   `kindle/kindlewallet/nft` after `nft/build.py`).
3. Open "KindleWallet" from the library. Exit gives the Kindle back; the power button
   locks the wallet.

Experimental and not audited: try it with small amounts.

## Licence

MIT or Apache-2.0, at your option. The coin logos in `assets/logos` are their projects'
trademarks, used to name the coins (from Simple Icons, web3icons, Base's brand kit and
quantus.com); the whitepapers belong to their authors and are not kept here.
