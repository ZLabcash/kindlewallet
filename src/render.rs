//! A page for the e-ink screen: big pixel text, QR codes and grayscale logos, saved
//! as a grayscale PNG that the Kindle's own `eips -g` draws.

use font8x8::UnicodeFonts;

use crate::logos::Gray;

pub struct Page {
    pub w: usize,
    pub h: usize,
    /// 0 is black, 255 white.
    gray: Vec<u8>,
}

impl Page {
    pub fn new(w: usize, h: usize) -> Self {
        Page { w, h, gray: vec![255; w * h] }
    }

    pub fn ink(&self, x: usize, y: usize) -> bool {
        self.gray[y * self.w + x] < 128
    }

    pub fn fill(&mut self, x: usize, y: usize, w: usize, h: usize, ink: bool) {
        for yy in y..(y + h).min(self.h) {
            for xx in x..(x + w).min(self.w) {
                self.gray[yy * self.w + xx] = if ink { 0 } else { 255 };
            }
        }
    }

    /// Darkens the page with `img`, its top-left corner at (x, y).
    pub fn image(&mut self, x: usize, y: usize, img: &Gray) {
        for iy in 0..img.h.min(self.h.saturating_sub(y)) {
            for ix in 0..img.w.min(self.w.saturating_sub(x)) {
                let p = &mut self.gray[(y + iy) * self.w + x + ix];
                *p = (*p).min(img.px[iy * img.w + ix]);
            }
        }
    }

    pub fn frame(&mut self, x: usize, y: usize, w: usize, h: usize, t: usize) {
        self.fill(x, y, w, t, true);
        self.fill(x, y + h - t, w, t, true);
        self.fill(x, y, t, h, true);
        self.fill(x + w - t, y, t, h, true);
    }

    /// Text with 8×8 glyphs scaled by `scale`; returns the width drawn.
    pub fn text(&mut self, x: usize, y: usize, s: &str, scale: usize) -> usize {
        for (n, c) in s.chars().enumerate() {
            let Some(glyph) = font8x8::BASIC_FONTS.get(c) else { continue };
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits >> col & 1 == 1 {
                        self.fill(x + (n * 8 + col) * scale, y + row * scale, scale, scale, true);
                    }
                }
            }
        }
        s.chars().count() * 8 * scale
    }

    pub fn text_centered(&mut self, y: usize, s: &str, scale: usize) {
        let w = s.chars().count() * 8 * scale;
        self.text(self.w.saturating_sub(w) / 2, y, s, scale);
    }

    /// Wraps `s` into lines of at most `cols` characters, at spaces when it has
    /// any (prose), anywhere otherwise (addresses, keys); returns the next y.
    pub fn wrapped(&mut self, x: usize, mut y: usize, s: &str, cols: usize, scale: usize) -> usize {
        let cols = cols.max(1);
        let mut lines: Vec<String> = Vec::new();
        if s.contains(' ') {
            let mut line = String::new();
            for word in s.split(' ') {
                if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > cols {
                    lines.push(std::mem::take(&mut line));
                }
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
            }
            lines.push(line);
        } else {
            let chars: Vec<char> = s.chars().collect();
            lines = chars.chunks(cols).map(|c| c.iter().collect()).collect();
        }
        for line in lines {
            self.text(x, y, &line, scale);
            y += 10 * scale;
        }
        y
    }

    /// A QR code `size` pixels wide (rounded down to whole modules), centred on `cx`.
    pub fn qr(&mut self, cx: usize, y: usize, size: usize, data: &str) -> bool {
        let Some(qr) = zfirm_core::qr::Qr::encode(data.as_bytes()) else { return false };
        let n = qr.size + 8; // quiet zone of 4 modules each side
        let m = (size / n).max(1);
        let x0 = cx.saturating_sub(n * m / 2);
        for qy in 0..qr.size {
            for qx in 0..qr.size {
                if qr.get(qx, qy) {
                    self.fill(x0 + (qx + 4) * m, y + (qy + 4) * m, m, m, true);
                }
            }
        }
        true
    }

    pub fn png(&self) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.w as u32, self.h as u32);
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().expect("png header");
            writer.write_image_data(&self.gray).expect("png data");
        }
        out
    }
}
