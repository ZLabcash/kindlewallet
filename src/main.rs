//! KindleWallet on the Kindle: takes over the screen and the touch panel, draws each
//! screen as a PNG with the Kindle's `eips`, and gives everything back on exit.
//!
//! `kindlewallet --preview OUT_DIR` instead plays a scripted tour on the desktop and
//! saves every screen as a PNG (600×800, a Kindle 10th generation's screen).
//!
//! `kindlewallet --serve DIR [--port 7700] [--bind IP] [--pin PIN] [--auto-approve]` runs the
//! phone link headless on a computer, with a test wallet in DIR, so ZApp can be
//! tried against it: it prints the pairing QR's text, saves the screen as
//! DIR/screen.png, and asks on the terminal (or answers yes by itself).

use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    net::{Ipv4Addr, TcpListener},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};

use kindlewallet::{
    app::{Action, App, Detail, Effect, NftPlace, Prompt, Secret},
    coins::Coin,
    link::{self, LinkEvent, Server},
    wifi,
};
use zfirm_core::{protocol, session::Show, sign};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--preview") {
        preview(Path::new(args.get(2).map_or("preview", String::as_str)));
        return;
    }
    if args.get(1).map(String::as_str) == Some("--serve") {
        if let Err(e) = serve(&args[2..]) {
            eprintln!("kindlewallet --serve: {e}");
            eprintln!("usage: kindlewallet --serve DIR [--port 7700] [--bind IP] [--pin PIN] [--auto-approve]");
            std::process::exit(2);
        }
        return;
    }
    let dir = std::env::var("KINDLEWALLET_DIR").map_or_else(|_| PathBuf::from("/mnt/us/kindlewallet"), PathBuf::from);
    kindle(dir);
}

fn preview(out: &Path) {
    std::fs::create_dir_all(out).unwrap();
    let dir = out.join("data");
    let _ = std::fs::remove_dir_all(&dir);
    // The whitepapers built by papers/build.py and the portraits by nft/build.py, when there.
    for name in ["papers", "nft"] {
        let built = Path::new("kindle/kindlewallet").join(name);
        if built.is_dir() {
            std::fs::create_dir_all(&dir).unwrap();
            let _ = std::os::unix::fs::symlink(std::fs::canonicalize(&built).unwrap(), dir.join(name));
        }
    }
    let mut app = App::new(dir);
    let mut n = 0;
    let mut shot = |app: &mut App, name: &str| {
        n += 1;
        std::fs::write(out.join(format!("{n:02}_{name}.png")), app.draw(600, 800).png()).unwrap();
    };
    shot(&mut app, "welcome");
    app.act(Action::Create);
    shot(&mut app, "words");
    app.act(Action::WroteThem);
    for _ in 0..2 {
        for d in [4, 8, 2, 9, 1, 3] {
            app.act(Action::Digit(d));
        }
        shot(&mut app, "pin");
        app.act(Action::PinOk);
    }
    shot(&mut app, "home");
    let pin = |app: &mut App| {
        for d in [4, 8, 2, 9, 1, 3] {
            app.act(Action::Digit(d));
        }
    };
    for coin in kindlewallet::coins::Coin::ALL {
        app.act(Action::Open(coin));
        shot(&mut app, coin.ticker());
        app.act(Action::Receive);
        shot(&mut app, &format!("{}_receive", coin.ticker()));
        if coin == Coin::Zcash {
            app.act(Action::NewAddress);
            shot(&mut app, "ZEC_next");
        }
        app.act(Action::Back);
        if coin == Coin::Ethereum {
            app.act(Action::Networks);
            shot(&mut app, "ETH_networks");
            app.act(Action::NetworksPage(1));
            shot(&mut app, "ETH_networks_2");
            app.act(Action::OpenNetwork(8));
            shot(&mut app, "ETH_network_avalanche");
            app.act(Action::NftHome(NftPlace::Evm(8)));
            shot(&mut app, "ETH_network_nfts");
            app.act(Action::Back);
            app.act(Action::Back);
            app.act(Action::Back);
        }
        if coin.has_nfts() && coin != Coin::Ethereum {
            app.act(Action::NftHome(NftPlace::Coin(coin)));
            shot(&mut app, &format!("{}_nfts", coin.ticker()));
            app.act(Action::Back);
        }
        let details: &[(Detail, &str)] = match coin {
            Coin::Zcash => &[(Detail::ZcashViewKey, "viewkey")],
            Coin::Bitcoin => &[(Detail::Zpub, "zpub")],
            Coin::Monero => &[(Detail::MoneroViewKey, "viewkey")],
            _ => &[],
        };
        for &(detail, name) in details {
            app.act(Action::Show(detail));
            shot(&mut app, &format!("{}_{name}", coin.ticker()));
            app.act(Action::Back);
        }
        if coin == Coin::Monero {
            app.act(Action::Reveal(Secret::MoneroWords));
            shot(&mut app, "XMR_words_pin");
            pin(&mut app);
            app.act(Action::PinOk);
            shot(&mut app, "XMR_words");
            app.act(Action::Back);
        }
        if coin == Coin::Zcash {
            preview_link(&mut app, &mut shot);
        }
        if coin == Coin::Bitcoin {
            app.act(Action::Paper);
            shot(&mut app, "BTC_paper");
            app.act(Action::PageTurn(true));
            shot(&mut app, "BTC_paper_2");
            app.act(Action::Back);
        }
        app.act(Action::Home);
    }
    app.act(Action::Settings);
    shot(&mut app, "settings");
    app.act(Action::Reveal(Secret::Recovery));
    for d in [1, 1, 1, 1, 1, 1] {
        app.act(Action::Digit(d));
    }
    app.act(Action::PinOk);
    shot(&mut app, "recovery_wrong_pin");
    pin(&mut app);
    app.act(Action::PinOk);
    shot(&mut app, "recovery_words");
    app.act(Action::Back);
    app.act(Action::Back);
    app.act(Action::Lock);
    shot(&mut app, "locked");
}

/// The phone link's screens, with a phone played by the preview: pairing, a
/// transaction to review and sign (an example, not checked), the zkSNARKs gallery.
fn preview_link(app: &mut App, shot: &mut impl FnMut(&mut App, &str)) {
    app.act(Action::ConnectPhone);
    shot(app, "ZEC_link_wifi");
    app.link_up(Ipv4Addr::new(192, 168, 1, 23));
    shot(app, "ZEC_link_qr");
    app.link_event(LinkEvent::Connected { phone: [9; 32], known: false });
    app.link_event(LinkEvent::Data(protocol::encode(protocol::msg::PING, &[])));
    app.link_event(LinkEvent::Data(protocol::encode(protocol::msg::HELLO, &[])));
    shot(app, "ZEC_pair_prompt");
    app.act(Action::Allow);
    shot(app, "ZEC_link_connected");
    app.act(Action::Back);
    shot(app, "ZEC_phone_connected");
    app.act(Action::LinkScreen);
    app.preview_show(Show::Busy("Checking transaction"));
    shot(app, "ZEC_busy");
    let review = sign::Review {
        payments: vec![sign::Payment {
            address: kindlewallet::coins::zcash(&[7; 64], 0).unwrap_or_default(),
            value: 25_000_000,
            ironwood: false,
            memo: Some("Thanks for the coffee".into()),
        }],
        change: 74_990_000,
        spent: 100_000_000,
        fee: 10_000,
        spends: 1,
        ..sign::Review::default()
    };
    let pages = review.pages();
    app.preview_show(Show::Review(review));
    for page in 1..=pages {
        shot(app, &format!("ZEC_review_{page}"));
        app.act(Action::ReviewPage(true));
    }
    // A tap on "Hold to sign" is not enough.
    let (w, h, s) = (600, 800, 3);
    app.press(w / 2, h - 84 * s + 13 * s, 0.2);
    shot(app, "ZEC_review_tap");
    app.preview_show(Show::Busy("Signing"));
    shot(app, "ZEC_signing");
    app.preview_show(Show::Done { ok: true, note: "Signed: phone sends it" });
    shot(app, "ZEC_done");
    app.act(Action::DoneOk);
    let mut nfts = vec![3u8, 0];
    for t in [1u16, 42, 7777] {
        nfts.extend_from_slice(&t.to_le_bytes());
    }
    app.link_event(LinkEvent::Data(protocol::encode(link::msg::NFTS, &nfts)));
    app.act(Action::Back);
    app.act(Action::Nfts);
    shot(app, "ZEC_nfts");
    app.act(Action::NftPage(1));
    shot(app, "ZEC_nfts_2");
    app.act(Action::NftTab { mine: true });
    shot(app, "ZEC_nfts_mine");
    app.act(Action::NftOpen(42));
    shot(app, "ZEC_nft");
    app.act(Action::Back);
    app.act(Action::Back);
    app.take_effects();
}

// ---- the desktop server ----

/// What wakes the server: the link, or an answer typed on the terminal.
enum Served {
    Link(LinkEvent),
    Answer(bool),
}

impl From<LinkEvent> for Served {
    fn from(e: LinkEvent) -> Self {
        Served::Link(e)
    }
}

fn serve(args: &[String]) -> Result<(), String> {
    let mut it = args.iter();
    let dir = PathBuf::from(it.next().ok_or("no DIR")?);
    let (mut port, mut pin, mut auto) = (link::PORT, "123456".to_string(), false);
    let mut bind = Ipv4Addr::UNSPECIFIED;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--port" => port = it.next().and_then(|p| p.parse().ok()).ok_or("--port needs a number")?,
            "--bind" => bind = it.next().and_then(|b| b.parse().ok()).ok_or("--bind needs an IPv4 address")?,
            "--pin" => pin = it.next().ok_or("--pin needs a value")?.clone(),
            "--auto-approve" => auto = true,
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dir = std::fs::canonicalize(&dir).map_err(|e| e.to_string())?;
    if dir.starts_with("/mnt/us") {
        return Err("a test server never runs on the Kindle's own wallet".into());
    }
    if !dir.join("wallet.zkv").exists() {
        // The BIP 39 test phrase: a wallet anyone can recreate, for tests only.
        let phrase = ["abandon"; 23].join(" ") + " art";
        std::fs::write(dir.join("wallet.zkv"), kindlewallet::vault::seal(&phrase, &pin)).map_err(|e| e.to_string())?;
        eprintln!("made a test wallet in {} (phrase \"abandon x23 art\", PIN {pin})", dir.display());
    }
    let mut app = App::new(dir.clone());
    app.port = port;
    for d in pin.bytes().filter(u8::is_ascii_digit) {
        app.act(Action::Digit(d - b'0'));
    }
    app.act(Action::PinOk);
    app.act(Action::Open(Coin::Zcash));
    if !app.debug_screen().starts_with("Coin") {
        return Err("wrong PIN for the wallet in DIR".into());
    }
    let (tx, rx) = mpsc::channel::<Served>();
    if !auto {
        let answers = tx.clone();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                let yes = matches!(line.trim(), "y" | "yes");
                if answers.send(Served::Answer(yes)).is_err() {
                    break;
                }
            }
        });
    }
    app.act(Action::ConnectPhone);
    let mut server: Option<Server> = None;
    let (mut shown, mut uri) = (String::new(), None);
    let mut asked = None;
    loop {
        // Effects may bring more (the link up puts the QR's secret on).
        let mut effects = app.take_effects();
        while !effects.is_empty() {
            for effect in effects {
                match effect {
                    Effect::LinkOn => {
                        let listener = TcpListener::bind((bind, port)).map_err(|e| format!("port {port}: {e}"))?;
                        server = Some(Server::start(listener, &dir, tx.clone()).map_err(|e| e.to_string())?);
                        let ip = if bind.is_unspecified() {
                            wifi::ipv4(None).map_or(Ipv4Addr::LOCALHOST, |(ip, _)| ip)
                        } else {
                            bind
                        };
                        app.link_up(ip);
                    }
                    Effect::LinkOff => server = None,
                    Effect::Psk(psk) => server.iter().for_each(|s| s.set_psk(psk)),
                    Effect::Send(bytes) => {
                        eprintln!("to phone: frame 0x{:02x}, {} bytes", bytes.get(2).copied().unwrap_or(0), bytes.len());
                        server.iter().for_each(|s| s.send(&bytes));
                    }
                    Effect::Hangup => server.iter().for_each(Server::hangup),
                }
            }
            effects = app.take_effects();
        }
        let screen = app.debug_screen();
        if screen != shown {
            let _ = std::fs::write(dir.join("screen.png"), app.draw(600, 800).png());
            eprintln!("screen: {screen}");
            shown = screen;
        }
        if app.pair_uri() != uri {
            uri = app.pair_uri();
            if let Some(u) = &uri {
                println!("pairing QR: {u}");
            }
        }
        if app.has_work() {
            app.run_work();
            continue;
        }
        if app.debug_screen().starts_with("Done") {
            app.act(Action::DoneOk);
            continue;
        }
        let prompt = app.prompt();
        if prompt.is_some() && auto {
            answer(&mut app, true);
            continue;
        }
        if prompt != asked {
            asked = prompt;
            match prompt {
                Some(Prompt::Pair) => {
                    println!("ZApp wants this wallet's viewing key. Allow? [y/N]")
                }
                Some(Prompt::Sign) => {
                    if let Some(r) = app.review() {
                        for p in &r.payments {
                            println!(
                                "send {} to {}{}",
                                kindlewallet::app::zec(p.value),
                                p.address,
                                p.memo.as_ref().map_or(String::new(), |m| format!(" memo {m:?}"))
                            );
                        }
                        println!(
                            "fee {}, total {}. Sign? [y/N]",
                            kindlewallet::app::zec(r.fee),
                            kindlewallet::app::zec(r.total_out())
                        );
                    }
                }
                None => {}
            }
        }
        match rx.recv() {
            Ok(Served::Link(e)) => {
                if let LinkEvent::Data(bytes) = &e {
                    eprintln!("from phone: {} bytes", bytes.len());
                }
                app.link_event(e)
            }
            Ok(Served::Answer(yes)) if prompt.is_some() => answer(&mut app, yes),
            Ok(Served::Answer(_)) => {}
            Err(_) => return Ok(()),
        }
    }
}

/// Answers the prompt on screen as a finger would (a hold, for signing).
fn answer(app: &mut App, yes: bool) {
    match (app.prompt(), yes) {
        (Some(Prompt::Pair), true) => app.act(Action::Allow),
        (Some(Prompt::Sign), true) => {
            let pages = app.review().map_or(1, |r| r.pages());
            for _ in 1..pages {
                app.act(Action::ReviewPage(true));
            }
            app.act(Action::Sign);
        }
        (Some(_), false) => app.act(Action::Refuse),
        (None, _) => {}
    }
}

// ---- on the Kindle ----

fn sh(cmd: &str) {
    let _ = Command::new("/bin/sh").arg("-c").arg(cmd).status();
}

fn screen_size() -> (usize, usize) {
    // fb_var_screeninfo starts with xres, yres (u32).
    const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;
    if let Ok(fb) = File::open("/dev/fb0") {
        let mut info = [0u32; 40];
        if unsafe { libc::ioctl(fb.as_raw_fd(), FBIOGET_VSCREENINFO as _, info.as_mut_ptr()) } == 0 && info[0] > 0 {
            return (info[0] as usize, info[1] as usize);
        }
    }
    (600, 800)
}

fn show(app: &mut App, w: usize, h: usize) {
    let path = "/tmp/kindlewallet.png";
    if std::fs::write(path, app.draw(w, h).png()).is_ok() {
        sh(&format!("eips -f -g {path} >/dev/null 2>&1"));
    }
}

/// The touch panel: the input device reporting absolute multitouch positions.
struct Touch {
    file: File,
    max_x: i32,
    max_y: i32,
    /// From `touch.conf` (words `swap`, `invx`, `invy`), for panels mounted turned.
    swap: bool,
    inv_x: bool,
    inv_y: bool,
}

const EV_KEY: u16 = 1;
const EV_ABS: u16 = 3;
const BTN_TOUCH: u16 = 0x14a;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;

fn ioc(dir: u32, ty: u8, nr: u8, size: usize) -> libc::c_ulong {
    ((dir << 30) | ((size as u32) << 16) | ((ty as u32) << 8) | nr as u32) as libc::c_ulong
}

fn abs_max(fd: i32, code: u16) -> Option<i32> {
    let mut info = [0i32; 6]; // value, minimum, maximum, fuzz, flat, resolution
    let req = ioc(2, b'E', 0x40 + code as u8, 24);
    (unsafe { libc::ioctl(fd, req as _, info.as_mut_ptr()) } == 0 && info[2] > 0).then_some(info[2])
}

impl Touch {
    fn open(dir: &Path) -> Option<Touch> {
        let conf = std::fs::read_to_string(dir.join("touch.conf")).unwrap_or_default();
        let has = |w: &str| conf.split_whitespace().any(|c| c == w);
        for n in 0..8 {
            let Ok(file) = File::open(format!("/dev/input/event{n}")) else { continue };
            let fd = file.as_raw_fd();
            let (Some(max_x), Some(max_y)) = (abs_max(fd, ABS_MT_POSITION_X), abs_max(fd, ABS_MT_POSITION_Y)) else {
                continue;
            };
            // Ours alone while KindleWallet runs.
            unsafe { libc::ioctl(fd, ioc(1, b'E', 0x90, 4) as _, 1i32) };
            eprintln!("touch: event{n}, max {max_x}x{max_y}, conf {conf:?}");
            return Some(Touch { file, max_x, max_y, swap: has("swap"), inv_x: has("invx"), inv_y: has("invy") });
        }
        None
    }

    /// Blocks until a finger lifts; returns where it was, in screen pixels, and how
    /// long it stayed (seconds).
    fn tap(&mut self, w: usize, h: usize) -> Option<(usize, usize, f32)> {
        // struct input_event: the kernel's timeval (two longs), u16 type, u16 code, i32 value.
        const TIME: usize = 2 * std::mem::size_of::<libc::c_long>();
        let mut buf = [0u8; TIME + 8];
        let (mut x, mut y) = (-1i32, -1i32);
        // From the gesture's first event (the finger down).
        let mut down: Option<Instant> = None;
        loop {
            self.file.read_exact(&mut buf).ok()?;
            down.get_or_insert_with(Instant::now);
            let ty = u16::from_ne_bytes([buf[TIME], buf[TIME + 1]]);
            let code = u16::from_ne_bytes([buf[TIME + 2], buf[TIME + 3]]);
            let value = i32::from_ne_bytes([buf[TIME + 4], buf[TIME + 5], buf[TIME + 6], buf[TIME + 7]]);
            match (ty, code) {
                (EV_ABS, ABS_MT_POSITION_X | ABS_X) => x = value,
                (EV_ABS, ABS_MT_POSITION_Y | ABS_Y) => y = value,
                (EV_ABS, ABS_MT_TRACKING_ID) if value == -1 && x >= 0 => break,
                (EV_KEY, BTN_TOUCH) if value == 0 && x >= 0 => break,
                _ => {}
            }
        }
        let (mut fx, mut fy) = (x as f32 / (self.max_x + 1) as f32, y as f32 / (self.max_y + 1) as f32);
        if self.swap {
            (fx, fy) = (fy, fx);
        }
        if self.inv_x {
            fx = 1.0 - fx;
        }
        if self.inv_y {
            fy = 1.0 - fy;
        }
        let held = down.map_or(0.0, |d| d.elapsed().as_secs_f32());
        eprintln!("tap raw {x},{y} -> {fx:.2},{fy:.2}, held {held:.1} s");
        let (sx, sy) = ((fx * w as f32) as usize, (fy * h as f32) as usize);
        Some((sx.min(w - 1), sy.min(h - 1), held))
    }
}

/// What wakes the main loop: a tap (and how long the finger stayed), the Kindle
/// going to sleep or waking, or the phone link.
enum Event {
    Tap(usize, usize, f32),
    Sleep,
    Wake,
    TouchGone,
    Link(LinkEvent),
    /// The link started in round `round` is listening at `ip`.
    LinkReady {
        round: u64,
        server: Server,
        ip: Ipv4Addr,
    },
    LinkFailed {
        round: u64,
        why: String,
    },
}

impl From<LinkEvent> for Event {
    fn from(e: LinkEvent) -> Self {
        Event::Link(e)
    }
}

/// The link's Wi-Fi, firewall and server on the Kindle. Each start is a new
/// round, so a late answer from one stopped meanwhile is told apart.
struct Net {
    dir: PathBuf,
    round: u64,
    cancel: Arc<AtomicBool>,
    server: Option<Server>,
    /// Wi-Fi was switched on at some point: put things back on exit.
    used: bool,
    /// A teardown running in the background: a new start waits for it.
    tearing: Option<std::thread::JoinHandle<()>>,
}

impl Net {
    fn start(&mut self, tx: mpsc::Sender<Event>) {
        // A start under way is dropped; Wi-Fi and firewall stay as they are (the new
        // round shuts and opens them again), so the Wi-Fi does not go off and on.
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(t) = self.tearing.take() {
            let _ = t.join();
        }
        if let Some(server) = self.server.take() {
            server.stop();
        }
        self.round += 1;
        self.used = true;
        self.cancel = Arc::new(AtomicBool::new(false));
        let (round, cancel, dir) = (self.round, self.cancel.clone(), self.dir.clone());
        std::thread::spawn(move || {
            let event = match bring_up(&cancel, &dir, tx.clone()) {
                Ok((server, ip)) => Event::LinkReady { round, server, ip },
                Err(why) => Event::LinkFailed { round, why },
            };
            let _ = tx.send(event);
        });
    }

    /// Stops the link and waits for Wi-Fi off and the firewall put back (on exit).
    fn stop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(t) = self.tearing.take() {
            let _ = t.join();
        }
        if let Some(server) = self.server.take() {
            server.stop();
        }
        if self.used {
            wifi::teardown();
        }
    }

    /// Stops the link; Wi-Fi off and the firewall put back in the background, so the
    /// screen answers at once ("Stop link" took seconds of shell commands before).
    fn stop_soon(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(server) = self.server.take() {
            server.stop();
        }
        if self.used {
            self.tearing = Some(std::thread::spawn(wifi::teardown));
        }
    }

    fn effects(&mut self, app: &mut App, tx: &mpsc::Sender<Event>) {
        let mut effects = app.take_effects();
        while !effects.is_empty() {
            for effect in effects {
                match effect {
                    Effect::LinkOn => self.start(tx.clone()),
                    Effect::LinkOff => self.stop_soon(),
                    Effect::Psk(psk) => self.server.iter().for_each(|s| s.set_psk(psk)),
                    Effect::Send(bytes) => self.server.iter().for_each(|s| s.send(&bytes)),
                    Effect::Hangup => self.server.iter().for_each(Server::hangup),
                }
            }
            effects = app.take_effects();
        }
    }
}

/// The firewall shut, Wi-Fi on, an address, the link let through, then listening.
fn bring_up(cancel: &AtomicBool, dir: &Path, tx: mpsc::Sender<Event>) -> Result<(Server, Ipv4Addr), String> {
    if !wifi::lockdown(cancel) {
        return Err("The firewall could not be set: Wi-Fi stays off".into());
    }
    let (ip, mask) = wifi::join(cancel)?;
    if !wifi::firewall(ip, mask, link::PORT, cancel) {
        return Err("The firewall could not be set: no link without it".into());
    }
    // Offline, checked: should anything reach the Internet, no link.
    if wifi::internet_reachable() {
        return Err("The Internet answered through the firewall: no link, Wi-Fi off".into());
    }
    let listener = TcpListener::bind(("0.0.0.0", link::PORT)).map_err(|e| format!("Port {}: {e}", link::PORT))?;
    let server = Server::start(listener, dir, tx).map_err(|e| e.to_string())?;
    Ok((server, ip))
}

/// Amazon's interface normally redraws the screen around sleep; with it stopped,
/// KindleWallet listens to the power daemon itself: it locks before sleeping (the
/// power button locks the wallet) and redraws on waking.
fn power_events(tx: mpsc::Sender<Event>) {
    let Ok(mut child) = Command::new("lipc-wait-event")
        .args(["-m", "com.lab126.powerd", "goingToScreenSaver,outOfScreenSaver,wakeupFromSuspend"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        eprintln!("no lipc-wait-event");
        return;
    };
    let Some(out) = child.stdout.take() else { return };
    for line in BufReader::new(out).lines().map_while(Result::ok) {
        eprintln!("power: {line}");
        let event = if line.starts_with("goingToScreenSaver") { Event::Sleep } else { Event::Wake };
        if tx.send(event).is_err() {
            break;
        }
    }
    let _ = child.kill();
}

fn kindle(dir: PathBuf) {
    // Amazon's interface off, the screensaver held back; both restored on exit.
    sh("lipc-set-prop com.lab126.powerd preventScreenSaver 1");
    sh("stop lab126_gui");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let (w, h) = screen_size();
    eprintln!("screen {w}x{h}");
    let touch = Touch::open(&dir);
    let mut net =
        Net { dir: dir.clone(), round: 0, cancel: Arc::new(AtomicBool::new(true)), server: None, used: false, tearing: None };
    let mut app = App::new(dir);
    match touch {
        None => eprintln!("no touch panel found"),
        Some(mut touch) => {
            let (tx, rx) = mpsc::channel();
            let power = tx.clone();
            std::thread::spawn(move || power_events(power));
            let link_tx = tx.clone();
            std::thread::spawn(move || {
                loop {
                    let event = match touch.tap(w, h) {
                        Some((x, y, held)) => Event::Tap(x, y, held),
                        None => Event::TouchGone,
                    };
                    let gone = matches!(event, Event::TouchGone);
                    if tx.send(event).is_err() || gone {
                        break;
                    }
                }
            });
            show(&mut app, w, h);
            while !app.quit {
                match rx.recv() {
                    Ok(Event::Tap(x, y, held)) => app.press(x, y, held),
                    Ok(Event::Sleep) => app.lock_if_open(),
                    Ok(Event::Wake) => {}
                    Ok(Event::Link(e)) => app.link_event(e),
                    Ok(Event::LinkReady { round, server, ip }) => {
                        if round == net.round && !net.cancel.load(Ordering::SeqCst) {
                            net.server = Some(server);
                            app.link_up(ip);
                        } else {
                            server.stop();
                        }
                    }
                    Ok(Event::LinkFailed { round, why }) => {
                        eprintln!("link failed: {why}");
                        if round == net.round && !net.cancel.load(Ordering::SeqCst) {
                            net.stop_soon();
                            app.link_failed(&why);
                        }
                    }
                    Ok(Event::TouchGone) | Err(_) => {
                        eprintln!("touch panel lost");
                        break;
                    }
                }
                net.effects(&mut app, &link_tx);
                if !app.quit {
                    show(&mut app, w, h);
                    eprintln!("drawn {}", app.debug_screen());
                }
                // What is running is on screen ("Checking...", "Signing..."): now run it.
                while app.has_work() && !app.quit {
                    app.run_work();
                    net.effects(&mut app, &link_tx);
                    show(&mut app, w, h);
                    eprintln!("drawn {}", app.debug_screen());
                }
            }
            app.act(Action::Exit);
            net.effects(&mut app, &link_tx);
            // "Closing": Amazon's interface takes a while to come back.
            show(&mut app, w, h);
        }
    }
    // Whatever happened, Wi-Fi off and the firewall as it was.
    net.stop();
    sh("lipc-set-prop com.lab126.powerd preventScreenSaver 0");
    eprintln!("starting Amazon's interface");
    sh("start lab126_gui");
    eprintln!("done");
}
