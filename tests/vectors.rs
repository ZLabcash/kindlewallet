//! Each coin against its standard's published test vector, from the BIP 39 phrase
//! "abandon × 11, about".

use zkindle::{coins, vault};

const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

#[test]
fn bitcoin_bip84() {
    // BIP 84's own test vector.
    let seed = zkindle::seed(PHRASE);
    assert_eq!(coins::bitcoin(&seed, 0).unwrap(), "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu");
    assert_eq!(coins::bitcoin(&seed, 1).unwrap(), "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g");
}

#[test]
fn ethereum_bip44() {
    let seed = zkindle::seed(PHRASE);
    assert_eq!(coins::ethereum(&seed, 0).unwrap(), "0x9858EfFD232B4033E47d90003D41EC34EcaEda94");
}

#[test]
fn zcash_orchard() {
    let seed = zkindle::seed(PHRASE);
    let a = coins::zcash(&seed, 0).unwrap();
    assert!(a.starts_with("u1"));
    assert_ne!(a, coins::zcash(&seed, 1).unwrap());
    assert!(coins::zcash_ufvk(&seed).unwrap().starts_with("uview1"));
}

#[test]
fn phrases_and_vault() {
    assert!(zkindle::check_phrase(PHRASE).is_some());
    assert!(zkindle::check_phrase(&PHRASE.replace("about", "abandon")).is_none());
    let fresh = zkindle::new_phrase();
    assert_eq!(fresh.split(' ').count(), 24);
    assert!(zkindle::check_phrase(&fresh).is_some());
    let sealed = vault::seal(&fresh, "482913");
    assert_eq!(&*vault::open(&sealed, "482913").unwrap(), &fresh);
    assert_eq!(vault::open(&sealed, "000000").unwrap_err(), vault::Error::WrongPin);
}
