//! The zkSNARKs collection, offline: 10,000 portraits of 26 × 26 pixels packed by
//! nft/build.py into `nft/zksnarks.bin` and `nft/traits.txt`, and the token numbers
//! the phone says its zilkroad account holds (`nfts`, one number per line).

use std::path::Path;

use crate::logos::Gray;

pub const COUNT: u16 = 10_000;

pub struct Collection {
    pub side: usize,
    count: usize,
    pixels: Vec<u8>,
    /// Trait names, then per token: its rank and values (empty when not packed).
    pub names: Vec<String>,
    traits: Vec<Option<(String, Vec<String>)>>,
}

impl Collection {
    pub fn load(dir: &Path) -> Option<Collection> {
        let bin = std::fs::read(dir.join("nft/zksnarks.bin")).ok()?;
        if bin.len() < 8 || &bin[..4] != b"ZKS1" {
            return None;
        }
        let count = u16::from_le_bytes([bin[4], bin[5]]) as usize;
        let (w, h) = (bin[6] as usize, bin[7] as usize);
        if w != h || bin.len() != 8 + count * w * h {
            return None;
        }
        let mut names = Vec::new();
        let mut traits = vec![None; count];
        let text = std::fs::read_to_string(dir.join("nft/traits.txt")).unwrap_or_default();
        for (i, line) in text.lines().enumerate() {
            let mut cols = line.split('\t');
            if i == 0 {
                names = cols.skip(2).map(String::from).collect();
                continue;
            }
            let Some(n) = cols.next().and_then(|n| n.parse::<usize>().ok()) else { continue };
            let rank = cols.next().unwrap_or_default().to_string();
            if (1..=count).contains(&n) {
                traits[n - 1] = Some((rank, cols.map(String::from).collect()));
            }
        }
        Some(Collection { side: w, count, pixels: bin[8..].to_vec(), names, traits })
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Token `n`'s portrait (from 1), each pixel a `scale` × `scale` square.
    pub fn portrait(&self, n: u16, scale: usize) -> Option<Gray> {
        let n = n as usize;
        if !(1..=self.count).contains(&n) {
            return None;
        }
        let side = self.side;
        let src = &self.pixels[(n - 1) * side * side..n * side * side];
        let w = side * scale;
        let mut px = vec![0u8; w * w];
        for y in 0..w {
            for x in 0..w {
                px[y * w + x] = src[(y / scale) * side + x / scale];
            }
        }
        Some(Gray { w, h: w, px })
    }

    /// Token `n`'s rank and (trait, value) pairs.
    pub fn traits(&self, n: u16) -> Option<(String, Vec<(String, String)>)> {
        let (rank, values) = self.traits.get((n as usize).checked_sub(1)?)?.as_ref()?;
        Some((rank.clone(), self.names.iter().cloned().zip(values.iter().cloned()).collect()))
    }
}

/// The tokens the phone last said are the user's, in order.
pub fn mine(dir: &Path) -> Vec<u16> {
    std::fs::read_to_string(dir.join("nfts"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

pub fn save_mine(dir: &Path, tokens: &[u16]) -> std::io::Result<()> {
    let text: String = tokens.iter().map(|n| format!("{n}\n")).collect();
    std::fs::write(dir.join("nfts"), text)
}

/// The `NFTS` payload: a u16 count, then that many u16 token numbers, all
/// little-endian like every zfirm field. Returns the valid numbers (1..=10000),
/// sorted, once each; `None` when the count does not match the length.
pub fn parse_tokens(payload: &[u8]) -> Option<Vec<u16>> {
    let (count, rest) = payload.split_first_chunk::<2>()?;
    if u16::from_le_bytes(*count) as usize * 2 != rest.len() {
        return None;
    }
    let mut tokens: Vec<u16> =
        rest.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).filter(|t| (1..=COUNT).contains(t)).collect();
    tokens.sort_unstable();
    tokens.dedup();
    Some(tokens)
}
