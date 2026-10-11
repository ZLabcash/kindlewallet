//! The screens and what a tap does, with no device I/O: the Kindle loop (main.rs)
//! and the desktop preview drive the same state machine.

use std::path::PathBuf;

use zeroize::Zeroizing;

use crate::{coins::Coin, render::Page, vault};

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
    ViewKey,
    Home,
    Lock,
    Exit,
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
    ViewKey,
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
            Action::ViewKey => self.screen = Screen::ViewKey,
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
                let words: Vec<&str> = phrase.split(' ').collect();
                let ws = (s - 1).max(2);
                let rows = words.len().div_ceil(2);
                for (i, word) in words.iter().enumerate() {
                    let (col, row) = (i / rows, i % rows);
                    let x = 10 * s + col * (w / 2);
                    let y = 46 * s + row * 12 * ws;
                    ui.p.text(x, y, &format!("{:>2} {word}", i + 1), ws);
                }
                let y = 46 * s + rows * 12 * ws + 8 * s;
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
                ui.p.text_centered(30 * s, "Receive", s);
                let mut y = 48 * s;
                for coin in Coin::ALL {
                    ui.button(y, coin.name(), Action::Open(coin));
                    y += 26 * s;
                }
                y += 6 * s;
                ui.button(y, "Recovery words", Action::ShowWords);
                ui.button(y + 26 * s, "Zcash viewing key", Action::ViewKey);
                ui.half_buttons(y + 52 * s, ("Lock", Action::Lock), ("Exit", Action::Exit));
            }
            Screen::Receive(coin) => {
                let Some((j, addr)) = self.address.clone() else { return p };
                ui.p.text_centered(30 * s, &format!("{} #{j}", coin.ticker()), s);
                let size = w.min(h / 2) - 20 * s;
                ui.p.qr(w / 2, 42 * s, size, &addr);
                let ws = (s - 1).max(2);
                let cols = (w - 16 * s) / (8 * ws);
                let y = ui.p.wrapped(8 * s, 46 * s + size, &addr, cols, ws);
                ui.half_buttons(y + 8 * s, ("New address", Action::NewAddress), ("Back", Action::Home));
            }
            Screen::ViewKey => {
                let key = self.seed.as_ref().and_then(|s| crate::coins::zcash_ufvk(s)).unwrap_or_default();
                ui.p.text_centered(30 * s, "Zcash viewing key", s);
                let ws = (s - 1).max(2);
                ui.p.wrapped(
                    8 * s,
                    44 * s,
                    "For the phone app: it sees your Zcash balance, it can never spend.",
                    (w - 16 * s) / (8 * ws),
                    ws,
                );
                let cols = (w - 8 * s) / (8 * 2);
                let y = ui.p.wrapped(4 * s, 80 * s, &key, cols, 2);
                ui.button(y + 8 * s, "Back", Action::Home);
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
        let s = self.s;
        self.p.fill(0, 0, self.w, 22 * s, true);
        // White text on the black band: draw, then invert the band.
        let mut band = Page::new(self.w, 22 * s);
        band.text_centered(7 * s, "KindleWallet", s);
        for y in 0..22 * s {
            for x in 0..self.w {
                if band_ink(&band, x, y) {
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

fn band_ink(p: &Page, x: usize, y: usize) -> bool {
    p.ink(x, y)
}
