//! The Kindle's end of the link to ZApp, over the local Wi-Fi (TCP port 7700).
//!
//! Inside an encrypted channel runs exactly the zfirm protocol byte stream the Flipper
//! speaks over Bluetooth, so the phone reuses its code. The channel:
//!
//! 1. phone → Kindle: `0x01 || E_p || P` (its fresh ephemeral key, its static key);
//! 2. Kindle → phone: `0x02 || E_k`;
//! 3. both: dh1 = X25519(E_p, K), dh2 = X25519(P, K), dh3 = X25519(E_p, E_k);
//!    th = SHA256("zfirm-kindle-v1" || E_p || P || K || E_k);
//!    HKDF-SHA256(salt = psk, ikm = dh1 || dh2 || dh3, info = th) → 64 bytes, the
//!    phone→Kindle key then the Kindle→phone key;
//! 4. records both ways: `len (u16 BE) || ChaCha20-Poly1305(key, 0^4 || counter u64 BE)`.
//!
//! The psk is the 16 bytes of the pairing QR (the phone's first connection after
//! scanning it) or 16 zero bytes (a phone already in the `phones` list). The Kindle
//! keeps whichever key set opens the phone's first record, and hangs up if none does.
//! Only the holder of the Kindle's key K can derive the keys, so the phone, which
//! keeps K from the QR, knows it talks to this Kindle.

use std::{
    fs::OpenOptions,
    io::{self, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::Sender,
    },
    time::Duration,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, aead::Aead};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

pub const PORT: u16 = 7700;
/// Hashed first into the transcript.
pub const LABEL: &[u8] = b"zfirm-kindle-v1";
pub const HELLO_PHONE: u8 = 0x01;
pub const HELLO_KINDLE: u8 = 0x02;
/// Largest plaintext in one record.
pub const MAX_RECORD: usize = 16 * 1024;
const TAG: usize = 16;

/// Kindle-only frames, outside zfirm_core (the Flipper never sees them).
pub mod msg {
    /// Phone → Kindle: the zkSNARKs its zilkroad account holds.
    pub const NFTS: u8 = 0x40;
    /// Kindle → phone: they are stored (empty payload).
    pub const NFTS_OK: u8 = 0xC0;
}

/// The keys of one connection.
#[derive(Clone, PartialEq, Eq)]
pub struct Keys {
    /// Phone → Kindle.
    pub k_pk: [u8; 32],
    /// Kindle → phone.
    pub k_kp: [u8; 32],
}

/// th = SHA256(label || E_p || P || K || E_k).
pub fn transcript(e_p: &[u8; 32], p: &[u8; 32], k: &[u8; 32], e_k: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    for part in [LABEL, e_p, p, k, e_k] {
        h.update(part);
    }
    h.finalize().into()
}

fn expand(psk: &[u8; 16], dh: [&[u8; 32]; 3], th: &[u8; 32]) -> Keys {
    let mut ikm = Zeroizing::new([0u8; 96]);
    for (i, d) in dh.iter().enumerate() {
        ikm[i * 32..(i + 1) * 32].copy_from_slice(*d);
    }
    let mut okm = Zeroizing::new([0u8; 64]);
    Hkdf::<Sha256>::new(Some(psk), &*ikm).expand(th, &mut *okm).expect("64 bytes is a valid length");
    Keys { k_pk: zfirm_core::arr(&okm[..32]), k_kp: zfirm_core::arr(&okm[32..]) }
}

/// A Diffie-Hellman result, refused when the peer sent a low-order point (all zeros).
fn dh(secret: &StaticSecret, public: &[u8; 32]) -> Option<Zeroizing<[u8; 32]>> {
    let shared = secret.diffie_hellman(&PublicKey::from(*public));
    shared.was_contributory().then(|| Zeroizing::new(shared.to_bytes()))
}

/// The Kindle's side: its static key `k`, its ephemeral `e_k`, the phone's `e_p` and `p`.
pub fn kindle_keys(k: &StaticSecret, e_k: &StaticSecret, e_p: &[u8; 32], p: &[u8; 32], psk: &[u8; 16]) -> Option<Keys> {
    let th = transcript(e_p, p, PublicKey::from(k).as_bytes(), PublicKey::from(e_k).as_bytes());
    Some(expand(psk, [&*dh(k, e_p)?, &*dh(k, p)?, &*dh(e_k, e_p)?], &th))
}

/// The phone's side (ZApp's, and the tests' fake phone).
pub fn phone_keys(p: &StaticSecret, e_p: &StaticSecret, k: &[u8; 32], e_k: &[u8; 32], psk: &[u8; 16]) -> Option<Keys> {
    let th = transcript(PublicKey::from(e_p).as_bytes(), PublicKey::from(p).as_bytes(), k, e_k);
    Some(expand(psk, [&*dh(e_p, k)?, &*dh(p, k)?, &*dh(e_p, e_k)?], &th))
}

/// One direction of records: a key and its counter.
pub struct Cipher {
    aead: ChaCha20Poly1305,
    counter: u64,
}

impl Cipher {
    pub fn new(key: &[u8; 32]) -> Self {
        Cipher { aead: ChaCha20Poly1305::new(key.into()), counter: 0 }
    }

    fn nonce(&mut self) -> [u8; 12] {
        let mut n = [0u8; 12];
        n[4..].copy_from_slice(&self.counter.to_be_bytes());
        self.counter += 1;
        n
    }

    /// `plaintext` (at most [`MAX_RECORD`] bytes) as one record: length, then ciphertext.
    pub fn seal(&mut self, plaintext: &[u8]) -> Vec<u8> {
        let nonce = self.nonce();
        let ct = self.aead.encrypt(&nonce.into(), plaintext).expect("encrypts");
        let mut out = Vec::with_capacity(2 + ct.len());
        out.extend_from_slice(&(ct.len() as u16).to_be_bytes());
        out.extend_from_slice(&ct);
        out
    }

    /// Every record needed for `bytes`, split at [`MAX_RECORD`].
    pub fn seal_all(&mut self, bytes: &[u8]) -> Vec<u8> {
        bytes.chunks(MAX_RECORD).flat_map(|c| self.seal(c)).collect()
    }

    /// One record's ciphertext (without its length).
    pub fn open(&mut self, ct: &[u8]) -> Option<Vec<u8>> {
        let nonce = self.nonce();
        self.aead.decrypt(&nonce.into(), ct).ok()
    }
}

/// Reads one record's ciphertext.
pub fn read_record(r: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 2];
    r.read_exact(&mut len)?;
    let len = u16::from_be_bytes(len) as usize;
    if !(TAG..=MAX_RECORD + TAG).contains(&len) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad record length"));
    }
    let mut ct = vec![0u8; len];
    r.read_exact(&mut ct)?;
    Ok(ct)
}

// ---- files in the data folder ----

/// Writes `bytes` readable by this user only.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut f = OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    std::fs::rename(tmp, path)
}

/// The Kindle's static key, made on first use (`link.key`, mode 600).
pub fn static_key(dir: &Path) -> io::Result<StaticSecret> {
    let path = dir.join("link.key");
    if let Ok(bytes) = std::fs::read(&path)
        && bytes.len() == 32
    {
        return Ok(StaticSecret::from(zfirm_core::arr::<32>(&bytes)));
    }
    let mut secret = Zeroizing::new([0u8; 32]);
    getrandom::getrandom(&mut *secret).map_err(|_| io::Error::other("no system randomness"))?;
    std::fs::create_dir_all(dir)?;
    write_private(&path, &*secret)?;
    Ok(StaticSecret::from(*secret))
}

pub fn public_key(dir: &Path) -> io::Result<[u8; 32]> {
    Ok(PublicKey::from(&static_key(dir)?).to_bytes())
}


pub fn b64(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

/// The pairing QR's text: `zfirm-kindle:?h=IP:7700&k=K&p=PSK&n=NAME`.
pub fn pair_uri(ip: std::net::Ipv4Addr, port: u16, k: &[u8; 32], psk: &[u8; 16], name: &str) -> String {
    let name: String = name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("zfirm-kindle:?h={ip}:{port}&k={}&p={}&n={name}", B64.encode(k), B64.encode(psk))
}

/// A fresh pairing secret, for one showing of the QR.
pub fn new_psk() -> [u8; 16] {
    let mut psk = [0u8; 16];
    getrandom::getrandom(&mut psk).expect("system randomness");
    psk
}

// ---- the server ----

/// What the link tells the main loop.
#[derive(Debug)]
pub enum LinkEvent {
    /// A phone finished the handshake. `known`: it is in `phones`; otherwise it used
    /// the QR's secret and is remembered only once the user allows it.
    Connected {
        phone: [u8; 32],
        known: bool,
    },
    /// Decrypted bytes of the zfirm frame stream.
    Data(Vec<u8>),
    Closed,
}

struct Conn {
    id: u64,
    stream: TcpStream,
    out: Cipher,
}

struct Shared {
    key: StaticSecret,
    psk: Mutex<Option<[u8; 16]>>,
    conn: Mutex<Option<Conn>>,
    stop: AtomicBool,
    next_id: AtomicU64,
    handshakes: AtomicUsize,
}

/// Listens for one phone at a time; a phone that completes the handshake replaces
/// the one before (the same phone back after its Wi-Fi dropped, usually).
pub struct Server {
    shared: Arc<Shared>,
}

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Handshakes in progress at once; more connections are dropped.
const MAX_HANDSHAKES: usize = 4;

impl Server {
    /// Serves on `listener`, with the key in `dir`; events go to `tx`.
    pub fn start<E: From<LinkEvent> + Send + 'static>(
        listener: TcpListener,
        dir: &Path,
        tx: Sender<E>,
    ) -> io::Result<Server> {
        // Phones remembered by older versions: none is now.
        let _ = std::fs::remove_file(dir.join("phones"));
        let shared = Arc::new(Shared {
            key: static_key(dir)?,
            psk: Mutex::new(None),
            conn: Mutex::new(None),
            stop: AtomicBool::new(false),
            next_id: AtomicU64::new(1),
            handshakes: AtomicUsize::new(0),
        });
        listener.set_nonblocking(true)?;
        let s = shared.clone();
        std::thread::spawn(move || {
            while !s.stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, from)) => {
                        if s.handshakes.fetch_add(1, Ordering::SeqCst) >= MAX_HANDSHAKES {
                            s.handshakes.fetch_sub(1, Ordering::SeqCst);
                            continue;
                        }
                        eprintln!("link: connection from {from}");
                        let (s, tx) = (s.clone(), tx.clone());
                        std::thread::spawn(move || serve(s, stream, tx));
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
                    Err(e) => {
                        eprintln!("link: accept: {e}");
                        std::thread::sleep(Duration::from_millis(500));
                    }
                }
            }
        });
        Ok(Server { shared })
    }

    /// The QR's secret while the QR is on screen, `None` once it is gone.
    pub fn set_psk(&self, psk: Option<[u8; 16]>) {
        *self.shared.psk.lock().unwrap() = psk;
    }

    /// Plaintext for the phone (frames), in as many records as it takes.
    pub fn send(&self, bytes: &[u8]) {
        let mut conn = self.shared.conn.lock().unwrap();
        if let Some(c) = conn.as_mut() {
            let records = c.out.seal_all(bytes);
            if c.stream.write_all(&records).is_err() {
                let _ = c.stream.shutdown(Shutdown::Both);
            }
        }
    }

    /// Drops the phone; its reading thread then reports [`LinkEvent::Closed`].
    pub fn hangup(&self) {
        if let Some(c) = self.shared.conn.lock().unwrap().as_ref() {
            let _ = c.stream.shutdown(Shutdown::Both);
        }
    }

    /// Stops listening and drops the phone.
    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        *self.shared.psk.lock().unwrap() = None;
        self.hangup();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

/// One connection: the handshake, then its records until it closes.
fn serve<E: From<LinkEvent>>(s: Arc<Shared>, mut stream: TcpStream, tx: Sender<E>) {
    let accepted = handshake(&s, &mut stream);
    s.handshakes.fetch_sub(1, Ordering::SeqCst);
    let Some((phone, known, keys, first)) = accepted else {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    let id = s.next_id.fetch_add(1, Ordering::SeqCst);
    let Ok(writer) = stream.try_clone() else { return };
    {
        // Events in order: the phone before it closes, then this one.
        let mut conn = s.conn.lock().unwrap();
        if s.stop.load(Ordering::SeqCst) {
            let _ = stream.shutdown(Shutdown::Both);
            return;
        }
        if let Some(old) = conn.take() {
            let _ = old.stream.shutdown(Shutdown::Both);
            let _ = tx.send(LinkEvent::Closed.into());
        }
        eprintln!("link: phone {} connected ({})", b64(&phone), if known { "known" } else { "pairing" });
        let _ = tx.send(LinkEvent::Connected { phone, known }.into());
        let _ = tx.send(LinkEvent::Data(first).into());
        *conn = Some(Conn { id, stream: writer, out: Cipher::new(&keys.k_kp) });
    }
    let _ = stream.set_read_timeout(None);
    let mut input = Cipher::new(&keys.k_pk);
    input.counter = 1;
    loop {
        let plain = read_record(&mut stream).ok().and_then(|ct| input.open(&ct));
        let Some(plain) = plain else { break };
        if tx.send(LinkEvent::Data(plain).into()).is_err() {
            break;
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
    let mut conn = s.conn.lock().unwrap();
    if conn.as_ref().is_some_and(|c| c.id == id) {
        *conn = None;
        eprintln!("link: phone gone");
        let _ = tx.send(LinkEvent::Closed.into());
    }
}

/// Returns the phone's key, whether it is known, the keys and its first record's plaintext.
fn handshake(s: &Shared, stream: &mut TcpStream) -> Option<([u8; 32], bool, Keys, Vec<u8>)> {
    // Blocking, with timeouts (on macOS an accepted socket keeps the listener's
    // non-blocking mode).
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT)).ok()?;
    let _ = stream.set_nodelay(true);
    let mut hello = [0u8; 65];
    stream.read_exact(&mut hello).ok()?;
    if hello[0] != HELLO_PHONE {
        return None;
    }
    let e_p: [u8; 32] = zfirm_core::arr(&hello[1..33]);
    let p: [u8; 32] = zfirm_core::arr(&hello[33..65]);
    let mut e_secret = Zeroizing::new([0u8; 32]);
    getrandom::getrandom(&mut *e_secret).ok()?;
    let e_k = StaticSecret::from(*e_secret);
    let mut reply = [0u8; 33];
    reply[0] = HELLO_KINDLE;
    reply[1..].copy_from_slice(PublicKey::from(&e_k).as_bytes());
    stream.write_all(&reply).ok()?;
    let first = read_record(stream).ok()?;

    // Only through the QR code on screen: every connection is a scan and an "Allow"
    // on the Kindle. No phone is remembered, so a restart of either side never leaves
    // a half-known link behind.
    let qr = *s.psk.lock().unwrap();
    if let Some(psk) = qr {
        let keys = kindle_keys(&s.key, &e_k, &e_p, &p, &psk)?;
        if let Some(plain) = Cipher::new(&keys.k_pk).open(&first) {
            return Some((p, false, keys, plain));
        }
    }
    eprintln!("link: handshake refused (no QR code on screen, or not its code)");
    None
}
