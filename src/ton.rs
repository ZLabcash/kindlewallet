//! TON from the BIP 39 seed, the way Trust Wallet (wallet-core) and the other BIP 39
//! wallets do it: the SLIP-10 ed25519 key at m/44'/607'/i' owns a standard wallet v4r2
//! contract (wallet id 698983191, the one Ledger's TON app and Tonkeeper show for a
//! Ledger key), whose address is the hash of its initial state: code and data cells.
//! Written user-friendly, non-bounceable, mainnet (UQ…), as wallets show it now.

use base64::Engine;
use sha2::{Digest, Sha256};

/// The wallet v4r2 code cell: representation hash and depth (wallet-core's
/// wallet_v4r2.code, the code every v4r2 wallet on chain runs).
const V4R2_CODE_HASH: [u8; 32] = [
    0xfe, 0xb5, 0xff, 0x68, 0x20, 0xe2, 0xff, 0x0d, 0x94, 0x83, 0xe7, 0xe0, 0xd6, 0x2c, 0x81, 0x7d,
    0x84, 0x67, 0x89, 0xfb, 0x4a, 0xe5, 0x80, 0xc8, 0x78, 0x86, 0x6d, 0x95, 0x9d, 0xab, 0xd5, 0xc0,
];
const V4R2_CODE_DEPTH: u16 = 7;
const WALLET_ID: u32 = 698_983_191;

/// A cell's representation hash: its two descriptor bytes, its bits (padded with a
/// 1 then 0s to a whole byte), then each reference's depth and hash.
fn cell_hash(bits: &[u8], bit_len: usize, refs: &[(u16, [u8; 32])]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([refs.len() as u8, (bit_len / 8 + bit_len.div_ceil(8)) as u8]);
    let mut data = bits[..bit_len.div_ceil(8)].to_vec();
    if !bit_len.is_multiple_of(8) {
        let last = data.len() - 1;
        data[last] |= 0x80 >> (bit_len % 8);
    }
    h.update(&data);
    for (depth, _) in refs {
        h.update(depth.to_be_bytes());
    }
    for (_, hash) in refs {
        h.update(hash);
    }
    h.finalize().into()
}

/// The account id (workchain 0) of the v4r2 wallet for this ed25519 public key.
pub fn v4r2_account(public: &[u8; 32]) -> [u8; 32] {
    // Data: seqno 0 (32 bits), wallet id (32), public key (256), no plugins (1 bit 0).
    let mut data = [0u8; 41];
    data[4..8].copy_from_slice(&WALLET_ID.to_be_bytes());
    data[8..40].copy_from_slice(public);
    let data_hash = cell_hash(&data, 321, &[]);
    // StateInit: no split depth, not special, code, data, no library: bits 00110.
    cell_hash(&[0b0011_0000], 5, &[(V4R2_CODE_DEPTH, V4R2_CODE_HASH), (0, data_hash)])
}

/// CRC-16/XMODEM (also Stellar's StrKey checksum).
pub(crate) fn crc16(data: &[u8]) -> u16 {
    data.iter().fold(0u16, |mut crc, &b| {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { crc << 1 ^ 0x1021 } else { crc << 1 };
        }
        crc
    })
}

/// User-friendly form: flag (0x51 non-bounceable), workchain, account, CRC16-XMODEM,
/// in URL-safe base64.
pub fn friendly(account: &[u8; 32]) -> String {
    let mut data = vec![0x51, 0x00];
    data.extend_from_slice(account);
    let check = crc16(&data);
    data.extend_from_slice(&check.to_be_bytes());
    base64::engine::general_purpose::URL_SAFE.encode(data)
}
