//! KindleWallet: an offline wallet for a jailbroken Kindle.
//!
//! One BIP 39 phrase, made on the device from its own randomness, gives:
//! - Bitcoin native SegWit (BIP 84);
//! - Ethereum, BNB Chain, Robinhood Chain, Base and Arbitrum (one BIP 44 address);
//! - Zcash shielded (Orchard, ZIP 32), with the code the ZFlip Flipper wallet uses;
//! - Solana (SLIP-10 ed25519);
//! - Monero (Trezor's derivation, so its 25 Monero words restore it anywhere);
//! - Quantus (post-quantum ML-DSA-87, as its official wallet derives it).
//!
//! The phrase is sealed on the Kindle with a PIN (Argon2id + AES-256-GCM). The
//! Kindle stays offline; the phone app talks to it on local Wi-Fi only.

pub mod app;
pub mod bip32;
pub mod coins;
pub mod logos;
pub mod monero;
pub mod render;
pub mod slip10;
pub mod vault;

pub use zfirm_core::mnemonic;

/// A fresh 24-word phrase from the system's randomness.
pub fn new_phrase() -> String {
    let mut entropy = [0u8; 32];
    getrandom::getrandom(&mut entropy).expect("system randomness");
    let indices = mnemonic::indices_from_entropy(&entropy);
    zeroize::Zeroize::zeroize(&mut entropy);
    mnemonic::sentence(&indices, &english()).expect("valid indices")
}

pub fn english() -> mnemonic::Packed<'static> {
    mnemonic::Packed::new(mnemonic::ENGLISH).expect("embedded list")
}

/// Checks the words and their checksum; returns the normalised phrase.
pub fn check_phrase(words: &str) -> Option<String> {
    let list = english();
    let indices = mnemonic::parse(words, &list).ok()?;
    mnemonic::validate(&indices).ok()?;
    mnemonic::sentence(&indices, &list)
}

/// The BIP 39 seed (no passphrase).
pub fn seed(phrase: &str) -> [u8; 64] {
    mnemonic::to_seed(phrase, "", |_| {})
}
