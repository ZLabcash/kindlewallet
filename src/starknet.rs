//! Starknet the way Argent X (now Ready) makes it from a BIP 39 phrase: the phrase's
//! Ethereum key (m/44'/60'/0'/0/0) seeds a second BIP 32 tree, whose key at
//! m/44'/9004'/0'/0/i is ground (EIP-2645) into a Stark key. The address is the
//! counterfactual address of Argent's account contract (v0.4.0, the class it deploys
//! now) for that key: it is the same before and after the account is deployed.

use sha2::{Digest, Sha256};
use starknet_crypto::{Felt, get_public_key, pedersen_hash};
use zeroize::Zeroizing;

use crate::bip32::{ExtendedKey, HARDENED};

/// Argent account v0.4.0 (TXV3_ACCOUNT_CLASS_HASH in @argent/x-shared).
const ARGENT_ACCOUNT_CLASS: &str = "0x036078334509b514626504edc9fb252328d1a240e4e948bef8d0c08dff45927f";

/// The Stark curve's order.
const ORDER: [u8; 32] = [
    0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xb7, 0x81, 0x12, 0x6d, 0xca, 0xe7, 0xb2, 0x32, 0x1e, 0x66, 0xa2, 0x41, 0xad, 0xc6, 0x4d, 0x2f,
];

/// a − b on 256-bit big-endian numbers (a ≥ b).
fn sub(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut borrow = 0i16;
    for i in (0..32).rev() {
        let d = a[i] as i16 - b[i] as i16 - borrow;
        out[i] = d.rem_euclid(256) as u8;
        borrow = (d < 0) as i16;
    }
    out
}

/// 31 · ORDER: the largest multiple of the order below 2^256.
const LIMIT: [u8; 32] = [
    0xf8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x0e, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xf7,
    0x38, 0xa1, 0x3b, 0x4b, 0x92, 0x0e, 0x94, 0x11, 0xae, 0x6d, 0xa5, 0xf4, 0x0b, 0x03, 0x58, 0xb1,
];

/// EIP-2645 key grinding (as micro-starknet's grindKey): the first sha256(key ‖ i)
/// below LIMIT, reduced mod the order (so every Stark key is equally likely).
pub fn grind(key: &[u8; 32]) -> [u8; 32] {
    for i in 0u8..=255 {
        let mut h = Sha256::new();
        h.update(key);
        h.update([i]);
        let mut x: [u8; 32] = h.finalize().into();
        if x < LIMIT {
            while x >= ORDER {
                x = sub(&x, &ORDER);
            }
            return x;
        }
    }
    unreachable!("each try passes with probability 31/32")
}

/// The Stark private key of account i of Argent X (a 32-byte big-endian number).
pub fn private_key(seed: &[u8; 64], i: u32) -> Option<Zeroizing<[u8; 32]>> {
    let eth = ExtendedKey::derive(seed, &[44 | HARDENED, 60 | HARDENED, HARDENED, 0, 0])?;
    let eth = Zeroizing::new(<[u8; 32]>::from(eth.key.to_bytes()));
    private_key_from_secret(&eth, i)
}

/// From Argent's "secret" (the Ethereum private key). Argent writes it as a number,
/// so its leading zero bytes are dropped before it seeds the tree.
pub fn private_key_from_secret(secret: &[u8; 32], i: u32) -> Option<Zeroizing<[u8; 32]>> {
    let start = secret.iter().position(|&b| b != 0).unwrap_or(31);
    let node = ExtendedKey::derive(&secret[start..], &[44 | HARDENED, 9004 | HARDENED, HARDENED, 0, i])?;
    let child = Zeroizing::new(<[u8; 32]>::from(node.key.to_bytes()));
    Some(Zeroizing::new(grind(&child)))
}

/// The Stark public key (x coordinate) for a private key.
pub fn public_key(private: &[u8; 32]) -> [u8; 32] {
    get_public_key(&Felt::from_bytes_be(private)).to_bytes_be()
}

fn pedersen_array(items: &[Felt]) -> Felt {
    let h = items.iter().fold(Felt::ZERO, |h, x| pedersen_hash(&h, x));
    pedersen_hash(&h, &Felt::from(items.len() as u64))
}

/// The counterfactual address of Argent's account for this Stark public key:
/// deployed by no one (0), salt = the key, constructor (owner: Signer::Starknet(key),
/// guardian: None) = [0, key, 1].
pub fn argent_address(public: &[u8; 32]) -> [u8; 32] {
    let key = Felt::from_bytes_be(public);
    let calldata = pedersen_array(&[Felt::ZERO, key, Felt::ONE]);
    let prefix = Felt::from_bytes_be_slice(b"STARKNET_CONTRACT_ADDRESS");
    let class = Felt::from_hex(ARGENT_ACCOUNT_CLASS).expect("constant");
    let address = pedersen_array(&[prefix, Felt::ZERO, key, class, calldata]).to_bytes_be();
    // mod 2^251 − 256 (one subtraction is enough: a felt is below twice that).
    let mut bound = [0u8; 32];
    bound[0] = 0x07;
    bound[1..31].fill(0xff);
    if address >= bound { sub(&address, &bound) } else { address }
}

/// Account i: 0x and 64 hex digits, as Argent X / Ready show it.
pub fn address(seed: &[u8; 64], i: u32) -> Option<String> {
    let private = private_key(seed, i)?;
    let address = argent_address(&public_key(&private));
    Some(format!("0x{}", address.iter().map(|b| format!("{b:02x}")).collect::<String>()))
}
