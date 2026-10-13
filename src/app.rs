//! The screens and what a tap does, with no device I/O: the Kindle loop (main.rs),
//! the desktop server and the desktop preview drive the same state machine.
//!
//! The phone link adds the zfirm signer session (pairing, review, signing): the
//! state machine takes link events and frames in and hands [`Effect`]s out (switch
//! the link on or off, frames to send) for the caller to carry out.

use std::{
    net::Ipv4Addr,
    path::{Path, PathBuf},
};

use zeroize::Zeroizing;
use zfirm_core::{
    Account, Network,
    protocol::{self, Decoder, Frame, Info, error},
    session::{self, Session, Show, Step, Work},
    sign::{self, Review},
};

use crate::{coins::Coin, evm, link, logos, monero, nft, render::Page, vault};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Create,
    WroteThem,
    Digit(u8),
    Erase,
    PinOk,
    /// A coin's menu.
    Open(Coin),
    /// The open coin's receiving address (a fresh one).
    Receive,
    NewAddress,
    Show(Detail),
    /// A secret, after the PIN is typed again.
    Reveal(Secret),
    Settings,
    /// The EVM networks sharing the 0x address, and a page of their list on or back.
    Networks,
    NetworksPage(i32),
    OpenNetwork(usize),
    /// The NFTs of a coin or of an EVM network.
    NftHome(NftPlace),
    Back,
    Paper,
    PageTurn(bool),
    Home,
    Lock,
    Exit,
    /// Zcash: Wi-Fi on and wait for ZApp.
    ConnectPhone,
    /// The link's own screen (pairing QR, state).
    LinkScreen,
    /// The pair prompt's answers.
    Allow,
    Refuse,
    /// Through a transaction's review pages.
    ReviewPage(bool),
    /// Signs the reviewed transaction; a tap on the screen counts only when held.
    Sign,
    /// Leaves a request's outcome.
    DoneOk,
    /// The zkSNARKs gallery.
    Nfts,
    /// The gallery's tabs: every token, or the user's.
    NftTab {
        mine: bool,
    },
    /// The gallery a number of pages on (or back).
    NftPage(i32),
    NftOpen(u16),
    /// From a portrait to the next or previous in its tab.
    NftStep(bool),
}

/// What the caller must do for the state machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Wi-Fi on, firewall, listen; then [`App::link_up`] or [`App::link_failed`].
    LinkOn,
    /// Stop listening, drop the phone, firewall back, Wi-Fi off.
    LinkOff,
    /// The pairing QR's secret while the QR is on screen.
    Psk(Option<[u8; 16]>),
    /// Frames for the phone.
    Send(Vec<u8>),
    /// Drop the phone.
    Hangup,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkPhase {
    Off,
    /// Wi-Fi joining, firewall going up.
    Starting,
    /// Listening at this address.
    Up(Ipv4Addr),
    Failed(String),
}

/// The phone connected through the link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Phone {
    pub key: [u8; 32],
    /// In the `phones` list (allowed before), rather than pairing with the QR.
    pub known: bool,
}

/// What the device waits for the user to answer (for the headless server).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prompt {
    Pair,
    Sign,
}

/// Seconds a finger must stay on "Hold to sign".
pub const HOLD_SECS: f32 = 1.5;
/// The name the phone sees.
pub const DEVICE_NAME: &str = "Kindle";
/// The gallery's grid.
const NFT_COLS: usize = 6;
/// EVM networks listed per page.
const NETWORKS_PER_PAGE: usize = 6;
const NFT_ROWS: usize = 5;

/// What a coin's own screen can show besides its receiving address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    /// Zcash: the unified full viewing key.
    ZcashViewKey,
    /// Bitcoin: the BIP 84 account key.
    Zpub,
    /// Monero: the private view key, for a view-only wallet.
    MoneroViewKey,
}

/// Where NFTs live: a coin, or one EVM network (index in [`evm::NETWORKS`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NftPlace {
    Coin(Coin),
    Evm(usize),
}

/// What is shown only after the PIN is typed again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Secret {
    /// The 24 words that restore everything.
    Recovery,
    /// Monero's own 25-word phrase.
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
    /// A coin's menu: receive, its keys, its extras.
    Coin(Coin),
    Receive(Coin),
    /// The EVM networks, at page n (from 0).
    Networks(usize),
    /// One EVM network: its address, its NFTs.
    Network(usize),
    NftHome(NftPlace),
    Settings,
    /// The PIN again, before a secret shows.
    PinCheck(Secret),
    Secret(Secret),
    Detail(Coin, Detail),
    /// A coin's whitepaper, at page n (from 1).
    Paper(Coin, usize),
    /// The phone link: pairing QR or the connected phone.
    Link,
    /// "Share the viewing key with this phone?"
    PairPrompt,
    /// Slow work running (checking, signing).
    Busy(String),
    /// A transaction's review, at page n (from 0); the last page confirms.
    Review(u8),
    Done {
        ok: bool,
        note: String,
    },
    /// The zkSNARKs grid (tab and page are in `App::gallery`).
    Nfts,
    /// One zkSNARK.
    Nft(u16),
    /// Leaving: Amazon's interface is on its way back.
    Closing,
}

impl Screen {
    /// The screens a phone's request puts up, over whatever was showing.
    fn is_request(&self) -> bool {
        matches!(self, Screen::PairPrompt | Screen::Busy(_) | Screen::Review(_) | Screen::Done { .. })
    }
}

/// The phone link's state.
struct Link {
    phase: LinkPhase,
    phone: Option<Phone>,
    /// The pairing secret of the QR on screen.
    psk: Option<[u8; 16]>,
}

/// The gallery's place: which tab, which page (from 0).
#[derive(Clone, Copy)]
struct Gallery {
    mine: bool,
    page: usize,
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
    /// The labels of the buttons last drawn, for tests.
    labels: Vec<(String, Rect)>,
    pub quit: bool,
    /// The Zcash account, while unlocked (what the phone link serves).
    zec: Option<Account>,
    link: Link,
    session: Session,
    decoder: Decoder,
    review: Option<Review>,
    work: Option<Work>,
    /// Where a phone's request returns to.
    resume: Screen,
    effects: Vec<Effect>,
    gallery: Gallery,
    /// The packed portraits, read on first use.
    collection: Option<nft::Collection>,
    mine: Vec<u16>,
    /// The link's TCP port, for the pairing QR.
    pub port: u16,
}

/// The unlocked Zcash account, as the zfirm session sees it.
struct ZecHost<'a> {
    account: Option<&'a Account>,
    index: u32,
}

impl session::Host for ZecHost<'_> {
    fn info(&self) -> Option<Info> {
        let account = self.account?;
        Some(Info {
            network: account.network().to_byte(),
            account: 0,
            seed_fingerprint: account.seed_fingerprint(),
            ufvk: account.ufvk(),
            created_at: 0,
            device_name: DEVICE_NAME.into(),
            address_index: self.index,
        })
    }

    fn address(&self) -> Option<(u32, String)> {
        Some((self.index, self.account?.encode_address(self.index)))
    }
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
            labels: Vec::new(),
            quit: false,
            zec: None,
            link: Link { phase: LinkPhase::Off, phone: None, psk: None },
            session: Session::new(),
            decoder: Decoder::new(),
            review: None,
            work: None,
            resume: Screen::Home,
            effects: Vec::new(),
            gallery: Gallery { mine: false, page: 0 },
            collection: None,
            mine: Vec::new(),
            port: link::PORT,
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
        self.press(x, y, 0.0);
    }

    /// A finger lifted at (x, y) after `held` seconds on the screen.
    pub fn press(&mut self, x: usize, y: usize, held: f32) {
        let Some(&(_, action)) = self.buttons.iter().find(|(r, _)| r.contains(x, y)) else { return };
        if action == Action::Sign && held < HOLD_SECS {
            self.note = "Keep your finger on it to sign".into();
            return;
        }
        self.act(action);
    }

    /// The middle of the button labelled `label` on the page last drawn (tests).
    pub fn button_at(&self, label: &str) -> Option<(usize, usize)> {
        self.labels.iter().find(|(l, _)| l == label).map(|(_, r)| (r.x + r.w / 2, r.y + r.h / 2))
    }

    /// What the caller must now do, in order.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    pub fn link_phase(&self) -> &LinkPhase {
        &self.link.phase
    }

    pub fn phone(&self) -> Option<Phone> {
        self.link.phone
    }

    /// What the user is being asked, if anything.
    pub fn prompt(&self) -> Option<Prompt> {
        match self.screen {
            Screen::PairPrompt => Some(Prompt::Pair),
            Screen::Review(_) => Some(Prompt::Sign),
            _ => None,
        }
    }

    /// The transaction under review.
    pub fn review(&self) -> Option<&Review> {
        self.review.as_ref()
    }

    /// The pairing QR's text, while it is on screen.
    pub fn pair_uri(&self) -> Option<String> {
        let (LinkPhase::Up(ip), Some(psk)) = (&self.link.phase, self.link.psk) else { return None };
        let k = link::public_key(&self.dir).ok()?;
        Some(link::pair_uri(*ip, self.port, &k, &psk, DEVICE_NAME))
    }

    /// The link is listening at `ip`.
    pub fn link_up(&mut self, ip: Ipv4Addr) {
        if self.link.phase == LinkPhase::Starting {
            self.link.phase = LinkPhase::Up(ip);
        }
        self.sync_psk();
    }

    /// The link could not start (the caller has already put Wi-Fi and firewall back).
    pub fn link_failed(&mut self, why: &str) {
        if self.link.phase == LinkPhase::Starting {
            self.link.phase = LinkPhase::Failed(why.into());
            self.link.phone = None;
        }
        self.sync_psk();
    }

    pub fn link_event(&mut self, event: link::LinkEvent) {
        if !matches!(self.link.phase, LinkPhase::Up(_)) {
            if matches!(event, link::LinkEvent::Connected { .. }) {
                self.effects.push(Effect::Hangup);
            }
            return;
        }
        match event {
            link::LinkEvent::Connected { phone, known } => {
                self.decoder.reset();
                self.session = Session::new();
                self.link.phone = Some(Phone { key: phone, known });
                // A QR code serves once: the next scan needs the next one.
                if self.link.psk.take().is_some() {
                    self.effects.push(Effect::Psk(None));
                }
            }
            link::LinkEvent::Data(bytes) => self.link_data(&bytes),
            link::LinkEvent::Closed => {
                self.decoder.reset();
                self.link.phone = None;
                self.work = None;
                let step = self.session.disconnected();
                self.apply(step);
                if matches!(self.screen, Screen::Busy(_) | Screen::PairPrompt | Screen::Review(_)) {
                    self.review = None;
                    self.screen = self.resume.clone();
                }
            }
        }
        self.sync_psk();
    }

    /// Slow work waiting to run (after its Busy page has been drawn).
    pub fn has_work(&self) -> bool {
        self.work.is_some()
    }

    /// Runs the pending work: checking or signing a transaction.
    pub fn run_work(&mut self) {
        let Some(work) = self.work.take() else { return };
        let Some(account) = self.zec.as_ref() else { return };
        let step = match work {
            Work::Review(req) => self.session.reviewed(sign::review(&req, account)),
            Work::Sign(req) => self.session.signed(sign::sign_reviewed(&req, account, rand_core::OsRng)),
        };
        self.apply(step);
        self.sync_psk();
    }

    /// Puts up what a session would show, without a phone (the desktop preview).
    pub fn preview_show(&mut self, show: Show) {
        self.apply(Step { show: Some(show), ..Step::default() });
    }

    pub fn act(&mut self, action: Action) {
        self.act_inner(action);
        self.sync_psk();
    }

    fn act_inner(&mut self, action: Action) {
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
            Action::Open(coin) => self.screen = Screen::Coin(coin),
            Action::Receive => {
                if let Screen::Coin(coin) = self.screen {
                    let index = self.load_index(coin);
                    self.show_address(coin, index);
                }
            }
            Action::Settings => self.screen = Screen::Settings,
            Action::Networks => self.screen = Screen::Networks(0),
            Action::NetworksPage(by) => {
                if let Screen::Networks(page) = self.screen {
                    let pages = evm::NETWORKS.len().div_ceil(NETWORKS_PER_PAGE);
                    self.screen = Screen::Networks((page as i64 + by as i64).clamp(0, pages as i64 - 1) as usize);
                }
            }
            Action::OpenNetwork(i) => {
                if self.seed.is_some() && i < evm::NETWORKS.len() {
                    self.show_address(Coin::Ethereum, 0);
                    self.screen = Screen::Network(i);
                }
            }
            Action::NftHome(place) => {
                self.mine = nft::mine(&self.dir);
                self.screen = Screen::NftHome(place);
            }
            Action::Reveal(secret) => {
                self.pin.clear();
                self.screen = Screen::PinCheck(secret);
            }
            // One address per coin, except Zcash: its shielded addresses are unlinkable,
            // so a new one each time costs nothing.
            Action::NewAddress => {
                if let Screen::Receive(coin) = self.screen
                    && coin == Coin::Zcash
                {
                    let next = self.address.as_ref().map_or(0, |(j, _)| j + 1);
                    self.show_address(coin, next);
                    // A paired phone follows the address shown.
                    if coin == Coin::Zcash
                        && let Some((j, text)) = self.address.clone()
                    {
                        let step = self.session.address_changed(j, &text);
                        self.apply(step);
                    }
                }
            }
            Action::Show(detail) => {
                if let Screen::Coin(coin) = self.screen {
                    self.screen = Screen::Detail(coin, detail);
                }
            }
            Action::Back => {
                self.pin.clear();
                match self.screen {
                    Screen::Receive(coin) | Screen::Detail(coin, _) | Screen::Paper(coin, _) => {
                        self.screen = Screen::Coin(coin)
                    }
                    Screen::Link => self.screen = Screen::Coin(Coin::Zcash),
                    Screen::Nfts => self.screen = Screen::NftHome(NftPlace::Coin(Coin::Zcash)),
                    Screen::Networks(_) => self.screen = Screen::Coin(Coin::Ethereum),
                    Screen::Network(i) => self.screen = Screen::Networks(i / NETWORKS_PER_PAGE),
                    Screen::NftHome(NftPlace::Coin(coin)) => self.screen = Screen::Coin(coin),
                    Screen::NftHome(NftPlace::Evm(i)) => self.screen = Screen::Network(i),
                    Screen::Nft(_) => self.screen = Screen::Nfts,
                    Screen::PinCheck(Secret::MoneroWords) | Screen::Secret(Secret::MoneroWords) => {
                        self.screen = Screen::Coin(Coin::Monero)
                    }
                    Screen::PinCheck(Secret::Recovery) | Screen::Secret(Secret::Recovery) => {
                        self.screen = Screen::Settings
                    }
                    // Leaving Zcash leaves link mode.
                    Screen::Coin(_) | Screen::Settings => self.act_inner(Action::Home),
                    _ => {}
                }
            }
            Action::Paper => {
                if let Screen::Coin(coin) = self.screen {
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
            // Leaving Zcash leaves link mode.
            // The phone link stays on while browsing: only "Stop link" ends it for good.
            Action::Home => self.screen = Screen::Home,
            Action::Lock => {
                self.link_off();
                self.phrase = None;
                self.seed = None;
                self.zec = None;
                self.address = None;
                self.screen = Screen::Unlock;
            }
            Action::Exit => {
                self.link_off();
                self.phrase = None;
                self.seed = None;
                self.zec = None;
                self.address = None;
                self.screen = Screen::Closing;
                self.quit = true;
            }
            Action::ConnectPhone => {
                self.link_on();
                self.screen = Screen::Link;
            }
            Action::LinkScreen => self.screen = Screen::Link,
            Action::Allow if self.screen == Screen::PairPrompt => self.answer(true),
            Action::Refuse if matches!(self.screen, Screen::PairPrompt | Screen::Review(_)) => self.answer(false),
            Action::ReviewPage(forward) => {
                if let Screen::Review(page) = self.screen {
                    let last = self.review.as_ref().map_or(0, |r| r.pages().saturating_sub(1));
                    self.screen = Screen::Review(if forward { (page + 1).min(last) } else { page.saturating_sub(1) });
                }
            }
            Action::Sign => {
                if let Screen::Review(page) = self.screen
                    && self.review.as_ref().is_some_and(|r| page + 1 == r.pages())
                {
                    self.answer(true);
                }
            }
            Action::Allow | Action::Refuse => {}
            Action::DoneOk => {
                if let Screen::Done { .. } = self.screen {
                    self.screen = self.resume.clone();
                }
            }
            Action::Nfts => {
                if self.collection.is_none() {
                    self.collection = nft::Collection::load(&self.dir);
                }
                self.mine = nft::mine(&self.dir);
                self.screen = Screen::Nfts;
            }
            Action::NftTab { mine } => self.gallery = Gallery { mine, page: 0 },
            Action::NftPage(by) => {
                let pages = self.gallery_len().div_ceil(NFT_COLS * NFT_ROWS).max(1);
                let page = (self.gallery.page as i64 + by as i64).clamp(0, pages as i64 - 1);
                self.gallery.page = page as usize;
            }
            Action::NftOpen(n) => self.screen = Screen::Nft(n),
            Action::NftStep(forward) => {
                if let Screen::Nft(n) = self.screen
                    && let Some(next) = self.gallery_step(n, forward)
                {
                    self.screen = Screen::Nft(next);
                }
            }
        }
    }

    // ---- the phone link ----

    /// Ends link mode: the caller switches Wi-Fi off; an open request is dropped.
    fn link_off(&mut self) {
        if self.link.phase != LinkPhase::Off {
            self.effects.push(Effect::LinkOff);
        }
        self.link = Link { phase: LinkPhase::Off, phone: None, psk: None };
        self.session = Session::new();
        self.decoder.reset();
        self.review = None;
        self.work = None;
        if self.screen.is_request() {
            self.screen = self.resume.clone();
        }
    }

    /// The pairing secret lives while the QR is on screen, through the requests of a
    /// phone that scanned it (until the user leaves the link's screen); a new one each
    /// time the QR comes back.
    fn sync_psk(&mut self) {
        let on_link = self.screen == Screen::Link || (self.screen.is_request() && self.resume == Screen::Link);
        let wanted = matches!(self.link.phase, LinkPhase::Up(_)) && on_link;
        if wanted && self.link.psk.is_none() {
            let psk = link::new_psk();
            self.link.psk = Some(psk);
            self.effects.push(Effect::Psk(Some(psk)));
        } else if !wanted && self.link.psk.is_some() {
            self.link.psk = None;
            self.effects.push(Effect::Psk(None));
        }
    }

    /// The Zcash address the wallet last showed (the one the phone follows).
    fn zec_index(&self) -> u32 {
        std::fs::read_to_string(self.index_path(Coin::Zcash)).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
    }

    fn link_data(&mut self, bytes: &[u8]) {
        let mut next = self.decoder.push(bytes);
        loop {
            match next {
                Ok(Some(frame)) => {
                    self.frame(frame);
                    next = self.decoder.next();
                }
                Ok(None) => break,
                Err(_) => {
                    let step = self.session.bad_frame();
                    self.apply(step);
                    break;
                }
            }
        }
    }

    fn frame(&mut self, f: Frame) {
        if self.zec.is_none() {
            self.send_error(error::LOCKED, "Locked");
            return;
        }
        if f.kind == link::msg::NFTS {
            if !self.session.paired() {
                self.send_error(error::NOT_PAIRED, "Say hello first");
            } else if let Some(tokens) = nft::parse_tokens(&f.payload) {
                match nft::save_mine(&self.dir, &tokens) {
                    Ok(()) => {
                        self.mine = tokens;
                        self.effects.push(Effect::Send(protocol::encode(link::msg::NFTS_OK, &[])));
                    }
                    Err(_) => self.send_error(error::BAD_REQUEST, "Could not store"),
                }
            } else {
                self.send_error(error::BAD_REQUEST, "Bad token list");
            }
            return;
        }
        let index = self.zec_index();
        let host = ZecHost { account: self.zec.as_ref(), index };
        let step = self.session.frame(f, &host);
        // A phone allowed before gets its viewing key without a new prompt (and
        // without the "waiting for the user" status that comes with one).
        if step.show == Some(Show::PairPrompt) && self.link.phone.is_some_and(|p| p.known) {
            self.answer(true);
            return;
        }
        self.apply(step);
    }

    fn send_error(&mut self, code: u8, message: &str) {
        self.effects.push(Effect::Send(protocol::encode(protocol::msg::ERROR, &protocol::encode_error(code, message))));
    }

    fn answer(&mut self, yes: bool) {
        let index = self.zec_index();
        let host = ZecHost { account: self.zec.as_ref(), index };
        let step = self.session.answer(yes, &host);
        self.apply(step);
    }

    fn apply(&mut self, step: Step) {
        self.effects.extend(step.send.into_iter().map(Effect::Send));
        if step.show.is_some() {
            self.note.clear();
        }
        if let Some(work) = step.work {
            self.work = Some(work);
        }
        match step.show {
            Some(Show::PairPrompt) => self.interrupt(Screen::PairPrompt),
            Some(Show::Busy(label)) => self.interrupt(Screen::Busy(label.into())),
            Some(Show::Review(review)) => {
                self.review = Some(review);
                self.interrupt(Screen::Review(0));
            }
            Some(Show::Done { ok, note }) => {
                self.review = None;
                self.interrupt(Screen::Done { ok, note: note.into() });
            }
            Some(Show::Resume) if self.screen.is_request() => self.screen = self.resume.clone(),
            Some(Show::Resume) | None => {}
        }
    }

    /// Puts a request's screen up, remembering where to come back.
    fn interrupt(&mut self, next: Screen) {
        if !self.screen.is_request() {
            self.resume = self.screen.clone();
        }
        self.screen = next;
    }

    // ---- the gallery ----

    /// The tokens of the open tab.
    fn gallery_len(&self) -> usize {
        if self.gallery.mine { self.mine.len() } else { nft::COUNT as usize }
    }

    fn gallery_token(&self, i: usize) -> Option<u16> {
        if self.gallery.mine { self.mine.get(i).copied() } else { (i < nft::COUNT as usize).then_some(i as u16 + 1) }
    }

    fn gallery_step(&self, n: u16, forward: bool) -> Option<u16> {
        let i = if self.gallery.mine { self.mine.iter().position(|&m| m == n)? } else { n as usize - 1 };
        let j = if forward { i + 1 } else { i.checked_sub(1)? };
        self.gallery_token(j)
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
            Screen::PinCheck(secret) => {
                let secret = *secret;
                let file = std::fs::read(self.dir.join("wallet.zkv")).unwrap_or_default();
                match vault::open(&file, &pin) {
                    Ok(_) => self.screen = Screen::Secret(secret),
                    Err(vault::Error::WrongPin) => self.note = "Wrong PIN".into(),
                    Err(vault::Error::Corrupt) => self.note = "Wallet file damaged".into(),
                }
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
        let seed = Zeroizing::new(crate::seed(&phrase));
        self.zec = Account::from_seed(&*seed, Network::Main, 0).ok();
        self.seed = Some(seed);
        self.phrase = Some(phrase);
        self.screen = Screen::Home;
        // Unlocked, the phone link is up: Wi-Fi on behind the firewall, for ZApp.
        self.link_on();
    }

    /// Wi-Fi on, firewall, listening (the caller does it). Locking, sleep and exit
    /// turn it off.
    fn link_on(&mut self) {
        if matches!(self.link.phase, LinkPhase::Off | LinkPhase::Failed(_)) && self.zec.is_some() {
            self.link.phase = LinkPhase::Starting;
            self.effects.push(Effect::LinkOn);
        }
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

    /// Zcash hands out a fresh address each time: the one after the last shown. Every
    /// other coin keeps address #0.
    fn load_index(&self, coin: Coin) -> u32 {
        if coin != Coin::Zcash {
            return 0;
        }
        std::fs::read_to_string(self.index_path(coin))
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .map_or(0, |j| j.saturating_add(1))
    }

    fn show_address(&mut self, coin: Coin, index: u32) {
        let (Some(seed), Some(phrase)) = (&self.seed, &self.phrase) else { return };
        match coin.address_from_phrase(phrase, seed, index) {
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
        let pair_uri = if self.screen == Screen::Link { self.pair_uri() } else { None };
        let (gallery_total, gallery_page): (usize, Vec<u16>) = if self.screen == Screen::Nfts {
            let per = NFT_COLS * NFT_ROWS;
            (self.gallery_len(), (0..per).map_while(|k| self.gallery_token(self.gallery.page * per + k)).collect())
        } else {
            (0, Vec::new())
        };
        self.labels.clear();
        let mut ui = Ui { p: &mut p, buttons: &mut self.buttons, labels: &mut self.labels, s, w, h };
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
            Screen::Words => {
                let phrase = self.phrase.as_deref().cloned().unwrap_or_default();
                ui.p.text_centered(30 * s, "Your 24 words", s);
                let y = ui.numbered_words(46 * s, &phrase) + 8 * s;
                ui.button(y, "I wrote them down", Action::WroteThem);
            }
            Screen::NewPin { first } => {
                let title = if first.is_none() { "Choose a PIN" } else { "Same PIN again" };
                ui.pin_pad(title, self.pin.len());
            }
            Screen::Unlock => ui.pin_pad("Enter your PIN", self.pin.len()),
            Screen::Home => {
                // The logos, four to a row; Ethereum's tile stands for every EVM network.
                let top = 26 * s;
                let cols = 4;
                let tile_w = (w - 16 * s) / cols;
                let tile_h = ((h - top - 34 * s) / Coin::ALL.len().div_ceil(cols)).min(tile_w);
                for (n, row) in Coin::ALL.chunks(cols).enumerate() {
                    let x0 = 8 * s + (cols - row.len()) * tile_w / 2;
                    for (i, &coin) in row.iter().enumerate() {
                        let r = Rect { x: x0 + i * tile_w, y: top + n * tile_h, w: tile_w - 2 * s, h: tile_h - 2 * s };
                        if coin == Coin::Ethereum {
                            ui.evm_tile(r);
                        } else {
                            ui.logo_tile(r, coin);
                        }
                    }
                }
                ui.half_buttons(h - 26 * s, ("Settings", Action::Settings), ("Lock", Action::Lock));
            }
            Screen::Coin(coin) => {
                ui.back_band("Home", coin.ticker(), Action::Back);
                let y = ui.coin_title(coin, coin_about(coin));
                let receive = if coin == Coin::Ethereum { "One 0x address, every network" } else { "Address and QR code" };
                let mut rows: Vec<(&str, String, Action)> = vec![("Receive", receive.into(), Action::Receive)];
                if coin == Coin::Ethereum {
                    rows.push((
                        "Networks",
                        format!("{} EVM networks, same address", evm::NETWORKS.len()),
                        Action::Networks,
                    ));
                }
                match coin {
                    Coin::Zcash => {
                        let state = if self.link.phone.is_some() { "ZApp connected" } else { "Scan the QR code with ZApp" };
                        rows.push(("Phone link", state.into(), Action::ConnectPhone));

                        rows.push((
                            "Viewing key",
                            "Sees balance, never spends".into(),
                            Action::Show(Detail::ZcashViewKey),
                        ));
                    }
                    Coin::Bitcoin => rows.push((
                        "Watch key (zpub)",
                        "For a watch-only wallet".into(),
                        Action::Show(Detail::Zpub),
                    )),
                    Coin::Monero => {
                        rows.push((
                            "View key",
                            "For a view-only wallet".into(),
                            Action::Show(Detail::MoneroViewKey),
                        ));
                        rows.push((
                            "Monero words",
                            "25 words, secret: PIN asked".into(),
                            Action::Reveal(Secret::MoneroWords),
                        ));
                    }
                    _ => {}
                }
                if coin.has_nfts() {
                    let sub = match coin {
                        Coin::Zcash if self.mine.is_empty() => "zkSNARKs".to_string(),
                        Coin::Zcash => format!("zkSNARKs: {} yours", self.mine.len()),
                        Coin::Ethereum => "Per network".to_string(),
                        _ => "Collectibles you hold".to_string(),
                    };
                    let action = if coin == Coin::Ethereum { Action::Networks } else { Action::NftHome(NftPlace::Coin(coin)) };
                    rows.push(("NFTs", sub, action));
                }
                if paper_pages(&dir, coin) > 0 {
                    rows.push(("Whitepaper", "Read it here, offline".into(), Action::Paper));
                }
                let mut y = y + 4 * s;
                for (title, sub, action) in rows {
                    y = ui.menu_row(y, title, &sub, action);
                }
            }
            Screen::Receive(coin) => {
                let Some((j, addr)) = self.address.clone() else { return p };
                let right = if coin == Coin::Zcash { format!("Address #{j}") } else { String::new() };
                ui.back_band(coin.name(), &right, Action::Back);
                let size = (w * 3 / 5).min(h / 2);
                let y = 30 * s;
                ui.p.qr(w / 2, y, size, &addr);
                let ws = (s - 1).max(2);
                let cols = (w - 16 * s) / (8 * ws);
                let mut y = ui.p.wrapped(8 * s, y + size + 4 * s, &addr, cols, ws);
                if coin == Coin::Ethereum {
                    y = ui.p.wrapped(
                        8 * s,
                        y + 4 * s,
                        "The same address on Ethereum, Base, BNB Chain, Arbitrum and every EVM network in Networks.",
                        cols,
                        ws,
                    );
                }
                let _ = y;
                if coin == Coin::Zcash {
                    ui.button(h - 30 * s, "New address", Action::NewAddress);
                }
            }
            Screen::Networks(page) => {
                let pages = evm::NETWORKS.len().div_ceil(NETWORKS_PER_PAGE);
                ui.back_band("Ethereum", &format!("Networks {}/{pages}", page + 1), Action::Back);
                let mut y = 28 * s;
                for (k, net) in evm::NETWORKS.iter().enumerate().skip(page * NETWORKS_PER_PAGE).take(NETWORKS_PER_PAGE) {
                    y = ui.network_row(y, net, Action::OpenNetwork(k));
                }
                ui.half_buttons(h - 26 * s, ("< Previous", Action::NetworksPage(-1)), ("Next >", Action::NetworksPage(1)));
            }
            Screen::Network(i) => {
                let net = &evm::NETWORKS[i];
                ui.back_band("Networks", net.symbol, Action::Back);
                let mut y = 28 * s;
                if let Some(logo) = logos::evm_logo(net.key) {
                    let logo = logo.shrink(2);
                    ui.p.image(8 * s, y, &logo);
                    ui.p.text(8 * s + logo.w + 5 * s, y + 3 * s, net.name, s);
                    ui.p.text(8 * s + logo.w + 5 * s, y + 13 * s, &format!("Chain id {}", net.chain_id), (s - 1).max(2));
                    y += logo.h + 4 * s;
                } else {
                    ui.p.text(8 * s, y, net.name, s);
                    y += 14 * s;
                }
                if let Some((_, addr)) = self.address.clone() {
                    let size = (w / 2).min(h / 3);
                    ui.p.qr(w / 2, y, size, &addr);
                    let ws = (s - 1).max(2);
                    y = ui.p.wrapped(8 * s, y + size + 2 * s, &addr, (w - 16 * s) / (8 * ws), ws) + 6 * s;
                }
                ui.menu_row(y, "NFTs", "Collectibles you hold", Action::NftHome(NftPlace::Evm(i)));
            }
            Screen::NftHome(place) => {
                let (back, name) = match place {
                    NftPlace::Coin(c) => (c.name(), c.name()),
                    NftPlace::Evm(i) => (evm::NETWORKS[i].name, evm::NETWORKS[i].name),
                };
                ui.back_band(back, "NFTs", Action::Back);
                let ws = (s - 1).max(2);
                let cols = (w - 16 * s) / (8 * ws);
                if place == NftPlace::Coin(Coin::Zcash) {
                    let held = if self.mine.is_empty() {
                        "All 10,000 portraits, offline".to_string()
                    } else {
                        format!("{} yours, and all 10,000", self.mine.len())
                    };
                    ui.menu_row(30 * s, "zkSNARKs", &held, Action::Nfts);
                } else {
                    ui.p.text_centered(50 * s, "None here yet", s);
                    ui.p.wrapped(
                        8 * s,
                        66 * s,
                        &format!(
                            "The NFTs you hold on {name} will show here: ZApp reads them on the phone and sends them over the link. The Kindle never goes online for it."
                        ),
                        cols,
                        ws,
                    );
                }
            }
            Screen::Settings => {
                ui.back_band("Home", "Settings", Action::Back);
                let mut y = 30 * s;
                for (title, sub, action) in [
                    ("Recovery words", "Your 24 words: PIN asked", Action::Reveal(Secret::Recovery)),
                    ("Exit KindleWallet", "Back to the Kindle's library", Action::Exit),
                ] {
                    y = ui.menu_row(y, title, sub, action);
                }
            }
            Screen::PinCheck(secret) => {
                ui.back_band("Cancel", "", Action::Back);
                let title = match secret {
                    Secret::Recovery => "PIN for your words",
                    Secret::MoneroWords => "PIN for Monero words",
                };
                ui.pin_pad(title, self.pin.len());
            }
            Screen::Secret(secret) => {
                let Some(seed) = self.seed.as_ref() else { return p };
                let (title, words) = match secret {
                    Secret::Recovery => ("Your 24 words", self.phrase.as_deref().cloned().unwrap_or_default()),
                    Secret::MoneroWords => ("Monero words", monero::Keys::from_seed(seed).phrase()),
                };
                ui.back_band("Hide", "", Action::Back);
                ui.p.text_centered(28 * s, title, s);
                let ws = (s - 1).max(2);
                let y = ui.p.wrapped(
                    8 * s,
                    40 * s,
                    "Paper only: never a photo, never typed into a phone or computer.",
                    (w - 16 * s) / (8 * ws),
                    ws,
                );
                ui.numbered_words(y + 4 * s, &words);
                ui.button(h - 26 * s, "Hide", Action::Back);
            }
            Screen::Link => {
                ui.back_band("Zcash", "Phone link", Action::Back);
                let ws = (s - 1).max(2);
                let cols = (w - 16 * s) / (8 * ws);
                let y = 30 * s;
                match self.link.phase.clone() {
                    LinkPhase::Up(_) if self.link.phone.is_some() => {
                        ui.p.text_centered(y + 30 * s, "ZApp connected", s);
                        ui.p.wrapped(8 * s, y + 50 * s, "What ZApp asks shows here, to check and sign.", cols, ws);
                        ui.p.text_centered(y + 80 * s, "Offline: no Internet access (checked)", ws);
                    }
                    LinkPhase::Up(ip) => {
                        if let Some(uri) = pair_uri {
                            let size = (w * 3 / 5).min(h / 2);
                            ui.p.qr(w / 2, y, size, &uri);
                            let y = y + size + 6 * s;
                            ui.p.text_centered(y, "Scan with ZApp: Connect", s);
                            ui.p.text_centered(y + 14 * s, &format!("{ip}:{}", self.port), ws);
                            ui.p.text_centered(y + 26 * s, "Offline: no Internet access (checked)", ws);
                        }
                    }
                    LinkPhase::Off | LinkPhase::Starting => {
                        ui.p.text_centered(y + 30 * s, "One moment...", s);
                    }
                    LinkPhase::Failed(why) => {
                        let y = ui.p.wrapped(8 * s, y + 10 * s, &why, cols, ws);
                        ui.p.wrapped(8 * s, y + 4 * s, "Is this Kindle's Wi-Fi set up (Amazon settings)?", cols, ws);
                        ui.button(h - 30 * s, "Try again", Action::ConnectPhone);
                    }
                }
            }
            Screen::PairPrompt => {
                let ws = (s - 1).max(2);
                let cols = (w - 16 * s) / (8 * ws);
                ui.p.text_centered(40 * s, "Pair a phone", s);
                let y =
                    ui.p.wrapped(8 * s, 60 * s, "ZApp wants this wallet's viewing key - it cannot spend.", cols, ws);
                let y = ui.p.wrapped(
                    8 * s,
                    y + 6 * s,
                    "With it the phone sees your Zcash balance and prepares payments; each one is signed here.",
                    cols,
                    ws,
                );
                if let Some(phone) = self.link.phone {
                    let id = link::b64(&phone.key);
                    ui.p.text(8 * s, y + 6 * s, &format!("Phone {}", &id[..8]), ws);
                }
                ui.half_buttons(h * 3 / 5, ("Allow", Action::Allow), ("Refuse", Action::Refuse));
            }
            Screen::Closing => {
                ui.p.text_centered(h / 3, "Closing", 2 * s);
                let ws = (s - 1).max(2);
                ui.p.wrapped(
                    8 * s,
                    h / 3 + 30 * s,
                    "The Kindle's own screens come back in a moment (up to a minute). No need to restart it.",
                    (w - 16 * s) / (8 * ws),
                    ws,
                );
            }
            Screen::Busy(label) => {
                ui.p.text_centered(h / 3, &format!("{label}..."), s);
                ui.p.text_centered(h / 3 + 16 * s, "A moment", (s - 1).max(2));
            }
            Screen::Review(page) => {
                if let Some(review) = self.review.clone() {
                    ui.review(&review, page);
                }
            }
            Screen::Done { ok, note } => {
                ui.p.text_centered(h / 4, if ok { "Signed" } else { "Not signed" }, 2 * s);
                let ws = (s - 1).max(2);
                ui.p.wrapped(8 * s, h / 4 + 26 * s, &note, (w - 16 * s) / (8 * ws), ws);
                ui.button(h * 3 / 5, "OK", Action::DoneOk);
            }
            Screen::Nfts => {
                let g = self.gallery;
                let total = gallery_total;
                let per = NFT_COLS * NFT_ROWS;
                let pages = total.div_ceil(per).max(1);
                ui.band("< Back", &format!("{} {}/{pages}", if g.mine { "Mine" } else { "All" }, g.page + 1));
                ui.buttons.push((Rect { x: 0, y: 0, w, h: 22 * s }, Action::Back));
                let half = (w - 26 * s) / 2;
                ui.tab(
                    Rect { x: 10 * s, y: 25 * s, w: half, h: 18 * s },
                    "All 10,000",
                    Action::NftTab { mine: false },
                    !g.mine,
                );
                let mine_label = format!("Mine ({})", self.mine.len());
                ui.tab(
                    Rect { x: 16 * s + half, y: 25 * s, w: half, h: 18 * s },
                    &mine_label,
                    Action::NftTab { mine: true },
                    g.mine,
                );
                let top = 47 * s;
                let ws = (s - 1).max(2);
                let cols = (w - 16 * s) / (8 * ws);
                if g.mine && self.mine.is_empty() {
                    ui.p.wrapped(
                        8 * s,
                        top + 10 * s,
                        "None yet. Sign in on zilkroad.com in ZApp, then connect the phone: the zkSNARKs you hold show here.",
                        cols,
                        ws,
                    );
                } else {
                    if self.collection.is_none() {
                        ui.p.wrapped(
                            8 * s,
                            h - 50 * s,
                            "Portraits not on this Kindle (nft/build.py makes them).",
                            cols,
                            ws,
                        );
                    }
                    let cell_w = w / NFT_COLS;
                    let cell_h = 100 * s / 3;
                    for (k, &n) in gallery_page.iter().enumerate() {
                        let r =
                            Rect { x: (k % NFT_COLS) * cell_w, y: top + (k / NFT_COLS) * cell_h, w: cell_w, h: cell_h };
                        ui.portrait_tile(r, n, self.collection.as_ref());
                    }
                }
                let quarter = (w - 38 * s) / 4;
                for (i, (label, by)) in [("<<", -10), ("<", -1), (">", 1), (">>", 10)].into_iter().enumerate() {
                    let r = Rect { x: 10 * s + i * (quarter + 6 * s), y: h - 26 * s, w: quarter, h: 20 * s };
                    ui.rect_button(r, label, Action::NftPage(by));
                }
            }
            Screen::Nft(n) => {
                let yours = self.mine.contains(&n);
                ui.band("< Back", &format!("#{n}{}", if yours { "  yours" } else { "" }));
                ui.buttons.push((Rect { x: 0, y: 0, w, h: 22 * s }, Action::Back));
                let ws = (s - 1).max(2);
                let mut y = 26 * s;
                if let Some(c) = self.collection.as_ref() {
                    let scale = (w * 4 / 5) / c.side;
                    if let Some(img) = c.portrait(n, scale) {
                        ui.p.image((w - img.w) / 2, y, &img);
                        y += img.h + 4 * s;
                    }
                    if let Some((rank, traits)) = c.traits(n) {
                        ui.p.text(10 * s, y, &format!("zkSNARKs #{n}"), ws);
                        if !rank.is_empty() {
                            let label = format!("Rank {rank}");
                            ui.p.text(w - 10 * s - label.len() * 8 * ws, y, &label, ws);
                        }
                        y += 12 * ws;
                        for (name, value) in traits {
                            ui.p.text(10 * s, y, &format!("{name:<10}{value}"), ws);
                            y += 10 * ws;
                        }
                    }
                } else {
                    ui.p.text_centered(h / 3, &format!("zkSNARKs #{n}"), s);
                }
                ui.half_buttons(h - 26 * s, ("< Previous", Action::NftStep(false)), ("Next >", Action::NftStep(true)));
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
                };
                ui.back_band(coin.name(), title, Action::Back);
                let ws = (s - 1).max(2);
                let y = ui.p.wrapped(8 * s, 28 * s, about, (w - 16 * s) / (8 * ws), ws) + 4 * s;
                let y = if qr {
                    let size = (w / 2).min(h / 3);
                    ui.p.qr(w / 2, y, size, &value);
                    y + size
                } else {
                    y
                };
                if qr {
                    ui.p.wrapped(4 * s, y + 2 * s, &value, (w - 8 * s) / 16, 2);
                } else {
                    ui.numbered_words(y + 2 * s, &value);
                }
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
    labels: &'a mut Vec<(String, Rect)>,
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
        self.labels.push((label.to_string(), r));
    }

    /// A tab: black with white text when selected.
    fn tab(&mut self, r: Rect, label: &str, action: Action, selected: bool) {
        if !selected {
            return self.rect_button(r, label, action);
        }
        let ls = (self.s - 1).max(2);
        self.p.fill(r.x, r.y, r.w, r.h, true);
        let mut text = Page::new(r.w, r.h);
        let tw = label.chars().count() * 8 * ls;
        text.text(r.w.saturating_sub(tw) / 2, (r.h - 8 * ls) / 2, label, ls);
        for y in 0..r.h {
            for x in 0..r.w {
                if text.ink(x, y) {
                    self.p.fill(r.x + x, r.y + y, 1, 1, false);
                }
            }
        }
        self.buttons.push((r, action));
    }

    /// A zkSNARK in the grid: its portrait (or just its number) and number.
    fn portrait_tile(&mut self, r: Rect, n: u16, collection: Option<&nft::Collection>) {
        let ls = 2;
        let label = format!("#{n}");
        let lw = label.len() * 8 * ls;
        if let Some(img) = collection.and_then(|c| c.portrait(n, (r.w - 4 * self.s) / c.side)) {
            self.p.image(r.x + (r.w - img.w) / 2, r.y, &img);
            self.p.text(r.x + r.w.saturating_sub(lw) / 2, r.y + img.h + 3, &label, ls);
        } else {
            self.p.frame(r.x + 4, r.y, r.w - 8, r.h - 24, 2);
            self.p.text(r.x + r.w.saturating_sub(lw) / 2, r.y + (r.h - 24) / 2 - 8, &label, ls);
        }
        self.buttons.push((r, Action::NftOpen(n)));
    }

    /// A transaction's review: each payment, its memo, then the confirmation with
    /// the deliberate "Hold to sign".
    fn review(&mut self, r: &Review, page: u8) {
        let (s, w, h) = (self.s, self.w, self.h);
        let ws = (s - 1).max(2);
        let cols = (w - 16 * s) / (8 * ws);
        let of = r.payments.len();
        match r.page(page) {
            Some((i, false)) => {
                let p = &r.payments[i];
                self.p.text_centered(30 * s, &format!("Send {}/{of}", i + 1), s);
                let amount = zec(p.value);
                let big = (2 * s).min((w - 16 * s) / (8 * amount.chars().count())).max(ws);
                self.p.text_centered(46 * s, &amount, big);
                self.p.text(8 * s, 46 * s + 8 * big + 12 * s, "to", ws);
                self.p.wrapped(8 * s, 46 * s + 8 * big + 26 * s, &p.address, cols, ws);
            }
            Some((i, true)) => {
                self.p.text_centered(30 * s, &format!("Memo {}/{of}", i + 1), s);
                let memo = r.payments[i].memo.clone().unwrap_or_default();
                self.p.wrapped(8 * s, 50 * s, &memo, cols, ws);
            }
            None => {
                self.p.text_centered(30 * s, "Confirm", s);
                let paid = r.total_out() - r.fee;
                let rows = [
                    (if of > 1 { format!("Payments ({of})") } else { "Payment".into() }, zec(paid)),
                    ("Fee".into(), zec(r.fee)),
                    ("Total".into(), zec(r.total_out())),
                ];
                let mut y = 52 * s;
                for (n, (label, value)) in rows.iter().enumerate() {
                    if n == 2 {
                        self.p.fill(8 * s, y - 4 * s, w - 16 * s, s.max(2), true);
                        y += 2 * s;
                    }
                    self.p.text(8 * s, y, label, ws);
                    self.p.text(w - 8 * s - value.chars().count() * 8 * ws, y, value, ws);
                    y += 16 * ws;
                }
                let r = Rect { x: 10 * s, y: h - 84 * s, w: w - 20 * s, h: 26 * s };
                self.p.frame(r.x, r.y, r.w, r.h, 2 * s);
                self.p.text_centered(r.y + (r.h - 8 * s) / 2, "Hold to sign", s);
                self.buttons.push((r, Action::Sign));
                self.p.text_centered(r.y - 12 * ws, "Hold it 1.5 seconds", ws);
            }
        }
        let last = r.pages().saturating_sub(1);
        if page < last {
            self.half_buttons(
                h - 52 * s,
                ("< Previous", Action::ReviewPage(false)),
                ("Next >", Action::ReviewPage(true)),
            );
        } else {
            self.half_buttons(h - 52 * s, ("< Previous", Action::ReviewPage(false)), ("Refuse", Action::Refuse));
        }
        if page < last {
            self.button(h - 26 * s, "Refuse", Action::Refuse);
        }
    }

    /// The black band with "< `back`" on the left (the whole band goes back) and
    /// `right` on the right.
    fn back_band(&mut self, back: &str, right: &str, action: Action) {
        self.band(&format!("< {back}"), right);
        self.buttons.push((Rect { x: 0, y: 0, w: self.w, h: 22 * self.s }, action));
    }

    /// One entry of a menu: a large framed row, its title and a line under it;
    /// returns the y of the next row.
    fn menu_row(&mut self, y: usize, title: &str, sub: &str, action: Action) -> usize {
        let (s, w) = (self.s, self.w);
        let ls = (s - 1).max(2);
        let r = Rect { x: 8 * s, y, w: w - 16 * s, h: 30 * s };
        self.p.frame(r.x, r.y, r.w, r.h, 2);
        self.p.text(r.x + 6 * s, r.y + 6 * s, title, s);
        self.p.text(r.x + 6 * s, r.y + 6 * s + 10 * s, sub, ls);
        self.p.text(r.x + r.w - 14 * s, r.y + (r.h - 8 * s) / 2, ">", s);
        self.labels.push((title.to_string(), r));
        self.buttons.push((r, action));
        y + r.h + 4 * s
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

    /// Ethereum's tile: four EVM logos in a square, for the whole EVM family.
    fn evm_tile(&mut self, r: Rect) {
        self.p.frame(r.x, r.y, r.w, r.h, 2);
        let cell = r.w.min(r.h) / 2 - 6;
        for (k, key) in ["ethereum", "avalanche", "binance", "base"].iter().enumerate() {
            let logo = if *key == "ethereum" { Some(logos::logo(Coin::Ethereum)) } else { logos::evm_logo(key) };
            let Some(mut logo) = logo else { continue };
            while logo.w > cell {
                logo = logo.shrink(2);
            }
            let (cx, cy) = (r.x + r.w / 4 + (k % 2) * r.w / 2, r.y + r.h / 4 + (k / 2) * r.h / 2);
            self.p.image(cx - logo.w / 2, cy - logo.h / 2, &logo);
        }
        self.buttons.push((r, Action::Open(Coin::Ethereum)));
    }

    /// An EVM network in the list: its logo, name, chain id; returns the y below.
    fn network_row(&mut self, y: usize, net: &evm::EvmNetwork, action: Action) -> usize {
        let (s, w) = (self.s, self.w);
        let r = Rect { x: 8 * s, y, w: w - 16 * s, h: 30 * s };
        self.p.frame(r.x, r.y, r.w, r.h, 2);
        let mut x = r.x + 5 * s;
        if let Some(mut logo) = logos::evm_logo(net.key) {
            while logo.h > r.h - 6 * s {
                logo = logo.shrink(2);
            }
            self.p.image(x, r.y + (r.h - logo.h) / 2, &logo);
            x += logo.w + 5 * s;
        }
        self.p.text(x, r.y + 6 * s, net.name, s);
        self.p.text(x, r.y + 16 * s, &format!("{}  chain {}", net.symbol, net.chain_id), (s - 1).max(2));
        self.p.text(r.x + r.w - 14 * s, r.y + (r.h - 8 * s) / 2, ">", s);
        self.buttons.push((r, action));
        y + r.h + 4 * s
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

/// A line under the coin's name, on its menu.
fn coin_about(coin: Coin) -> &'static str {
    match coin {
        Coin::Bitcoin => "Native SegWit (bc1)",
        Coin::Zcash => "Shielded (Orchard)",
        Coin::Monero => "Private by default",
        Coin::Solana => "Ed25519 account",
        Coin::Quantus => "Post-quantum (ML-DSA)",
        Coin::Ethereum => "And every EVM network",
        Coin::Tron => "TRC-20 tokens",
        Coin::Xrp => "XRP Ledger account",
        Coin::Sui => "Move, Ed25519",
        Coin::Aptos => "Move, Ed25519",
        Coin::Near => "Implicit account",
        Coin::Stellar => "SEP-0005 account",
        Coin::Ton => "Wallet v4r2",
        Coin::Cardano => "Shelley base address",
        Coin::Starknet => "Argent account",
        Coin::Bittensor => "Substrate (sr25519)",
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

/// Zatoshis as ZEC, without trailing zeros.
pub fn zec(zats: u64) -> String {
    let (whole, frac) = (zats / zfirm_core::COIN, zats % zfirm_core::COIN);
    if frac == 0 {
        return format!("{whole} ZEC");
    }
    format!("{whole}.{} ZEC", format!("{frac:08}").trim_end_matches('0'))
}
