//! Monero from the BIP 39 seed, the way Trezor does it: the SLIP-10 ed25519 key at
//! m/44'/128'/0' is the Monero recovery key, so its 25-word Monero phrase restores
//! the same wallet in any Monero app. Address 0 is the primary address (4…), the
//! next ones are subaddresses of account 0 (8…).

use curve25519_dalek::{EdwardsPoint, Scalar, constants::ED25519_BASEPOINT_TABLE};
use sha3::{Digest, Keccak256};
use zeroize::Zeroizing;

use crate::slip10;

const PRIMARY: u8 = 18;
const SUBADDRESS: u8 = 42;
const WORDS: &str = include_str!("../assets/monero_english.txt");

pub struct Keys {
    pub spend: Zeroizing<Scalar>,
    pub view: Zeroizing<Scalar>,
}

impl Keys {
    pub fn from_seed(seed: &[u8; 64]) -> Self {
        Self::account(seed, 0)
    }

    /// The wallet at m/44'/128'/account' (Trezor's accounts).
    pub fn account(seed: &[u8; 64], account: u32) -> Self {
        let recovery = Zeroizing::new(slip10::derive(seed, &[44, 128, account]));
        let spend = Zeroizing::new(Scalar::from_bytes_mod_order(*recovery));
        let view = Zeroizing::new(Scalar::from_bytes_mod_order(Keccak256::digest(spend.as_bytes()).into()));
        Keys { spend, view }
    }

    fn spend_public(&self) -> EdwardsPoint {
        &*self.spend * ED25519_BASEPOINT_TABLE
    }

    /// Primary address for 0, subaddress (0, j) otherwise.
    pub fn address(&self, j: u32) -> String {
        let spend = self.spend_public();
        if j == 0 {
            return encode(PRIMARY, &spend, &(&*self.view * ED25519_BASEPOINT_TABLE));
        }
        let mut h = Keccak256::new();
        h.update(b"SubAddr\0");
        h.update(self.view.as_bytes());
        h.update(0u32.to_le_bytes());
        h.update(j.to_le_bytes());
        let m = Scalar::from_bytes_mod_order(h.finalize().into());
        let d = spend + &m * ED25519_BASEPOINT_TABLE;
        encode(SUBADDRESS, &d, &(*self.view * d))
    }

    /// The private view key, hex, for a view-only wallet next to the primary address.
    pub fn view_key_hex(&self) -> String {
        self.view.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The standard 25-word Monero phrase of the spend key.
    pub fn phrase(&self) -> String {
        let list: Vec<&str> = WORDS.lines().collect();
        let n = list.len() as u32;
        let mut words: Vec<&str> = Vec::with_capacity(25);
        for chunk in self.spend.as_bytes().chunks(4) {
            let x = u32::from_le_bytes(chunk.try_into().expect("4 bytes"));
            let w1 = x % n;
            let w2 = (x / n + w1) % n;
            let w3 = (x / n / n + w2) % n;
            words.extend([list[w1 as usize], list[w2 as usize], list[w3 as usize]]);
        }
        let prefixes: String = words.iter().map(|w| &w[..3.min(w.len())]).collect();
        let check = crc32fast::hash(prefixes.as_bytes()) as usize % words.len();
        words.push(words[check]);
        words.join(" ")
    }
}

fn encode(prefix: u8, spend: &EdwardsPoint, view: &EdwardsPoint) -> String {
    let mut data = vec![prefix];
    data.extend_from_slice(spend.compress().as_bytes());
    data.extend_from_slice(view.compress().as_bytes());
    let check = Keccak256::digest(&data);
    data.extend_from_slice(&check[..4]);
    base58(&data)
}

/// Monero's base58: 8-byte blocks, each written as exactly 11 characters (a shorter
/// last block as fewer).
fn base58(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    const SIZES: [usize; 9] = [0, 2, 3, 5, 6, 7, 9, 10, 11];
    let mut out = String::new();
    for block in data.chunks(8) {
        let mut num = block.iter().fold(0u64, |n, &b| n << 8 | b as u64);
        let mut chars = vec![b'1'; SIZES[block.len()]];
        for c in chars.iter_mut().rev() {
            *c = ALPHABET[(num % 58) as usize];
            num /= 58;
        }
        out.push_str(std::str::from_utf8(&chars).expect("ascii"));
    }
    out
}
