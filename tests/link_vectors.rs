//! The phone link's handshake and records with fixed keys, for ZApp to check its own
//! side against: tests/link_vectors.json holds the same transcript.
//!
//! `cargo test --test link_vectors` asserts the file; with KINDLEWALLET_WRITE_VECTORS=1
//! (or when the file is missing) it writes it, and prints it either way.

use std::net::Ipv4Addr;

use kindlewallet::link::{self, Cipher, HELLO_KINDLE, HELLO_PHONE};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};
use zfirm_core::protocol::{self, msg};

/// 32 bytes counting up from `first`.
fn scalar(first: u8) -> [u8; 32] {
    std::array::from_fn(|i| first.wrapping_add(i as u8))
}

fn pair(first: u8) -> (StaticSecret, [u8; 32]) {
    let secret = StaticSecret::from(scalar(first));
    let public = PublicKey::from(&secret).to_bytes();
    (secret, public)
}

fn transcript() -> Value {
    let (k, k_pub) = pair(0x01);
    let (p, p_pub) = pair(0x21);
    let (e_p, e_p_pub) = pair(0x41);
    let (e_k, e_k_pub) = pair(0x61);
    let psk: [u8; 16] = std::array::from_fn(|i| 0xA0 + i as u8);

    let phone_hello = [&[HELLO_PHONE][..], &e_p_pub, &p_pub].concat();
    let kindle_hello = [&[HELLO_KINDLE][..], &e_k_pub].concat();
    let th = link::transcript(&e_p_pub, &p_pub, &k_pub, &e_k_pub);
    let dh = |a: &StaticSecret, b: &[u8; 32]| a.diffie_hellman(&PublicKey::from(*b)).to_bytes();
    let (dh1, dh2, dh3) = (dh(&e_p, &k_pub), dh(&p, &k_pub), dh(&e_p, &e_k_pub));

    // Both ends derive the same keys, with the QR's secret and with zeros.
    let keys = link::phone_keys(&p, &e_p, &k_pub, &e_k_pub, &psk).unwrap();
    assert!(link::kindle_keys(&k, &e_k, &e_p_pub, &p_pub, &psk).unwrap() == keys);
    let zero = link::phone_keys(&p, &e_p, &k_pub, &e_k_pub, &[0; 16]).unwrap();
    assert!(link::kindle_keys(&k, &e_k, &e_p_pub, &p_pub, &[0; 16]).unwrap() == zero);
    assert!(zero.k_pk != keys.k_pk);

    // The phone's first records: PING (counter 0), then HELLO (counter 1). The
    // Kindle's first: PONG (counter 0).
    let ping = protocol::encode(msg::PING, &[]);
    let hello = protocol::encode(msg::HELLO, &[]);
    let pong = protocol::encode(msg::PONG, &[]);
    let mut to_kindle = Cipher::new(&keys.k_pk);
    let (ping_record, hello_record) = (to_kindle.seal(&ping), to_kindle.seal(&hello));
    let pong_record = Cipher::new(&keys.k_kp).seal(&pong);
    let zero_ping_record = Cipher::new(&zero.k_pk).seal(&ping);

    // And they open on the other end, in order.
    let mut kindle_in = Cipher::new(&keys.k_pk);
    assert_eq!(kindle_in.open(&ping_record[2..]).unwrap(), ping);
    assert_eq!(kindle_in.open(&hello_record[2..]).unwrap(), hello);
    assert!(Cipher::new(&keys.k_pk).open(&zero_ping_record[2..]).is_none());

    let h = |b: &[u8]| hex::encode(b);
    let b = |x: &[u8]| link::b64(x);
    json!({
        "about": "KindleWallet phone link v1 test vectors (kindlewallet/tests/link_vectors.rs). Hex unless named _b64 (base64url, no padding). Scalars are X25519 secret keys before clamping.",
        "label": "zfirm-kindle-v1",
        "kindle_static_secret": h(&scalar(0x01)),
        "kindle_static_public": h(&k_pub),
        "kindle_static_public_b64": b(&k_pub),
        "phone_static_secret": h(&scalar(0x21)),
        "phone_static_public": h(&p_pub),
        "phone_static_public_b64": b(&p_pub),
        "phone_ephemeral_secret": h(&scalar(0x41)),
        "phone_ephemeral_public": h(&e_p_pub),
        "kindle_ephemeral_secret": h(&scalar(0x61)),
        "kindle_ephemeral_public": h(&e_k_pub),
        "psk": h(&psk),
        "psk_b64": b(&psk),
        "pair_uri": link::pair_uri(Ipv4Addr::new(192, 168, 1, 23), link::PORT, &k_pub, &psk, "Kindle"),
        "phone_hello": h(&phone_hello),
        "kindle_hello": h(&kindle_hello),
        "dh1": h(&dh1),
        "dh2": h(&dh2),
        "dh3": h(&dh3),
        "th": h(&th),
        "th_check": h(&Sha256::digest([b"zfirm-kindle-v1".as_slice(), &e_p_pub, &p_pub, &k_pub, &e_k_pub].concat())),
        "with_psk": {
            "k_pk": h(&keys.k_pk),
            "k_kp": h(&keys.k_kp),
            "ping_frame": h(&ping),
            "hello_frame": h(&hello),
            "pong_frame": h(&pong),
            "phone_record_0_ping": h(&ping_record),
            "phone_record_1_hello": h(&hello_record),
            "kindle_record_0_pong": h(&pong_record),
        },
        "with_zero_psk": {
            "k_pk": h(&zero.k_pk),
            "k_kp": h(&zero.k_kp),
            "phone_record_0_ping": h(&zero_ping_record),
        },
        "nfts_frame_1_42_7777": h(&protocol::encode(link::msg::NFTS, &[3, 0, 1, 0, 42, 0, 0x61, 0x1E])),
        "nfts_ok_frame": h(&protocol::encode(link::msg::NFTS_OK, &[])),
    })
}

#[test]
fn link_vectors() {
    let vectors = transcript();
    let text = serde_json::to_string_pretty(&vectors).unwrap() + "\n";
    println!("{text}");
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/link_vectors.json");
    if std::env::var_os("KINDLEWALLET_WRITE_VECTORS").is_some() || !std::path::Path::new(path).exists() {
        std::fs::write(path, &text).unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(stored, vectors, "tests/link_vectors.json differs from the code");
}

#[test]
fn records_split_at_16_kib() {
    let key = [7u8; 32];
    let bytes = vec![0x5A; link::MAX_RECORD + 10];
    let sealed = Cipher::new(&key).seal_all(&bytes);
    let mut r = sealed.as_slice();
    let mut opener = Cipher::new(&key);
    let mut out = Vec::new();
    while !r.is_empty() {
        out.extend(opener.open(&link::read_record(&mut r).unwrap()).unwrap());
    }
    assert_eq!(out, bytes);
}

#[test]
fn nfts_payload_is_little_endian() {
    assert_eq!(kindlewallet::nft::parse_tokens(&[3, 0, 1, 0, 42, 0, 0x61, 0x1E]), Some(vec![1, 42, 7777]));
    // Count and length disagree.
    assert_eq!(kindlewallet::nft::parse_tokens(&[0, 3, 0, 1, 0, 42, 0x1E, 0x61]), None);
    // Numbers outside 1..=10000 are dropped.
    assert_eq!(kindlewallet::nft::parse_tokens(&[3, 0, 0, 0, 0x11, 0x27, 5, 0]), Some(vec![5]));
}
