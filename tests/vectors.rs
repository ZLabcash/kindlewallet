//! Each coin against its standard's published test vector, from the BIP 39 phrase
//! "abandon × 11, about".

use kindlewallet::coins::{self, Coin};
use kindlewallet::{evm, monero, starknet, vault};

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
    assert_eq!(Coin::Ethereum.address(&seed, 0).unwrap(), "0x9858EfFD232B4033E47d90003D41EC34EcaEda94");
    assert_eq!(Coin::Ethereum.chain_id(), Some(1));
    assert_eq!(evm::NETWORKS.len(), 24);
    let mut ids: Vec<u64> = evm::NETWORKS.iter().map(|n| n.chain_id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), evm::NETWORKS.len());
    for n in evm::NETWORKS {
        assert!(kindlewallet::logos::evm_logo(n.key).is_some(), "{}", n.key);
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

// Mysten's TS SDK (packages/sui/test/unit/cryptography/ed25519-keypair.test.ts), made
// against the Rust keytool.
#[test]
fn sui_slip10() {
    let cases = [
        (
            "film crazy soon outside stand loop subway crumble thrive popular green nuclear struggle pistol arm wife phrase warfare march wheat nephew ask sunny firm",
            "0xa2d14fad60c56049ecf75246a481934691214ce413e6a8ae2fe6834c173a6133",
        ),
        (
            "require decline left thought grid priority false tiny gasp angle royal system attack beef setup reward aunt skill wasp tray vital bounce inflict level",
            "0x1ada6e6f3f3e4055096f606c746690f1108fcc2ca479055cc434a3e1d3f758aa",
        ),
        (
            "organ crash swim stick traffic remember army arctic mesh slice swear summer police vast chaos cradle squirrel hood useless evidence pet hub soap lake",
            "0xe69e896ca10f5a77732769803cc2b5707f0ab9d4407afb5e4b4464b89769af14",
        ),
    ];
    for (phrase, address) in cases {
        assert_eq!(coins::sui(&kindlewallet::seed(phrase), 0).unwrap(), address);
    }
}

const SHOOT: &str = "shoot island position soft burden budget tooth cruel issue economy destroy above";

// Aptos's TS SDK (packages/ts-sdk/tests/unit/helper.ts, `wallet`): legacy ed25519 at
// m/44'/637'/0'/0'/0'.
#[test]
fn aptos_slip10() {
    let seed = kindlewallet::seed(SHOOT);
    assert_eq!(coins::aptos(&seed, 0).unwrap(), "0x07968dab936c1bad187c60ce4082f307d030d780e91e694ae03aef16aba73f30");
}

// near-seed-phrase's own test (test/index.test.js): its public key, as an implicit account.
#[test]
fn near_implicit() {
    let seed = kindlewallet::seed(SHOOT);
    let key = bs58::decode("r4yuiZE45mzeZAENDEF2pWeFBJkW8mQYGx3rU46zCqh").into_vec().unwrap();
    assert_eq!(coins::near(&seed, 0).unwrap(), hex::encode(key));
}

// SEP-0005's test vectors 3 (24 words) and 5 ("abandon … about").
#[test]
fn stellar_sep5() {
    let seed = kindlewallet::seed(
        "bench hurt jump file august wise shallow faculty impulse spring exact slush thunder author capable act festival slice deposit sauce coconut afford frown better",
    );
    assert_eq!(coins::stellar(&seed, 0).unwrap(), "GC3MMSXBWHL6CPOAVERSJITX7BH76YU252WGLUOM5CJX3E7UCYZBTPJQ");
    assert_eq!(coins::stellar(&seed, 1).unwrap(), "GB3MTYFXPBZBUINVG72XR7AQ6P2I32CYSXWNRKJ2PV5H5C7EAM5YYISO");
    assert_eq!(coins::stellar(&seed, 2).unwrap(), "GDYF7GIHS2TRGJ5WW4MZ4ELIUIBINRNYPPAWVQBPLAZXC2JRDI4DGAKU");
    let seed = kindlewallet::seed(PHRASE);
    assert_eq!(coins::stellar(&seed, 0).unwrap(), "GB3JDWCQJCWMJ3IILWIGDTQJJC5567PGVEVXSCVPEQOTDN64VJBDQBYX");
    assert_eq!(coins::stellar(&seed, 1).unwrap(), "GDVSYYTUAJ3ACHTPQNSTQBDQ4LDHQCMNY4FCEQH5TJUMSSLWQSTG42MV");
}

// Trust Wallet's wallet-core (tests/chains/TheOpenNetwork/TWAnyAddressTests.cpp):
// m/44'/607'/0', wallet v4r2.
#[test]
fn ton_v4r2() {
    let seed = kindlewallet::seed("stuff diamond cycle federal scan spread pigeon people engage teach snack grain");
    assert_eq!(coins::ton(&seed, 0).unwrap(), "UQDYW_1eScJVxtitoBRksvoV9cCYo4uKGWLVNIHB1JqRRyQx");
}

// cardano-serialization-lib (rust/src/tests/address.rs: bip32_12_base, bip32_15_base,
// bip32_24_base), mainnet base addresses from the entropy.
#[test]
fn cardano_cip1852() {
    let cases: [(&str, &str); 3] = [
        ("df9ed25ed146bf43336a5d7cf7395994", "addr1qx2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwqfjkjv7"),
        ("0ccb74f36b7da1649a8144675522d4d8097c6412", "addr1q9u5vlrf4xkxv2qpwngf6cjhtw542ayty80v8dyr49rf5ewvxwdrt70qlcpeeagscasafhffqsxy36t90ldv06wqrk2qld6xc3"),
        (
            "4e828f9a67ddcff0e6391ad4f26ddb7579f59ba14b6dd4baf63dcfdb9d2420da",
            "addr1qyy6nhfyks7wdu3dudslys37v252w2nwhv0fw2nfawemmn8k8ttq8f3gag0h89aepvx3xf69g0l9pf80tqv7cve0l33sdn8p3d",
        ),
    ];
    for (entropy, address) in cases {
        let entropy = hex::decode(entropy).unwrap();
        assert_eq!(kindlewallet::cardano::address(&entropy, 0).unwrap(), address);
        let indices = kindlewallet::mnemonic::indices_for(&entropy).unwrap();
        let phrase = kindlewallet::mnemonic::sentence(&indices, &kindlewallet::english()).unwrap();
        assert!(Coin::Cardano.needs_phrase());
        assert_eq!(Coin::Cardano.address(&kindlewallet::seed(&phrase), 0), None);
        assert_eq!(Coin::Cardano.address_from_phrase(&phrase, &kindlewallet::seed(&phrase), 0).unwrap(), address);
    }
    assert_eq!(
        kindlewallet::mnemonic::sentence(&kindlewallet::mnemonic::indices_for(&hex::decode("df9ed25ed146bf43336a5d7cf7395994").unwrap()).unwrap(), &kindlewallet::english()).unwrap(),
        "test walk nut penalty hip pave soap entry language right filter choice"
    );
}

// Argent X's own tests (packages/extension/test/keyDerivation.test.ts): Stark keys of
// accounts 5 and 7 from a secret, and EIP-2645 grinding. The address of "abandon …
// about" was checked with starknet.js (hash.calculateContractAddressFromHash with the
// constructor calldata @argent/x-shared builds for account v0.4.0).
#[test]
fn starknet_argent() {
    let secret: [u8; 32] = hex::decode("e6904d63affe7a13cd30345b000c9b1ffc087832332d7303cf237ffda8a177d0").unwrap().try_into().unwrap();
    let stark = |i| hex::encode(starknet::public_key(&starknet::private_key_from_secret(&secret, i).unwrap()));
    assert_eq!(stark(5), "05c7c65bfda7a85af0681c85c9c440f0aa6825feef6f9c96e55fb2ce08c8d4bc");
    assert_eq!(stark(7), "0605d5a0ece3b316f0d72221228acb7f01dcb34db74e0c02790db156741f5a86");
    let key: [u8; 32] = hex::decode("86F3E7293141F20A8BAFF320E8EE4ACCB9D4A4BF2B4D295E8CEE784DB46E0519").unwrap().try_into().unwrap();
    assert_eq!(hex::encode(starknet::grind(&key)), "05c8c8683596c732541a59e03007b2d30dbbbb873556fe65b5fb63c16688f941");
    let seed = kindlewallet::seed(PHRASE);
    assert_eq!(starknet::address(&seed, 0).unwrap(), "0x0424e0202bce2d9abbee87ee8faa8fd416bcf02ad8fe18d66320f23b1b07fc02");
    assert_eq!(starknet::address(&seed, 1).unwrap(), "0x0591960a2eae4798de21771480cede3ce3c17cbeeda9b3b988cfadd2e8db16a4");
}

// subkey's README (polkadot-sdk substrate/bin/utils/subkey): sr25519, SS58 prefix 42.
// //1 and //2 checked with polkadot.js (Keyring addFromUri).
#[test]
fn bittensor_sr25519() {
    let phrase = "hotel forest jar hover kite book view eight stuff angle legend defense";
    let seed = kindlewallet::seed(phrase);
    assert!(Coin::Bittensor.needs_phrase());
    assert_eq!(Coin::Bittensor.address_from_phrase(phrase, &seed, 0).unwrap(), "5Hpm9fq3W3dQgwWpAwDS2ZHKAdnk86QRCu7iX4GnmDxycrte");
    assert_eq!(Coin::Bittensor.address_from_phrase(phrase, &seed, 1).unwrap(), "5GeZ29sMuejjNiXaCaD4dcYKm7kAEdvKUEdozrHa4YnxfdQi");
    let seed = kindlewallet::seed(PHRASE);
    assert_eq!(Coin::Bittensor.address_from_phrase(PHRASE, &seed, 2).unwrap(), "5E4KqGS74S7rWFhj3D6U7KRoVPdgrnD89Ed7evZC55C58Kvh");
}

#[test]
fn every_coin_has_an_address_and_a_logo() {
    let seed = kindlewallet::seed(PHRASE);
    for coin in Coin::ALL {
        let a = coin.address_from_phrase(PHRASE, &seed, 0).unwrap();
        assert_eq!(coin.address(&seed, 0).is_none(), coin.needs_phrase(), "{a}");
        assert!(kindlewallet::logos::logo(coin).w > 0);
    }
    let with_nfts: Vec<&str> = Coin::ALL.iter().filter(|c| c.has_nfts()).map(|c| c.ticker()).collect();
    assert_eq!(with_nfts, ["BTC", "ETH", "ZEC", "SOL", "TRX", "XRP", "SUI", "APT", "NEAR", "TON", "ADA", "STRK"]);
}
