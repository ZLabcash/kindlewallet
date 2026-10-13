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
        Coin::Zcash => include_bytes!("../assets/logos/zcash.png"),
        Coin::Solana => include_bytes!("../assets/logos/solana.png"),
        Coin::Monero => include_bytes!("../assets/logos/monero.png"),
        Coin::Tron => include_bytes!("../assets/logos/tron.png"),
        Coin::Xrp => include_bytes!("../assets/logos/xrp.png"),
        Coin::Sui => include_bytes!("../assets/logos/sui.png"),
        Coin::Aptos => include_bytes!("../assets/logos/aptos.png"),
        Coin::Near => include_bytes!("../assets/logos/near.png"),
        Coin::Stellar => include_bytes!("../assets/logos/stellar.png"),
        Coin::Ton => include_bytes!("../assets/logos/ton.png"),
        Coin::Cardano => include_bytes!("../assets/logos/cardano.png"),
        Coin::Starknet => include_bytes!("../assets/logos/starknet.png"),
        Coin::Bittensor => include_bytes!("../assets/logos/bittensor.png"),
        Coin::Quantus => include_bytes!("../assets/logos/quantus.png"),
    })
}

/// An EVM network's logo, by its `evm::EvmNetwork::key`.
pub fn evm_logo(key: &str) -> Option<Gray> {
    let bytes: &[u8] = match key {
        "ethereum" => include_bytes!("../assets/logos/ethereum.png"),
        "base" => include_bytes!("../assets/logos/base.png"),
        "binance" => include_bytes!("../assets/logos/binance.png"),
        "arbitrum" => include_bytes!("../assets/logos/arbitrum.png"),
        "hyperevm" => include_bytes!("../assets/logos/hyperevm.png"),
        "robinhood" => include_bytes!("../assets/logos/robinhood.png"),
        "monad" => include_bytes!("../assets/logos/monad.png"),
        "polygon" => include_bytes!("../assets/logos/polygon.png"),
        "avalanche" => include_bytes!("../assets/logos/avalanche.png"),
        "plasma" => include_bytes!("../assets/logos/plasma.png"),
        "optimism" => include_bytes!("../assets/logos/optimism.png"),
        "ink" => include_bytes!("../assets/logos/ink.png"),
        "cronos" => include_bytes!("../assets/logos/cronos.png"),
        "mantle" => include_bytes!("../assets/logos/mantle.png"),
        "gnosis" => include_bytes!("../assets/logos/gnosis.png"),
        "linea" => include_bytes!("../assets/logos/linea.png"),
        "sonic" => include_bytes!("../assets/logos/sonic.png"),
        "scroll" => include_bytes!("../assets/logos/scroll.png"),
        "worldchain" => include_bytes!("../assets/logos/worldchain.png"),
        "unichain" => include_bytes!("../assets/logos/unichain.png"),
        "berachain" => include_bytes!("../assets/logos/berachain.png"),
        "zksync" => include_bytes!("../assets/logos/zksync.png"),
        "celo" => include_bytes!("../assets/logos/celo.png"),
        "sei" => include_bytes!("../assets/logos/sei.png"),
        _ => return None,
    };
    Some(Gray::decode(bytes))
}
