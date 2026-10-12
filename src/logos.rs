//! Each coin's logo, from its official monochrome SVG (assets/logos), rendered
//! once to a 112 px grayscale PNG for the e-ink screen; and the grayscale pictures
//! KindleWallet draws (whitepaper pages too).

use crate::coins::Coin;

/// A grayscale picture: 0 is black, 255 white.
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Gray {
    fn decode(bytes: &[u8]) -> Gray {
        let mut reader = png::Decoder::new(bytes).read_info().expect("embedded png");
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).expect("embedded png");
        let (w, h) = (info.width as usize, info.height as usize);
        let step = info.color_type.samples();
        let px = buf[..info.buffer_size()].chunks(step).map(|c| c[0]).collect();
        Gray { w, h, px }
    }

    /// A grayscale PNG from disk (a whitepaper page).
    pub fn load(path: &std::path::Path) -> Option<Gray> {
        let bytes = std::fs::read(path).ok()?;
        let mut reader = png::Decoder::new(bytes.as_slice()).read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).ok()?;
        let step = info.color_type.samples();
        let px = buf[..info.buffer_size()].chunks(step).map(|c| c[0]).collect();
        Some(Gray { w: info.width as usize, h: info.height as usize, px })
    }

    /// Shrunk by a whole factor, averaging each square.
    pub fn shrink(&self, by: usize) -> Gray {
        let (w, h) = (self.w / by, self.h / by);
        let mut px = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0usize;
                for dy in 0..by {
                    for dx in 0..by {
                        sum += self.px[(y * by + dy) * self.w + x * by + dx] as usize;
                    }
                }
                px.push((sum / (by * by)) as u8);
            }
        }
        Gray { w, h, px }
    }
}

pub fn logo(coin: Coin) -> Gray {
    Gray::decode(match coin {
        Coin::Bitcoin => include_bytes!("../assets/logos/bitcoin.png"),
        Coin::Ethereum => include_bytes!("../assets/logos/ethereum.png"),
        Coin::Xrp => include_bytes!("../assets/logos/xrp.png"),
        Coin::Zcash => include_bytes!("../assets/logos/zcash.png"),
        Coin::Bnb => include_bytes!("../assets/logos/binance.png"),
        Coin::Solana => include_bytes!("../assets/logos/solana.png"),
        Coin::Monero => include_bytes!("../assets/logos/monero.png"),
        Coin::Tron => include_bytes!("../assets/logos/tron.png"),
        Coin::Robinhood => include_bytes!("../assets/logos/robinhood.png"),
        Coin::Base => include_bytes!("../assets/logos/base.png"),
        Coin::Arbitrum => include_bytes!("../assets/logos/arbitrum.png"),
        Coin::Quantus => include_bytes!("../assets/logos/quantus.png"),
    })
}
