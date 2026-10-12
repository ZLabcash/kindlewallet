//! Receiving addresses for each coin, from the BIP 39 seed. Every derivation follows
//! the coin's usual wallets, so the same 24 words restore the same funds there.

use bech32::{Hrp, segwit};
use blake2::Blake2b512;
use ml_dsa::{Keypair, MlDsa87, SigningKey};
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use sha3::Keccak256;
use zeroize::Zeroizing;
use zfirm_core::Network;

use crate::bip32::{ExtendedKey, HARDENED};
use crate::{monero, slip10};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coin {
    Bitcoin,
    Ethereum,
    Xrp,
    Zcash,
    Bnb,
    Solana,
    Monero,
    Tron,
    Base,
    Arbitrum,
    Robinhood,
    Quantus,
}

impl Coin {
    /// In the order of the home grid.
    pub const ALL: [Coin; 12] = [
        Coin::Bitcoin,
        Coin::Ethereum,
        Coin::Xrp,
        Coin::Zcash,
        Coin::Bnb,
        Coin::Solana,
        Coin::Monero,
        Coin::Tron,
        Coin::Base,
        Coin::Arbitrum,
        Coin::Robinhood,
        Coin::Quantus,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Coin::Bitcoin => "Bitcoin",
            Coin::Ethereum => "Ethereum",
            Coin::Xrp => "XRP Ledger",
            Coin::Zcash => "Zcash",
            Coin::Bnb => "BNB Chain",
            Coin::Solana => "Solana",
            Coin::Monero => "Monero",
            Coin::Tron => "Tron",
            Coin::Robinhood => "Robinhood Chain",
            Coin::Base => "Base",
            Coin::Arbitrum => "Arbitrum",
            Coin::Quantus => "Quantus",
        }
    }

    /// Short name; also names the coin's address counter file.
    pub fn ticker(self) -> &'static str {
        match self {
            Coin::Bitcoin => "BTC",
            Coin::Ethereum => "ETH",
            Coin::Xrp => "XRP",
            Coin::Zcash => "ZEC",
            Coin::Bnb => "BNB",
            Coin::Solana => "SOL",
            Coin::Monero => "XMR",
            Coin::Tron => "TRX",
            Coin::Robinhood => "HOOD",
            Coin::Base => "BASE",
            Coin::Arbitrum => "ARB",
            Coin::Quantus => "QTC",
        }
    }

    /// The EVM chain id, for the networks that share Ethereum's address.
    pub fn chain_id(self) -> Option<u64> {
        match self {
            Coin::Ethereum => Some(1),
            Coin::Bnb => Some(56),
            Coin::Robinhood => Some(4663),
            Coin::Base => Some(8453),
            Coin::Arbitrum => Some(42161),
            _ => None,
        }
    }

    /// Receiving address number `index` of account 0.
    pub fn address(self, seed: &[u8; 64], index: u32) -> Option<String> {
        if self.chain_id().is_some() {
            return ethereum(seed, index);
        }
        match self {
            Coin::Zcash => zcash(seed, index),
            Coin::Bitcoin => bitcoin(seed, index),
            Coin::Xrp => xrp(seed, index),
            Coin::Tron => tron(seed, index),
            Coin::Solana => solana(seed, index),
            Coin::Monero => Some(monero::Keys::from_seed(seed).address(index)),
            Coin::Quantus => quantus(seed, index),
            _ => None,
        }
    }
}

/// Orchard unified address j of ZIP 32 account 0 (diversified: unlinkable).
pub fn zcash(seed: &[u8; 64], j: u32) -> Option<String> {
    let account = zfirm_core::account::Account::from_seed(seed, Network::Main, 0).ok()?;
    Some(account.encode_address(j))
}

/// The Zcash viewing key, for the phone app (it can see, never spend).
pub fn zcash_ufvk(seed: &[u8; 64]) -> Option<String> {
    Some(zfirm_core::account::Account::from_seed(seed, Network::Main, 0).ok()?.ufvk())
}

fn hash160(data: &[u8]) -> [u8; 20] {
    Ripemd160::digest(Sha256::digest(data)).into()
}

/// BIP 84: m/84'/0'/0'/0/i, P2WPKH (bc1q…).
pub fn bitcoin(seed: &[u8; 64], i: u32) -> Option<String> {
    let key = ExtendedKey::derive(seed, &[84 | HARDENED, HARDENED, HARDENED, 0, i])?;
    let hash = hash160(key.public(true).as_bytes());
    segwit::encode(Hrp::parse("bc").ok()?, segwit::VERSION_0, &hash).ok()
}

/// The BIP 84 account key (zpub), for a watch-only Bitcoin wallet.
pub fn bitcoin_zpub(seed: &[u8; 64]) -> Option<String> {
    let parent = ExtendedKey::derive(seed, &[84 | HARDENED, HARDENED])?;
    let account = parent.child(HARDENED)?;
    let mut data = vec![0x04, 0xb2, 0x47, 0x46, 3];
    data.extend_from_slice(&hash160(parent.public(true).as_bytes())[..4]);
    data.extend_from_slice(&HARDENED.to_be_bytes());
    data.extend_from_slice(&account.chain);
    data.extend_from_slice(account.public(true).as_bytes());
    let check = Sha256::digest(Sha256::digest(&data));
    data.extend_from_slice(&check[..4]);
    Some(bs58::encode(data).into_string())
}

/// BIP 44: m/44'/60'/0'/0/i, the address with its EIP-55 checksum. Ethereum, BNB
/// Chain, Base, Arbitrum and Robinhood Chain all use this one address.
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

/// BIP 44: m/44'/144'/0'/0/i (Xaman, Ledger, Trezor), the classic r… address.
pub fn xrp(seed: &[u8; 64], i: u32) -> Option<String> {
    let key = ExtendedKey::derive(seed, &[44 | HARDENED, 144 | HARDENED, HARDENED, 0, i])?;
    let mut data = vec![0];
    data.extend_from_slice(&hash160(key.public(true).as_bytes()));
    let check = Sha256::digest(Sha256::digest(&data));
    data.extend_from_slice(&check[..4]);
    Some(bs58::encode(data).with_alphabet(bs58::Alphabet::RIPPLE).into_string())
}

/// BIP 44: m/44'/195'/0'/0/i (TronLink, Ledger): Ethereum's 20-byte key hash behind
/// 0x41, in base58check (T…).
pub fn tron(seed: &[u8; 64], i: u32) -> Option<String> {
    let key = ExtendedKey::derive(seed, &[44 | HARDENED, 195 | HARDENED, HARDENED, 0, i])?;
    let hash = Keccak256::digest(&key.public(false).as_bytes()[1..]);
    let mut data = vec![0x41];
    data.extend_from_slice(&hash[12..]);
    let check = Sha256::digest(Sha256::digest(&data));
    data.extend_from_slice(&check[..4]);
    Some(bs58::encode(data).into_string())
}

/// m/44'/501'/i'/0' (Phantom, Solflare, Trezor): the ed25519 public key in base58.
pub fn solana(seed: &[u8; 64], i: u32) -> Option<String> {
    let secret = Zeroizing::new(slip10::derive(seed, &[44, 501, i, 0]));
    let key = ed25519_dalek::SigningKey::from_bytes(&secret);
    Some(bs58::encode(key.verifying_key().as_bytes()).into_string())
}

/// Quantus, as its official wallet makes it: an HD tree keyed "Dilithium seed"
/// (SLIP-10 shaped, hardened only) to m/44'/189189'/i'/0'/0', whose key seeds an
/// ML-DSA-87 key pair (FIPS 204); the account is the Poseidon2 hash of the public
/// key, written in SS58 with prefix 189 (qz…).
pub fn quantus(seed: &[u8; 64], i: u32) -> Option<String> {
    let xi = Zeroizing::new(slip10::derive_with(b"Dilithium seed", seed, &[44, 189189, i, 0, 0]));
    let key = SigningKey::<MlDsa87>::from_seed(&ml_dsa::Seed::try_from(&xi[..]).ok()?);
    let public = key.verifying_key().encode();
    Some(ss58(189, &qp_poseidon_core::hash_bytes(public.as_slice())))
}

/// SS58 with a two-byte prefix (formats 64 to 16383).
fn ss58(format: u16, account: &[u8; 32]) -> String {
    let mut data = vec![((format & 0b1111_1100) >> 2) as u8 | 0b0100_0000, (format >> 8) as u8 | ((format & 0b11) as u8) << 6];
    data.extend_from_slice(account);
    let mut h = Blake2b512::new();
    h.update(b"SS58PRE");
    h.update(&data);
    let check = h.finalize();
    data.extend_from_slice(&check[..2]);
    bs58::encode(data).into_string()
}
