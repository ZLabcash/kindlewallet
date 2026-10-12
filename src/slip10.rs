//! SLIP-10 on ed25519: hardened-only derivation of 32-byte private keys
//! (Solana, and the Monero key seed the way Trezor makes it). Quantus runs the
//! same tree under its own key, "Dilithium seed".

use hmac::{Hmac, Mac};
use sha2::Sha512;
use zeroize::Zeroizing;

use crate::bip32::HARDENED;

fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 64] {
    let mut m = Hmac::<Sha512>::new_from_slice(key).expect("any key length");
    for p in parts {
        m.update(p);
    }
    m.finalize().into_bytes().into()
}

/// The private key at `path`; every step is hardened (the flag is added if missing).
pub fn derive(seed: &[u8], path: &[u32]) -> [u8; 32] {
    derive_with(b"ed25519 seed", seed, path)
}

/// The same tree from another master key string.
pub fn derive_with(curve: &[u8], seed: &[u8], path: &[u32]) -> [u8; 32] {
    let mut i = Zeroizing::new(hmac(curve, &[seed]));
    for &index in path {
        let index = index | HARDENED;
        *i = hmac(&i[32..], &[&[0], &i[..32], &index.to_be_bytes()]);
    }
    i[..32].try_into().expect("32 bytes")
}
