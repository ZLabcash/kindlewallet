//! BIP 32 on secp256k1, private derivation only (enough to make addresses).

use hmac::{Hmac, Mac};
use k256::{SecretKey, elliptic_curve::sec1::ToEncodedPoint};
use sha2::Sha512;

pub const HARDENED: u32 = 0x8000_0000;

pub struct ExtendedKey {
    pub key: SecretKey,
    pub chain: [u8; 32],
}

fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 64] {
    let mut m = Hmac::<Sha512>::new_from_slice(key).expect("any key length");
    for p in parts {
        m.update(p);
    }
    m.finalize().into_bytes().into()
}

impl ExtendedKey {
    pub fn master(seed: &[u8]) -> Option<Self> {
        let i = hmac(b"Bitcoin seed", &[seed]);
        Some(ExtendedKey { key: SecretKey::from_slice(&i[..32]).ok()?, chain: i[32..].try_into().ok()? })
    }

    pub fn child(&self, index: u32) -> Option<Self> {
        let i = if index & HARDENED != 0 {
            hmac(&self.chain, &[&[0], &self.key.to_bytes(), &index.to_be_bytes()])
        } else {
            hmac(&self.chain, &[self.public(true).as_bytes(), &index.to_be_bytes()])
        };
        let tweak = SecretKey::from_slice(&i[..32]).ok()?;
        let sum = *tweak.to_nonzero_scalar() + *self.key.to_nonzero_scalar();
        let key = SecretKey::from_bytes(&sum.to_bytes()).ok()?;
        Some(ExtendedKey { key, chain: i[32..].try_into().ok()? })
    }

    /// m/a/b/c… with hardened steps already flagged.
    pub fn derive(seed: &[u8], path: &[u32]) -> Option<Self> {
        path.iter().try_fold(Self::master(seed)?, |k, &i| k.child(i))
    }

    pub fn public(&self, compressed: bool) -> k256::EncodedPoint {
        self.key.public_key().to_encoded_point(compressed)
    }
}
