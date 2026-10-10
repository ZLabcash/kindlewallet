//! The phrase, sealed with the PIN: Argon2id (32 MiB, 3 passes) then AES-256-GCM.
//! File: "ZKV1" || salt (16) || nonce (12) || ciphertext + tag.
//!
//! Plainly: the Kindle has no secure element, so the PIN is the only thing between
//! a copy of this file and the phrase. Argon2id makes each guess cost real memory
//! and time; a long PIN is what makes guessing hopeless.

use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

const MAGIC: &[u8; 4] = b"ZKV1";

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    WrongPin,
    Corrupt,
}

fn key(pin: &str, salt: &[u8]) -> Zeroizing<[u8; 32]> {
    let params = Params::new(32 * 1024, 3, 1, Some(32)).expect("valid params");
    let mut out = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pin.as_bytes(), salt, &mut *out)
        .expect("valid lengths");
    out
}

pub fn seal(phrase: &str, pin: &str) -> Vec<u8> {
    let mut salt_nonce = [0u8; 28];
    getrandom::getrandom(&mut salt_nonce).expect("system randomness");
    let (salt, nonce) = salt_nonce.split_at(16);
    let k = key(pin, salt);
    let cipher = Aes256Gcm::new_from_slice(&*k).expect("32-byte key");
    let ct = cipher.encrypt(nonce.into(), phrase.as_bytes()).expect("encrypts");
    [MAGIC.as_slice(), &salt_nonce, &ct].concat()
}

pub fn open(file: &[u8], pin: &str) -> Result<Zeroizing<String>, Error> {
    if file.len() < 4 + 28 + 16 || &file[..4] != MAGIC {
        return Err(Error::Corrupt);
    }
    let (salt, rest) = file[4..].split_at(16);
    let (nonce, ct) = rest.split_at(12);
    let k = key(pin, salt);
    let cipher = Aes256Gcm::new_from_slice(&*k).expect("32-byte key");
    let plain = cipher.decrypt(nonce.into(), ct).map_err(|_| Error::WrongPin)?;
    String::from_utf8(plain).map(Zeroizing::new).map_err(|_| Error::Corrupt)
}
