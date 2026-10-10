//! Receiving addresses for each coin, from the BIP 39 seed.

use bech32::{Hrp, segwit};
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use sha3::Keccak256;
use zflip_core::Network;

use crate::bip32::{ExtendedKey, HARDENED};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coin {
    Zcash,
    Bitcoin,
    Ethereum,
}

impl Coin {
    pub const ALL: [Coin; 3] = [Coin::Zcash, Coin::Bitcoin, Coin::Ethereum];

    pub fn name(self) -> &'static str {
        match self {
            Coin::Zcash => "Zcash (shielded)",
            Coin::Bitcoin => "Bitcoin",
            Coin::Ethereum => "Ethereum",
        }
    }

    pub fn ticker(self) -> &'static str {
        match self {
            Coin::Zcash => "ZEC",
            Coin::Bitcoin => "BTC",
            Coin::Ethereum => "ETH",
        }
    }

    /// Receiving address number `index` of account 0.
    pub fn address(self, seed: &[u8; 64], index: u32) -> Option<String> {
        match self {
            Coin::Zcash => zcash(seed, index),
            Coin::Bitcoin => bitcoin(seed, index),
            Coin::Ethereum => ethereum(seed, index),
        }
    }
}

/// Orchard unified address j of ZIP 32 account 0 (diversified: unlinkable).
pub fn zcash(seed: &[u8; 64], j: u32) -> Option<String> {
    let account = zflip_core::account::Account::from_seed(seed, Network::Main, 0).ok()?;
    Some(account.encode_address(j))
}

/// The Zcash viewing key, for the phone app (it can see, never spend).
pub fn zcash_ufvk(seed: &[u8; 64]) -> Option<String> {
    Some(zflip_core::account::Account::from_seed(seed, Network::Main, 0).ok()?.ufvk())
}

/// BIP 84: m/84'/0'/0'/0/i, P2WPKH (bc1q…).
pub fn bitcoin(seed: &[u8; 64], i: u32) -> Option<String> {
    let key = ExtendedKey::derive(seed, &[84 | HARDENED, HARDENED, HARDENED, 0, i])?;
    let hash = Ripemd160::digest(Sha256::digest(key.public(true).as_bytes()));
    segwit::encode(Hrp::parse("bc").ok()?, segwit::VERSION_0, &hash).ok()
}

/// BIP 44: m/44'/60'/0'/0/i, the address with its EIP-55 checksum.
pub fn ethereum(seed: &[u8; 64], i: u32) -> Option<String> {
    let key = ExtendedKey::derive(seed, &[44 | HARDENED, 60 | HARDENED, HARDENED, 0, i])?;
    let point = key.public(false);
    let hash = Keccak256::digest(&point.as_bytes()[1..]);
    let hex: String = hash[12..].iter().map(|b| format!("{b:02x}")).collect();
    let check = Keccak256::digest(hex.as_bytes());
    let mut out = String::from("0x");
    for (n, c) in hex.chars().enumerate() {
        let nibble = (check[n / 2] >> if n % 2 == 0 { 4 } else { 0 }) & 0xF;
        out.push(if c.is_ascii_alphabetic() && nibble >= 8 { c.to_ascii_uppercase() } else { c });
    }
    Some(out)
}
