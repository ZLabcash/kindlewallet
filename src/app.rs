//! The screens and what a tap does, with no device I/O: the Kindle loop (main.rs)
//! and the desktop preview drive the same state machine.

use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use crate::{coins::Coin, logos, monero, render::Page, vault};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Create,
    WroteThem,
    Digit(u8),
    Erase,
    PinOk,
    Open(Coin),
    NewAddress,
    ShowWords,
    Show(Detail),
    Back,
    Paper,
    PageTurn(bool),
    Home,
    Lock,
    Exit,
}

/// What a coin's own screen can show besides its receiving address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    /// Zcash: the unified full viewing key.
    ZcashViewKey,
    /// Bitcoin: the BIP 84 account key.
    Zpub,
    /// Monero: the private view key, for a view-only wallet.
    MoneroViewKey,
    /// Monero: its own 25-word phrase.
    MoneroWords,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Screen {
    Welcome,
    /// The new phrase, before it is sealed.
    Words,
    /// Choosing a PIN; `first` holds the first entry while it is confirmed.
    NewPin { first: Option<String> },
    Unlock,
    Home,
    Receive(Coin),
    ShowWords,
    Detail(Coin, Detail),
    /// A coin's whitepaper, at page n (from 1).
    Paper(Coin, usize),
}

pub struct App {
    dir: PathBuf,
    screen: Screen,
    phrase: Option<Zeroizing<String>>,
    seed: Option<Zeroizing<[u8; 64]>>,
    pin: Zeroizing<String>,
    note: String,
    /// The receiving address shown for the open coin, and its index.
    address: Option<(u32, String)>,
    buttons: Vec<(Rect, Action)>,
    pub quit: bool,
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl Rect {
    fn contains(&self, x: usize, y: usize) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

const PIN_MIN: usize = 6;

impl App {
    pub fn new(dir: PathBuf) -> Self {
        let screen = if dir.join("wallet.zkv").exists() { Screen::Unlock } else { Screen::Welcome };
        App {
            dir,
            screen,
            phrase: None,
            seed: None,
            pin: Zeroizing::new(String::new()),
            note: String::new(),
            address: None,
            buttons: Vec::new(),
            quit: false,
        }
    }

    /// The current screen, for the log.
    pub fn debug_screen(&self) -> String {
        format!("{:?}", self.screen)
    }

    /// Locks an open wallet (the Kindle going to sleep); a new phrase not yet sealed
    /// by its PIN is left as it is.
    pub fn lock_if_open(&mut self) {
        if self.seed.is_some() {
            self.act(Action::Lock);
        }
    }

    pub fn tap(&mut self, x: usize, y: usize) {
        if let Some(&(_, action)) = self.buttons.iter().find(|(r, _)| r.contains(x, y)) {
            self.act(action);
        }
    }

    pub fn act(&mut self, action: Action) {
        self.note.clear();
        match action {
            Action::Create => {
                self.phrase = Some(Zeroizing::new(crate::new_phrase()));
                self.screen = Screen::Words;
            }
            Action::WroteThem => self.screen = Screen::NewPin { first: None },
            Action::Digit(d) => {
                if self.pin.len() < 16 {
                    self.pin.push((b'0' + d) as char);
                }
            }
            Action::Erase => {
                self.pin.pop();
            }
            Action::PinOk => self.pin_ok(),
            Action::Open(coin) => {
                let index = self.load_index(coin);
                self.show_address(coin, index);
            }
            Action::NewAddress => {
                if let Screen::Receive(coin) = self.screen {
                    let next = self.address.as_ref().map_or(0, |(j, _)| j + 1);
                    self.show_address(coin, next);
                }
            }
            Action::ShowWords => self.screen = Screen::ShowWords,
            Action::Show(detail) => {
                if let Screen::Receive(coin) = self.screen {
                    self.screen = Screen::Detail(coin, detail);
                }
            }
            Action::Back => {
                if let Screen::Detail(coin, _) | Screen::Paper(coin, _) = self.screen {
                    self.screen = Screen::Receive(coin);
                }
            }
            Action::Paper => {
                if let Screen::Receive(coin) = self.screen {
                    let page = std::fs::read_to_string(self.paper_mark(coin))
                        .ok()
                        .and_then(|s| s.trim().parse().ok())
                        .unwrap_or(1)
                        .clamp(1, self.paper_pages(coin).max(1));
                    self.screen = Screen::Paper(coin, page);
                }
            }
            Action::PageTurn(forward) => {
                if let Screen::Paper(coin, page) = self.screen {
                    let last = self.paper_pages(coin);
                    let page = if forward { (page + 1).min(last) } else { page.saturating_sub(1).max(1) };
                    let _ = std::fs::write(self.paper_mark(coin), page.to_string());
                    self.screen = Screen::Paper(coin, page);
                }
            }
            Action::Home => self.screen = Screen::Home,
            Action::Lock => {
                self.phrase = None;
                self.seed = None;
                self.address = None;
                self.screen = Screen::Unlock;
            }
            Action::Exit => self.quit = true,
        }
    }

    fn pin_ok(&mut self) {
        let pin = std::mem::replace(&mut self.pin, Zeroizing::new(String::new()));
        if pin.len() < PIN_MIN {
            self.note = format!("At least {PIN_MIN} digits");
            return;
        }
        match &self.screen {
            Screen::NewPin { first: None } => self.screen = Screen::NewPin { first: Some(pin.to_string()) },
            Screen::NewPin { first: Some(first) } => {
                if *first != *pin {
                    self.note = "The two PINs differ: again".into();
                    self.screen = Screen::NewPin { first: None };
                    return;
                }
                let Some(phrase) = self.phrase.clone() else { return };
                let sealed = vault::seal(&phrase, &pin);
                let _ = std::fs::create_dir_all(&self.dir);
                if std::fs::write(self.dir.join("wallet.zkv"), sealed).is_err() {
                    self.note = "Could not save the wallet".into();
                    return;
                }
                for coin in Coin::ALL {
                    let _ = std::fs::remove_file(self.index_path(coin));
                }
                self.unlocked(phrase);
            }
            Screen::Unlock => {
                let file = std::fs::read(self.dir.join("wallet.zkv")).unwrap_or_default();
                match vault::open(&file, &pin) {
                    Ok(phrase) => self.unlocked(phrase),
                    Err(vault::Error::WrongPin) => self.note = "Wrong PIN".into(),
                    Err(vault::Error::Corrupt) => self.note = "Wallet file damaged".into(),
                }
            }
            _ => {}
        }
    }

    fn unlocked(&mut self, phrase: Zeroizing<String>) {
        self.seed = Some(Zeroizing::new(crate::seed(&phrase)));
        self.phrase = Some(phrase);
        self.screen = Screen::Home;
    }

    fn paper_pages(&self, coin: Coin) -> usize {
        paper_pages(&self.dir, coin)
    }

    /// The page the reader was last on, per coin.
    fn paper_mark(&self, coin: Coin) -> PathBuf {
        self.dir.join(format!("page_{}", coin.ticker().to_lowercase()))
    }

    fn index_path(&self, coin: Coin) -> PathBuf {
        self.dir.join(format!("index_{}", coin.ticker().to_lowercase()))
    }

    /// Opening a coin hands out a fresh address: the one after the last shown.
    fn load_index(&self, coin: Coin) -> u32 {
        std::fs::read_to_string(self.index_path(coin))
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .map_or(0, |j| j.saturating_add(1))
    }

    fn show_address(&mut self, coin: Coin, index: u32) {
        let Some(seed) = &self.seed else { return };
        match coin.address(seed, index) {
            Some(a) => {
                let _ = std::fs::write(self.index_path(coin), index.to_string());
                self.address = Some((index, a));
                self.screen = Screen::Receive(coin);
            }
            None => self.note = "Could not derive the address".into(),
        }
    }

    // ---- drawing ----

    pub fn draw(&mut self, w: usize, h: usize) -> Page {
        let mut p = Page::new(w, h);
        self.buttons.clear();
        let s = (w / 200).max(2); // text scale: 3 on a 600 px wide Kindle
        let dir = self.dir.clone();
        let mut ui = Ui { p: &mut p, buttons: &mut self.buttons, s, w, h };
        ui.header();
        match self.screen.clone() {
            Screen::Welcome => {
                ui.p.text_centered(h / 5, "Offline wallet", s);
                ui.p.text_centered(h / 5 + 14 * s, "Zcash  Bitcoin  Ethereum", s.max(3) - 1);
                ui.p.wrapped(
                    8 * s,
                    h / 3,
                    "One phrase of 24 words, made here, never online. Write it on paper: it is your money.",
                    (w - 16 * s) / (8 * (s - 1).max(2)),
                    (s - 1).max(2),
                );
                ui.button(h * 3 / 5, "Create a wallet", Action::Create);
                ui.button(h * 3 / 5 + 26 * s, "Exit", Action::Exit);
            }
            Screen::Words | Screen::ShowWords => {
                let phrase = self.phrase.as_deref().cloned().unwrap_or_default();
                ui.p.text_centered(30 * s, "Your 24 words", s);
                let y = ui.numbered_words(46 * s, &phrase) + 8 * s;
                if self.screen == Screen::Words {
                    ui.button(y, "I wrote them down", Action::WroteThem);
                } else {
                    ui.button(y, "Back", Action::Home);
                }
            }
            Screen::NewPin { first } => {
                let title = if first.is_none() { "Choose a PIN" } else { "Same PIN again" };
                ui.pin_pad(title, self.pin.len());
            }
            Screen::Unlock => ui.pin_pad("Enter your PIN", self.pin.len()),
            Screen::Home => {
                // The logos, three to a row; the last row centred.
                let top = 26 * s;
                let tile_w = (w - 20 * s) / 3;
                let tile_h = ((h - top - 34 * s) / Coin::ALL.len().div_ceil(3)).min(tile_w);
                for (n, row) in Coin::ALL.chunks(3).enumerate() {
                    let x0 = 10 * s + (3 - row.len()) * tile_w / 2;
                    for (i, &coin) in row.iter().enumerate() {
                        let r = Rect { x: x0 + i * tile_w, y: top + n * tile_h, w: tile_w - 2 * s, h: tile_h - 2 * s };
                        ui.logo_tile(r, coin);
                    }
                }
                let y = h - 26 * s;
                ui.third_buttons(y, [("Words", Action::ShowWords), ("Lock", Action::Lock), ("Exit", Action::Exit)]);
            }
            Screen::Receive(coin) => {
                let Some((j, addr)) = self.address.clone() else { return p };
                let y = ui.coin_title(coin, &format!("Address #{j}"));
                let size = (w * 3 / 5).min(h / 2);
                ui.p.qr(w / 2, y, size, &addr);
                let ws = (s - 1).max(2);
                let cols = (w - 16 * s) / (8 * ws);
                let mut y = ui.p.wrapped(8 * s, y + size + 2 * s, &addr, cols, ws);
                if coin.chain_id().is_some() {
                    y = ui.p.wrapped(8 * s, y + 2 * s, "Same 0x address on every EVM network.", cols, ws);
                }
                let extras: &[(&str, Detail)] = match coin {
                    Coin::Zcash => &[("Viewing key", Detail::ZcashViewKey)],
                    Coin::Bitcoin => &[("zpub", Detail::Zpub)],
                    Coin::Monero => &[("View key", Detail::MoneroViewKey), ("25 words", Detail::MoneroWords)],
                    _ => &[],
                };
                let y = y + 6 * s;
                ui.half_buttons(y, ("New address", Action::NewAddress), ("Back", Action::Home));
                let mut more: Vec<(&str, Action)> = extras.iter().map(|&(l, d)| (l, Action::Show(d))).collect();
                if paper_pages(&dir, coin) > 0 {
                    more.push(("Whitepaper", Action::Paper));
                }
                let y = y + 24 * s;
                match more.as_slice() {
                    [one] => ui.button(y, one.0, one.1),
                    [a, b] => ui.half_buttons(y, *a, *b),
                    [a, b, c] => ui.third_buttons(y, [*a, *b, *c]),
                    _ => {}
                }
            }
            Screen::Paper(coin, page) => {
                let last = paper_pages(&dir, coin);
                ui.band("< Back", &format!("{} {page}/{last}", coin.ticker()));
                let file = paper_dir(&dir, coin).join(format!("{page:03}.png"));
                if let Some(img) = logos::Gray::load(&file) {
                    ui.p.image(0, 22 * s, &img);
                }
                // The band goes back; the left half of the page turns back, the right on.
                let body = h - 22 * s;
                ui.buttons.push((Rect { x: 0, y: 0, w, h: 22 * s }, Action::Back));
                ui.buttons.push((Rect { x: 0, y: 22 * s, w: w / 2, h: body }, Action::PageTurn(false)));
                ui.buttons.push((Rect { x: w / 2, y: 22 * s, w: w - w / 2, h: body }, Action::PageTurn(true)));
            }
            Screen::Detail(coin, detail) => {
                let Some(seed) = self.seed.as_ref() else { return p };
                let (title, about, value, qr) = match detail {
                    Detail::ZcashViewKey => (
                        "Viewing key",
                        "For the phone app: it sees your Zcash balance, it can never spend.",
                        crate::coins::zcash_ufvk(seed).unwrap_or_default(),
                        true,
                    ),
                    Detail::Zpub => (
                        "Watch key (zpub)",
                        "A watch-only Bitcoin wallet sees every address with it, and can never spend.",
                        crate::coins::bitcoin_zpub(seed).unwrap_or_default(),
                        true,
                    ),
                    Detail::MoneroViewKey => (
                        "View key",
                        "For a view-only Monero wallet, with the primary address (#0). It can never spend.",
                        monero::Keys::from_seed(seed).view_key_hex(),
                        true,
                    ),
                    Detail::MoneroWords => (
                        "Monero words",
                        "The same Monero wallet as 25 Monero words, for any Monero app. Secret, like your 24 words.",
                        monero::Keys::from_seed(seed).phrase(),
                        false,
                    ),
                };
                let y = ui.coin_title(coin, title);
                let ws = (s - 1).max(2);
                let y = ui.p.wrapped(8 * s, y, about, (w - 16 * s) / (8 * ws), ws) + 4 * s;
                let y = if qr {
                    let size = (w / 2).min(h / 3);
                    ui.p.qr(w / 2, y, size, &value);
                    y + size
                } else {
                    y
                };
                let y = if qr {
                    ui.p.wrapped(4 * s, y + 2 * s, &value, (w - 8 * s) / 16, 2)
                } else {
                    ui.numbered_words(y + 2 * s, &value)
                };
                ui.button(y + 6 * s, "Back", Action::Back);
            }
        }
        if !self.note.is_empty() {
            let note = self.note.clone();
            p.fill(0, h - 22 * s, w, 22 * s, false);
            p.text_centered(h - 18 * s, &note, (s - 1).max(2));
        }
        p
    }
}

struct Ui<'a> {
    p: &'a mut Page,
    buttons: &'a mut Vec<(Rect, Action)>,
    s: usize,
    w: usize,
    h: usize,
}

impl Ui<'_> {
    fn header(&mut self) {
        let mut band = Page::new(self.w, 22 * self.s);
        band.text_centered(7 * self.s, "KindleWallet", self.s);
        self.invert_band(&band);
    }

    /// The black band with `left` and `right` in it (the reader's header).
    fn band(&mut self, left: &str, right: &str) {
        let s = self.s;
        let ls = (s - 1).max(2);
        let mut band = Page::new(self.w, 22 * s);
        band.text(8 * s, (22 * s - 8 * ls) / 2, left, ls);
        let rw = right.chars().count() * 8 * ls;
        band.text(self.w.saturating_sub(rw + 8 * s), (22 * s - 8 * ls) / 2, right, ls);
        self.invert_band(&band);
    }

    fn invert_band(&mut self, band: &Page) {
        let s = self.s;
        self.p.fill(0, 0, self.w, 22 * s, true);
        // White text on the black band.
        for y in 0..22 * s {
            for x in 0..self.w {
                if band_ink(band, x, y) {
                    self.p.fill(x, y, 1, 1, false);
                }
            }
        }
    }

    fn rect_button(&mut self, r: Rect, label: &str, action: Action) {
        let s = self.s;
        self.p.frame(r.x, r.y, r.w, r.h, s.max(2));
        let ls = (s - 1).max(2);
        let tw = label.chars().count() * 8 * ls;
        self.p.text(r.x + r.w.saturating_sub(tw) / 2, r.y + (r.h - 8 * ls) / 2, label, ls);
        self.buttons.push((r, action));
    }

    fn button(&mut self, y: usize, label: &str, action: Action) {
        let s = self.s;
        self.rect_button(Rect { x: 10 * s, y, w: self.w - 20 * s, h: 20 * s }, label, action);
    }

    fn half_buttons(&mut self, y: usize, a: (&str, Action), b: (&str, Action)) {
        let s = self.s;
        let half = (self.w - 26 * s) / 2;
        self.rect_button(Rect { x: 10 * s, y, w: half, h: 20 * s }, a.0, a.1);
        self.rect_button(Rect { x: 16 * s + half, y, w: half, h: 20 * s }, b.0, b.1);
    }

    /// The words of a phrase, numbered, in two columns; returns the y below them.
    fn numbered_words(&mut self, top: usize, phrase: &str) -> usize {
        let (s, w) = (self.s, self.w);
        let words: Vec<&str> = phrase.split(' ').collect();
        let ws = (s - 1).max(2);
        let rows = words.len().div_ceil(2);
        for (i, word) in words.iter().enumerate() {
            let (col, row) = (i / rows, i % rows);
            self.p.text(10 * s + col * (w / 2), top + row * 12 * ws, &format!("{:>2} {word}", i + 1), ws);
        }
        top + rows * 12 * ws
    }

    fn third_buttons(&mut self, y: usize, items: [(&str, Action); 3]) {
        let s = self.s;
        let third = (self.w - 32 * s) / 3;
        for (i, (label, action)) in items.into_iter().enumerate() {
            self.rect_button(Rect { x: 10 * s + i * (third + 6 * s), y, w: third, h: 20 * s }, label, action);
        }
    }

    /// A coin's logo in a framed tile that opens it.
    fn logo_tile(&mut self, r: Rect, coin: Coin) {
        self.p.frame(r.x, r.y, r.w, r.h, 2);
        let logo = logos::logo(coin);
        let logo = if logo.w + 8 > r.w.min(r.h) { logo.shrink(2) } else { logo };
        self.p.image(r.x + (r.w - logo.w) / 2, r.y + (r.h - logo.h) / 2, &logo);
        self.buttons.push((r, Action::Open(coin)));
    }

    /// The coin's small logo and name, then `sub`; returns the y below them.
    fn coin_title(&mut self, coin: Coin, sub: &str) -> usize {
        let s = self.s;
        let logo = logos::logo(coin).shrink(2);
        let y = 25 * s;
        self.p.image(8 * s, y, &logo);
        let x = 8 * s + logo.w + 5 * s;
        self.p.text(x, y + 3 * s, coin.name(), s);
        let ls = (s - 1).max(2);
        self.p.text(x, y + 13 * s, sub, ls);
        y + logo.h + 4 * s
    }

    fn pin_pad(&mut self, title: &str, len: usize) {
        let (s, w, h) = (self.s, self.w, self.h);
        self.p.text_centered(32 * s, title, s);
        let dots: String = "*".repeat(len);
        self.p.text_centered(48 * s, if dots.is_empty() { "_" } else { &dots }, s);
        let keys = [
            ("1", Action::Digit(1)),
            ("2", Action::Digit(2)),
            ("3", Action::Digit(3)),
            ("4", Action::Digit(4)),
            ("5", Action::Digit(5)),
            ("6", Action::Digit(6)),
            ("7", Action::Digit(7)),
            ("8", Action::Digit(8)),
            ("9", Action::Digit(9)),
            ("<", Action::Erase),
            ("0", Action::Digit(0)),
            ("OK", Action::PinOk),
        ];
        let top = 66 * s;
        let kw = (w - 20 * s) / 3;
        let kh = ((h - top - 26 * s) / 4).min(kw);
        for (i, (label, action)) in keys.into_iter().enumerate() {
            let r = Rect { x: 10 * s + (i % 3) * kw, y: top + (i / 3) * kh, w: kw - 2 * s, h: kh - 2 * s };
            self.rect_button(r, label, action);
        }
    }
}

/// Whitepapers live beside the wallet, one folder of page pictures per coin (made
/// on a computer by papers/build.py).
fn paper_dir(dir: &Path, coin: Coin) -> PathBuf {
    dir.join("papers").join(coin.ticker())
}

fn paper_pages(dir: &Path, coin: Coin) -> usize {
    std::fs::read_dir(paper_dir(dir, coin))
        .map(|d| d.filter_map(Result::ok).filter(|e| e.path().extension().is_some_and(|x| x == "png")).count())
        .unwrap_or(0)
}

fn band_ink(p: &Page, x: usize, y: usize) -> bool {
    p.ink(x, y)
}
