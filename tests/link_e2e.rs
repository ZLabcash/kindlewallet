//! End to end over TCP: `kindlewallet --serve` with its test wallet, and a fake phone
//! written here from the spec. It pairs with the QR's secret, gets the viewing key,
//! hands over its zkSNARKs, sends a real transaction (tests/sign_request.bin, made
//! from a librustzcash PCZT through zfirm's phone bridge) and checks the signatures;
//! strangers are turned away.

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use kindlewallet::link::{self, Cipher, HELLO_KINDLE, HELLO_PHONE};
use orchard::primitives::redpallas::{Signature, SpendAuth, VerificationKey};
use x25519_dalek::{PublicKey, StaticSecret};
use zfirm_core::{
    Account, Network,
    protocol::{self, Decoder, Frame, Info, error, msg, status},
    request::{self, SignRequest},
    sign,
};

const PIN: &str = "424242";

/// The server, stopped when dropped.
struct Served {
    child: Child,
    port: u16,
    dir: PathBuf,
    k: [u8; 32],
    psk: [u8; 16],
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn serve(name: &str) -> Served {
    let dir = std::env::temp_dir().join(format!("kindlewallet-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kindlewallet"))
        .arg("--serve")
        .arg(&dir)
        .args(["--port", &port.to_string(), "--bind", "127.0.0.1", "--pin", PIN, "--auto-approve"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // "pairing QR: zfirm-kindle:?h=127.0.0.1:PORT&k=K&p=PSK&n=Kindle"
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let uri = loop {
        let line = lines.next().expect("the server printed its QR").unwrap();
        if let Some(uri) = line.strip_prefix("pairing QR: ") {
            break uri.to_string();
        }
    };
    std::thread::spawn(move || lines.for_each(drop));
    let param = |key: &str| {
        let query = uri.split_once('?').unwrap().1;
        query.split('&').find_map(|kv| kv.strip_prefix(&format!("{key}="))).unwrap().to_string()
    };
    assert_eq!(param("h"), format!("127.0.0.1:{port}"));
    assert_eq!(param("n"), "Kindle");
    let k = B64.decode(param("k")).unwrap();
    let psk = B64.decode(param("p")).unwrap();
    Served { child, port, dir, k: k.try_into().unwrap(), psk: psk.try_into().unwrap() }
}

/// ZApp's side of the channel.
struct Phone {
    stream: TcpStream,
    out: Cipher,
    input: Cipher,
    decoder: Decoder,
}

impl Phone {
    /// Connects and sends its first record (a PING, as ZApp does).
    fn connect(s: &Served, p: &StaticSecret, psk: &[u8; 16]) -> Phone {
        let mut stream = TcpStream::connect(("127.0.0.1", s.port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(120))).unwrap();
        let mut e_secret = [0u8; 32];
        getrandom::getrandom(&mut e_secret).unwrap();
        let e_p = StaticSecret::from(e_secret);
        let hello = [&[HELLO_PHONE][..], PublicKey::from(&e_p).as_bytes(), PublicKey::from(p).as_bytes()].concat();
        stream.write_all(&hello).unwrap();
        let mut reply = [0u8; 33];
        stream.read_exact(&mut reply).unwrap();
        assert_eq!(reply[0], HELLO_KINDLE);
        let keys = link::phone_keys(p, &e_p, &s.k, &reply[1..].try_into().unwrap(), psk).unwrap();
        let mut phone =
            Phone { stream, out: Cipher::new(&keys.k_pk), input: Cipher::new(&keys.k_kp), decoder: Decoder::new() };
        phone.send(msg::PING, &[]);
        phone
    }

    fn send(&mut self, kind: u8, payload: &[u8]) {
        let records = self.out.seal_all(&protocol::encode(kind, payload));
        self.stream.write_all(&records).unwrap();
    }

    /// The next frame, `None` once the Kindle hung up.
    fn recv(&mut self) -> Option<Frame> {
        loop {
            if let Some(f) = self.decoder.next().unwrap() {
                return Some(f);
            }
            let ct = link::read_record(&mut self.stream).ok()?;
            let plain = self.input.open(&ct).expect("the Kindle's records open");
            if let Some(f) = self.decoder.push(&plain).unwrap() {
                return Some(f);
            }
        }
    }

    /// Frames until one of `kind` (STATUS updates skipped); panics on an ERROR.
    fn until(&mut self, kind: u8) -> Frame {
        loop {
            let f = self.recv().expect("still connected");
            if f.kind == kind {
                return f;
            }
            assert_ne!(f.kind, msg::ERROR, "error {:?}", protocol::decode_error(&f.payload));
        }
    }
}

fn static_key() -> StaticSecret {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).unwrap();
    StaticSecret::from(b)
}

fn test_account() -> Account {
    let phrase = ["abandon"; 23].join(" ") + " art";
    Account::from_seed(&kindlewallet::seed(&phrase), Network::Main, 0).unwrap()
}

fn phones(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("phones")).unwrap_or_default()
}

#[test]
fn pair_share_nfts_and_sign() {
    let s = serve("sign");
    let p = static_key();
    let mut phone = Phone::connect(&s, &p, &s.psk);
    assert_eq!(phone.recv().unwrap().kind, msg::PONG);

    // Nothing but PING before pairing.
    phone.send(link::msg::NFTS, &[1, 0, 1, 0]);
    let f = phone.recv().unwrap();
    assert_eq!((f.kind, protocol::decode_error(&f.payload).unwrap().0), (msg::ERROR, error::NOT_PAIRED));

    phone.send(msg::HELLO, &[]);
    assert_eq!(phone.recv().unwrap(), Frame { kind: msg::STATUS, payload: vec![status::AWAITING_USER] });
    let info = Info::decode(&phone.until(msg::INFO).payload).unwrap();
    let account = test_account();
    assert_eq!(info.ufvk, account.ufvk());
    assert_eq!(info.seed_fingerprint, account.seed_fingerprint());
    assert_eq!(info.device_name, "Kindle");
    let (j, address) = protocol::decode_address(&phone.until(msg::ADDRESS).payload).unwrap();
    assert_eq!(address, account.encode_address(j));
    // Allowed: remembered.
    assert_eq!(phones(&s.dir), "", "no phone is remembered");

    // The zkSNARKs the phone's zilkroad account holds.
    phone.send(link::msg::NFTS, &[3, 0, 0x61, 0x1E, 1, 0, 42, 0]);
    assert_eq!(phone.recv().unwrap(), Frame { kind: link::msg::NFTS_OK, payload: vec![] });
    assert_eq!(std::fs::read_to_string(s.dir.join("nfts")).unwrap(), "1\n42\n7777\n");

    // A real transaction: checked, approved (held), signed.
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/sign_request.bin")).unwrap();
    let req = SignRequest::decode(&bytes).unwrap();
    phone.send(msg::SIGN, &bytes);
    let sigs = request::decode_signatures(&phone.until(msg::SIGNED).payload).unwrap();
    assert_eq!(sigs.len(), 1);
    let sighash = sign::shielded_sighash(&req).unwrap();
    for sig in &sigs {
        let bundle = req.bundles.iter().find(|b| b.is_ironwood() == sig.ironwood).unwrap();
        let rk = VerificationKey::<SpendAuth>::try_from(bundle.actions[sig.action_index as usize].rk).unwrap();
        rk.verify(&sighash, &Signature::<SpendAuth>::from(sig.signature)).expect("valid spend authorization");
    }
    // Not for another sighash.
    let other = [0u8; 32];
    let rk = VerificationKey::<SpendAuth>::try_from(req.bundles[0].actions[sigs[0].action_index as usize].rk).unwrap();
    assert!(rk.verify(&other, &Signature::<SpendAuth>::from(sigs[0].signature)).is_err());
    drop(phone);

    // Back later without a QR code: refused. Every connection is a scan and an Allow,
    // and the code scanned once does not serve twice.
    let mut phone = Phone::connect(&s, &p, &[0; 16]);
    assert!(phone.recv().is_none());
    let mut phone = Phone::connect(&s, &p, &s.psk);
    assert!(phone.recv().is_none(), "a QR code serves once");
    let _ = account;
}

#[test]
fn strangers_are_turned_away() {
    let s = serve("strangers");
    // An unknown phone without the QR's secret.
    let mut phone = Phone::connect(&s, &static_key(), &[0; 16]);
    assert!(phone.recv().is_none());
    // A wrong secret.
    let mut wrong = s.psk;
    wrong[0] ^= 1;
    let mut phone = Phone::connect(&s, &static_key(), &wrong);
    assert!(phone.recv().is_none());
    assert_eq!(phones(&s.dir), "");
    // The right one still works.
    let mut phone = Phone::connect(&s, &static_key(), &s.psk);
    assert_eq!(phone.recv().unwrap().kind, msg::PONG);
}
