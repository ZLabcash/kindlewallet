//! KindleWallet on the Kindle: takes over the screen and the touch panel, draws each
//! screen as a PNG with the Kindle's `eips`, and gives everything back on exit.
//!
//! `kindlewallet --preview OUT_DIR` instead plays a scripted tour on the desktop and
//! saves every screen as a PNG (600×800, a Kindle 10th generation's screen).

use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
};

use kindlewallet::{
    app::{Action, App, Detail},
    coins::Coin,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--preview") {
        preview(Path::new(args.get(2).map_or("preview", String::as_str)));
        return;
    }
    let dir = std::env::var("KINDLEWALLET_DIR").map_or_else(|_| PathBuf::from("/mnt/us/kindlewallet"), PathBuf::from);
    kindle(dir);
}

fn preview(out: &Path) {
    std::fs::create_dir_all(out).unwrap();
    let dir = out.join("data");
    let _ = std::fs::remove_dir_all(&dir);
    // The whitepapers built by papers/build.py, when there.
    let papers = Path::new("kindle/kindlewallet/papers");
    if papers.is_dir() {
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::os::unix::fs::symlink(std::fs::canonicalize(papers).unwrap(), dir.join("papers"));
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
    for coin in kindlewallet::coins::Coin::ALL {
        app.act(Action::Open(coin));
        shot(&mut app, coin.ticker());
        app.act(Action::NewAddress);
        shot(&mut app, &format!("{}_next", coin.ticker()));
        let details: &[(Detail, &str)] = match coin {
            Coin::Zcash => &[(Detail::ZcashViewKey, "viewkey")],
            Coin::Bitcoin => &[(Detail::Zpub, "zpub")],
            Coin::Monero => &[(Detail::MoneroViewKey, "viewkey"), (Detail::MoneroWords, "words")],
            _ => &[],
        };
        for &(detail, name) in details {
            app.act(Action::Show(detail));
            shot(&mut app, &format!("{}_{name}", coin.ticker()));
            app.act(Action::Back);
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
    app.act(Action::ShowWords);
    shot(&mut app, "recovery_words");
    app.act(Action::Home);
    app.act(Action::Lock);
    shot(&mut app, "locked");
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

    /// Blocks until a finger lifts; returns where it was, in screen pixels.
    fn tap(&mut self, w: usize, h: usize) -> Option<(usize, usize)> {
        // struct input_event: the kernel's timeval (two longs), u16 type, u16 code, i32 value.
        const TIME: usize = 2 * std::mem::size_of::<libc::c_long>();
        let mut buf = [0u8; TIME + 8];
        let (mut x, mut y) = (-1i32, -1i32);
        loop {
            self.file.read_exact(&mut buf).ok()?;
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
        eprintln!("tap raw {x},{y} -> {fx:.2},{fy:.2}");
        let (sx, sy) = ((fx * w as f32) as usize, (fy * h as f32) as usize);
        Some((sx.min(w - 1), sy.min(h - 1)))
    }
}

/// What wakes the main loop: a tap, or the Kindle going to sleep or waking.
enum Event {
    Tap(usize, usize),
    Sleep,
    Wake,
    TouchGone,
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
    let mut app = App::new(dir);
    match touch {
        None => eprintln!("no touch panel found"),
        Some(mut touch) => {
            let (tx, rx) = mpsc::channel();
            let power = tx.clone();
            std::thread::spawn(move || power_events(power));
            std::thread::spawn(move || {
                loop {
                    let event = match touch.tap(w, h) {
                        Some((x, y)) => Event::Tap(x, y),
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
                    Ok(Event::Tap(x, y)) => app.tap(x, y),
                    Ok(Event::Sleep) => app.lock_if_open(),
                    Ok(Event::Wake) => {}
                    Ok(Event::TouchGone) | Err(_) => {
                        eprintln!("touch panel lost");
                        break;
                    }
                }
                if !app.quit {
                    show(&mut app, w, h);
                    eprintln!("drawn {}", app.debug_screen());
                }
            }
        }
    }
    sh("eips -c");
    sh("lipc-set-prop com.lab126.powerd preventScreenSaver 0");
    sh("start lab126_gui");
}
