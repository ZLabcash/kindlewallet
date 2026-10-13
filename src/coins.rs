//! Receiving addresses for each coin, from the BIP 39 seed (or, for Cardano and
//! Bittensor, the phrase's entropy). Every derivation follows the coin's usual
//! wallets, so the same 24 words restore the same funds there.

use bech32::{Hrp, segwit};
use blake2::{Blake2b, Blake2b512, digest::consts::U32};
use ml_dsa::{Keypair, MlDsa87, SigningKey};
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use sha3::{Keccak256, Sha3_256};
use zeroize::Zeroizing;
use zfirm_core::Network;

use crate::bip32::{ExtendedKey, HARDENED};
use crate::{cardano, monero, slip10, starknet, ton};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coin {
    Bitcoin,
    /// Ethereum and every EVM network in `evm::NETWORKS` (one address for all).
    Ethereum,
    Zcash,
    Solana,
    Monero,
    Tron,
    Xrp,
    Sui,
    Aptos,
    Near,
    Stellar,
    Ton,
    Cardano,
    Starknet,
    Bittensor,
    Quantus,
}

impl Coin {
    /// In the order of the home grid.
    pub const ALL: [Coin; 16] = [
        Coin::Bitcoin,
        Coin::Ethereum,
        Coin::Zcash,
        Coin::Solana,
        Coin::Monero,
        Coin::Tron,
        Coin::Xrp,
        Coin::Sui,
        Coin::Aptos,
        Coin::Near,
        Coin::Stellar,
        Coin::Ton,
        Coin::Cardano,
        Coin::Starknet,
        Coin::Bittensor,
        Coin::Quantus,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Coin::Bitcoin => "Bitcoin",
            Coin::Ethereum => "Ethereum",
            Coin::Zcash => "Zcash",
            Coin::Solana => "Solana",
            Coin::Monero => "Monero",
            Coin::Tron => "Tron",
            Coin::Xrp => "XRP Ledger",
            Coin::Sui => "Sui",
            Coin::Aptos => "Aptos",
            Coin::Near => "NEAR",
            Coin::Stellar => "Stellar",
            Coin::Ton => "TON",
            Coin::Cardano => "Cardano",
            Coin::Starknet => "Starknet",
            Coin::Bittensor => "Bittensor",
            Coin::Quantus => "Quantus",
        }
    }

    /// Short name; also names the coin's address counter file.
    pub fn ticker(self) -> &'static str {
        match self {
            Coin::Bitcoin => "BTC",
            Coin::Ethereum => "ETH",
            Coin::Zcash => "ZEC",
            Coin::Solana => "SOL",
            Coin::Monero => "XMR",
            Coin::Tron => "TRX",
            Coin::Xrp => "XRP",
            Coin::Sui => "SUI",
            Coin::Aptos => "APT",
            Coin::Near => "NEAR",
            Coin::Stellar => "XLM",
            Coin::Ton => "TON",
            Coin::Cardano => "ADA",
            Coin::Starknet => "STRK",
            Coin::Bittensor => "TAO",
            Coin::Quantus => "QTC",
        }
    }

    /// Ethereum mainnet's chain id for the EVM group; the other networks are in
    /// `evm::NETWORKS`.
    pub fn chain_id(self) -> Option<u64> {
        match self {
            Coin::Ethereum => Some(1),
            _ => None,
        }
    }

    /// Whether the chain has NFTs worth a gallery: Ordinals, ERC-721 on the EVM
    /// networks, Zcash's zkSNARKs inscriptions, Metaplex, TRC-721, XLS-20, and the
    /// native objects/tokens of Sui, Aptos, NEAR, TON, Cardano and Starknet.
    pub fn has_nfts(self) -> bool {
        !matches!(self, Coin::Monero | Coin::Stellar | Coin::Bittensor | Coin::Quantus)
    }

    /// Cardano and Bittensor derive from the phrase's entropy, not from the BIP 39
    /// seed: use `address_from_phrase` for them.
    pub fn needs_phrase(self) -> bool {
        matches!(self, Coin::Cardano | Coin::Bittensor)
    }

    /// Receiving address number `index` of account 0, for the coins that derive from
    /// the BIP 39 seed (None for those that need the phrase).
    pub fn address(self, seed: &[u8; 64], index: u32) -> Option<String> {
        match self {
            Coin::Bitcoin => bitcoin(seed, index),
            Coin::Ethereum => ethereum(seed, index),
            Coin::Zcash => zcash(seed, index),
            Coin::Solana => solana(seed, index),
            Coin::Monero => Some(monero::Keys::from_seed(seed).address(index)),
            Coin::Tron => tron(seed, index),
            Coin::Xrp => xrp(seed, index),
            Coin::Sui => sui(seed, index),
            Coin::Aptos => aptos(seed, index),
            Coin::Near => near(seed, index),
            Coin::Stellar => stellar(seed, index),
            Coin::Ton => ton(seed, index),
            Coin::Starknet => starknet::address(seed, index),
            Coin::Quantus => quantus(seed, index),
            Coin::Cardano | Coin::Bittensor => None,
        }
    }

    /// Receiving address number `index` for any coin, from the phrase and its seed.
    pub fn address_from_phrase(self, phrase: &str, seed: &[u8; 64], index: u32) -> Option<String> {
        match self {
            Coin::Cardano => cardano::address(&crate::entropy(phrase)?, index),
            Coin::Bittensor => bittensor(&crate::entropy(phrase)?, index),
            _ => self.address(seed, index),
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

/// BIP 44: m/44'/60'/0'/0/i, the address with its EIP-55 checksum. Every network in
/// `evm::NETWORKS` uses this one address.
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

/// The ed25519 public key of the SLIP-10 key at `path` (all steps hardened).
fn ed25519_public(seed: &[u8; 64], path: &[u32]) -> [u8; 32] {
    let secret = Zeroizing::new(slip10::derive(seed, path));
    ed25519_dalek::SigningKey::from_bytes(&secret).verifying_key().to_bytes()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// m/44'/784'/i'/0'/0' (Sui Wallet/Slush, Suiet, Ledger): 0x + blake2b-256 of the
/// ed25519 scheme flag (0x00) and the public key.
pub fn sui(seed: &[u8; 64], i: u32) -> Option<String> {
    let mut h = Blake2b::<U32>::new();
    h.update([0x00]);
    h.update(ed25519_public(seed, &[44, 784, i, 0, 0]));
    Some(format!("0x{}", hex(&h.finalize())))
}

/// m/44'/637'/i'/0'/0' (Petra, Pontem, Ledger): the legacy ed25519 authentication
/// key, sha3-256 of the public key and the scheme byte 0x00, as the account address.
pub fn aptos(seed: &[u8; 64], i: u32) -> Option<String> {
    let mut h = Sha3_256::new();
    h.update(ed25519_public(seed, &[44, 637, i, 0, 0]));
    h.update([0x00]);
    Some(format!("0x{}", hex(&h.finalize())))
}

/// m/44'/397'/i' (near-seed-phrase, so MyNearWallet, Meteor, HERE): the implicit
/// account, the ed25519 public key in hex (0 is the wallets' only key; the next ones
/// follow the same pattern).
pub fn near(seed: &[u8; 64], i: u32) -> Option<String> {
    Some(hex(&ed25519_public(seed, &[44, 397, i])))
}

/// SEP-0005: m/44'/148'/i' (Lobstr, Freighter, Ledger), the public key as a StrKey
/// (G…): version byte 6 << 3, key, CRC-16/XMODEM little-endian, base32.
pub fn stellar(seed: &[u8; 64], i: u32) -> Option<String> {
    let mut data = vec![6 << 3];
    data.extend_from_slice(&ed25519_public(seed, &[44, 148, i]));
    let check = ton::crc16(&data);
    data.extend_from_slice(&check.to_le_bytes());
    Some(base32(&data))
}

/// RFC 4648 base32 without padding.
fn base32(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let (mut acc, mut bits) = (0u32, 0);
    for &b in data {
        acc = acc << 8 | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[(acc >> bits) as usize & 31] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[(acc << (5 - bits)) as usize & 31] as char);
    }
    out
}

/// m/44'/607'/i' (Trust Wallet, the BIP 39 TON wallets): the wallet v4r2 address,
/// non-bounceable (UQ…). See `ton`.
pub fn ton(seed: &[u8; 64], i: u32) -> Option<String> {
    Some(ton::friendly(&ton::v4r2_account(&ed25519_public(seed, &[44, 607, i]))))
}

/// Bittensor (btcli, the Bittensor wallet, Talisman, polkadot.js), the Substrate way:
/// the sr25519 mini secret is the first 32 bytes of PBKDF2-HMAC-SHA512 over the
/// phrase's entropy (salt "mnemonic", 2048 rounds), expanded Ed25519-style; the
/// account is its public key in SS58 with prefix 42 (5…). Address 0 is the root
/// account (the coldkey btcli makes); address i is the hard derivation //i.
pub fn bittensor(entropy: &[u8], i: u32) -> Option<String> {
    use schnorrkel::{ExpansionMode, MiniSecretKey, derive::ChainCode};
    let mut mini = Zeroizing::new([0u8; 64]);
    pbkdf2::pbkdf2_hmac::<sha2::Sha512>(entropy, b"mnemonic", 2048, &mut *mini);
    let mut secret = MiniSecretKey::from_bytes(&mini[..32]).ok()?.expand(ExpansionMode::Ed25519);
    if i > 0 {
        // A numeric junction is the SCALE u64, zero-padded to the 32-byte chain code.
        let mut code = [0u8; 32];
        code[..8].copy_from_slice(&(i as u64).to_le_bytes());
        secret = secret.hard_derive_mini_secret_key(Some(ChainCode(code)), b"").0.expand(ExpansionMode::Ed25519);
    }
    Some(ss58(42, &secret.to_public().to_bytes()))
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

/// SS58: the network prefix (one byte below 64, two bytes up to 16383), the account,
/// and two bytes of blake2b-512("SS58PRE" ‖ both).
fn ss58(format: u16, account: &[u8; 32]) -> String {
    let mut data = if format < 64 {
        vec![format as u8]
    } else {
        vec![((format & 0b1111_1100) >> 2) as u8 | 0b0100_0000, (format >> 8) as u8 | ((format & 0b11) as u8) << 6]
    };
    data.extend_from_slice(account);
    let mut h = Blake2b512::new();
    h.update(b"SS58PRE");
    h.update(&data);
    let check = h.finalize();
    data.extend_from_slice(&check[..2]);
    bs58::encode(data).into_string()
}
