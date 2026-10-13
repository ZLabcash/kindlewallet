//! Cardano Shelley addresses, the way Yoroi, Eternl, Lace and Ledger make them
//! (CIP-1852): an Icarus master key from the phrase's entropy (not the BIP 39 seed),
//! BIP32-Ed25519 (Khovratovich–Law, version 2) down to m/1852'/1815'/0', then the
//! payment key at …/0/i and the staking key at …/2/0, in a base address (addr1…).

use bech32::{Bech32, Hrp};
use blake2::{Blake2b, Digest, digest::consts::U28};
use curve25519_dalek::{Scalar, constants::ED25519_BASEPOINT_TABLE};
use hmac::{Hmac, Mac};
use sha2::Sha512;
use zeroize::{Zeroize, Zeroizing};

use crate::bip32::HARDENED;

/// An extended private key: the 64-byte ed25519 secret (kL, kR) and its chain code.
pub struct XPrv {
    kl: [u8; 32],
    kr: [u8; 32],
    chain: [u8; 32],
}

impl XPrv {
    /// The Icarus master key (CIP-3): PBKDF2-HMAC-SHA512 of the entropy, 4096
    /// rounds, no passphrase, 96 bytes, clamped.
    pub fn master(entropy: &[u8]) -> Self {
        let mut k = Zeroizing::new([0u8; 96]);
        pbkdf2::pbkdf2_hmac::<Sha512>(b"", entropy, 4096, &mut *k);
        k[0] &= 0b1111_1000;
        k[31] &= 0b0001_1111;
        k[31] |= 0b0100_0000;
        let part = |r: core::ops::Range<usize>| -> [u8; 32] { k[r].try_into().expect("32 bytes") };
        XPrv { kl: part(0..32), kr: part(32..64), chain: part(64..96) }
    }

    /// The ed25519 public key (kL · B).
    pub fn public(&self) -> [u8; 32] {
        let s = Zeroizing::new(Scalar::from_bytes_mod_order(self.kl));
        (&*s * ED25519_BASEPOINT_TABLE).compress().to_bytes()
    }

    /// Child `index`; hardened when the flag is set.
    pub fn child(&self, index: u32) -> Self {
        let mac = |tag: u8| -> Zeroizing<[u8; 64]> {
            let mut m = Hmac::<Sha512>::new_from_slice(&self.chain).expect("any key length");
            m.update(&[tag]);
            if index & HARDENED != 0 {
                m.update(&self.kl);
                m.update(&self.kr);
            } else {
                m.update(&self.public());
            }
            m.update(&index.to_le_bytes());
            Zeroizing::new(m.finalize().into_bytes().into())
        };
        let hardened = index & HARDENED != 0;
        let z = mac(if hardened { 0 } else { 2 });
        let c = mac(if hardened { 1 } else { 3 });
        // kL + 8·ZL (ZL: the first 28 bytes), kR + ZR, both little-endian mod 2^256.
        let mut kl = [0u8; 32];
        let mut carry = 0u16;
        for i in 0..32 {
            let low = if i < 28 { z[i] << 3 } else { 0 };
            let high = if (1..=28).contains(&i) { z[i - 1] >> 5 } else { 0 };
            let sum = self.kl[i] as u16 + (low | high) as u16 + carry;
            kl[i] = sum as u8;
            carry = sum >> 8;
        }
        let mut kr = [0u8; 32];
        carry = 0;
        for i in 0..32 {
            let sum = self.kr[i] as u16 + z[32 + i] as u16 + carry;
            kr[i] = sum as u8;
            carry = sum >> 8;
        }
        XPrv { kl, kr, chain: c[32..].try_into().expect("32 bytes") }
    }

    /// m/a/b/c… with hardened steps already flagged.
    pub fn derive(&self, path: &[u32]) -> Self {
        let mut key = self.child(path[0]);
        for &i in &path[1..] {
            key = key.child(i);
        }
        key
    }
}

impl Drop for XPrv {
    fn drop(&mut self) {
        self.kl.zeroize();
        self.kr.zeroize();
        self.chain.zeroize();
    }
}

fn key_hash(public: &[u8; 32]) -> [u8; 28] {
    Blake2b::<U28>::digest(public).into()
}

/// Base address i of account 0, from the BIP 39 entropy: payment key m/1852'/1815'/0'/0/i,
/// staking key m/1852'/1815'/0'/2/0 (one stake key for the account, as wallets do).
pub fn address(entropy: &[u8], i: u32) -> Option<String> {
    let account = XPrv::master(entropy).derive(&[1852 | HARDENED, 1815 | HARDENED, HARDENED]);
    let payment = account.derive(&[0, i]).public();
    let stake = account.derive(&[2, 0]).public();
    let mut data = vec![0x01]; // header: type 0 (key hash, key hash), network 1 (mainnet)
    data.extend_from_slice(&key_hash(&payment));
    data.extend_from_slice(&key_hash(&stake));
    bech32::encode::<Bech32>(Hrp::parse("addr").ok()?, &data).ok()
}
