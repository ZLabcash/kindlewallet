//! Each coin against its standard's published test vector, from the BIP 39 phrase
//! "abandon × 11, about".

use kindlewallet::{coins, monero, vault};

const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

#[test]
fn bitcoin_bip84() {
    // BIP 84's own test vector.
    let seed = kindlewallet::seed(PHRASE);
    assert_eq!(coins::bitcoin(&seed, 0).unwrap(), "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu");
    assert_eq!(coins::bitcoin(&seed, 1).unwrap(), "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g");
}

#[test]
fn ethereum_bip44() {
    let seed = kindlewallet::seed(PHRASE);
    assert_eq!(coins::ethereum(&seed, 0).unwrap(), "0x9858EfFD232B4033E47d90003D41EC34EcaEda94");
}

#[test]
fn zcash_orchard() {
    let seed = kindlewallet::seed(PHRASE);
    let a = coins::zcash(&seed, 0).unwrap();
    assert!(a.starts_with("u1"));
    assert_ne!(a, coins::zcash(&seed, 1).unwrap());
    assert!(coins::zcash_ufvk(&seed).unwrap().starts_with("uview1"));
}

#[test]
fn phrases_and_vault() {
    assert!(kindlewallet::check_phrase(PHRASE).is_some());
    assert!(kindlewallet::check_phrase(&PHRASE.replace("about", "abandon")).is_none());
    let fresh = kindlewallet::new_phrase();
    assert_eq!(fresh.split(' ').count(), 24);
    assert!(kindlewallet::check_phrase(&fresh).is_some());
    let sealed = vault::seal(&fresh, "482913");
    assert_eq!(&*vault::open(&sealed, "482913").unwrap(), &fresh);
    assert_eq!(vault::open(&sealed, "000000").unwrap_err(), vault::Error::WrongPin);
}

// Trezor's device tests (common/tests/fixtures/solana/get_address.json), "all" × 12.
#[test]
fn solana_slip10() {
    let seed = kindlewallet::seed("all all all all all all all all all all all all");
    assert_eq!(coins::solana(&seed, 0).unwrap(), "14CCvQzQzHCVgZM3j9soPnXuJXh1RmCfwLVUcdfbZVBS");
}

// Trezor's device tests (tests/device_tests/monero/test_getaddress.py).
#[test]
fn monero_trezor() {
    let seed = kindlewallet::seed("alcohol woman abuse must during monitor noble actual mixed trade anger aisle");
    let expect = [
        "4Ahp23WfMrMFK3wYL2hLWQFGt87ZTeRkufS6JoQZu6MEFDokAQeGWmu9MA3GFq1yVLSJQbKJqVAn9F9DLYGpRzRAEXqAXKM",
        "44iAazhoAkv5a5RqLNVyh82a1n3ceNggmN4Ho7bUBJ14WkEVR8uFTe9f7v5rNnJ2kEbVXxfXiRzsD5Jtc6NvBi4D6WNHPie",
        "47ejhmbZ4wHUhXaqA4b7PN667oPMkokf4ZkNdWrMSPy9TNaLVr7vLqVUQHh2MnmaAEiyrvLsX8xUf99q3j1iAeMV8YvSFcH",
    ];
    for (account, address) in expect.iter().enumerate() {
        assert_eq!(&monero::Keys::account(&seed, account as u32).address(0), address);
    }
    let keys = monero::Keys::from_seed(&seed);
    assert!(keys.address(1).starts_with('8'));
    assert_eq!(keys.phrase().split(' ').count(), 25);
}

// Quantus's own vectors (quantus_sdk/test/generate_keys_test.dart, quantus-cli wallet tests):
// ML-DSA-87 at m/44'/189189'/i'/0'/0'.
#[test]
fn quantus_mldsa87() {
    let seed = kindlewallet::seed(
        "orchard answer curve patient visual flower maze noise retreat penalty cage small earth domain scan pitch bottom crunch theme club client swap slice raven",
    );
    assert_eq!(coins::quantus(&seed, 0).unwrap(), "qzm5QCox8Dp5A3oSXZZYHD8YoYgPz7enykZb6RPUropdCyN5h");
    assert_eq!(coins::quantus(&seed, 1).unwrap(), "qzmufPopkLKAwDmTzR5uXg8GMp5sUP48CqafJLUz3fPMSSGSh");
}

// BIP 84's own test vector: the account's extended public key.
#[test]
fn bitcoin_zpub() {
    let seed = kindlewallet::seed(PHRASE);
    assert_eq!(
        coins::bitcoin_zpub(&seed).unwrap(),
        "zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs"
    );
}

#[test]
fn evm_networks_share_the_address() {
    let seed = kindlewallet::seed(PHRASE);
    for coin in coins::Coin::ALL.into_iter().filter(|c| c.chain_id().is_some()) {
        assert_eq!(coin.address(&seed, 0).unwrap(), "0x9858EfFD232B4033E47d90003D41EC34EcaEda94");
    }
}

// Trezor's device tests (tests/device_tests/ripple/test_get_address.py).
#[test]
fn xrp_bip44() {
    let seed = kindlewallet::seed(
        "armed bundle pudding lazy strategy impulse where identify submit weekend physical antenna flight social acoustic absurd whip snack decide blur unfold fiction pumpkin athlete",
    );
    assert_eq!(coins::xrp(&seed, 0).unwrap(), "r4ocGE47gm4G4LkA9mriVHQqzpMLBTgnTY");
    assert_eq!(coins::xrp(&seed, 1).unwrap(), "rUt9ULSrUvfCmke8HTFU1szbmFpWzVbBXW");
}

// The address TronLink and every BIP 44 tool give for this phrase (checked with bip_utils).
#[test]
fn tron_bip44() {
    let seed = kindlewallet::seed(PHRASE);
    assert_eq!(coins::tron(&seed, 0).unwrap(), "TUEZSdKsoDHQMeZwihtdoBiN46zxhGWYdH");
    assert_eq!(coins::tron(&seed, 1).unwrap(), "TSeJkUh4Qv67VNFwY8LaAxERygNdy6NQZK");
    assert_eq!(coins::xrp(&seed, 0).unwrap(), "rHsMGQEkVNJmpGWs8XUBoTBiAAbwxZN5v3");
}
