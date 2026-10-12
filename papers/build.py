"""Each coin's whitepaper, laid out again for the Kindle's 600 x 800 e-ink screen.

PDFs are reflowed the way k2pdfopt does it: every page is rendered at 300 dpi, cut
into columns, lines and words, and the word images are set again in bigger, shorter
lines; formulas and figures stay pictures. Markdown and HTML sources are typeset with
Literata (an OFL e-reading font). Each page is a grayscale PNG, papers/<TICKER>/NNN.png,
that KindleWallet shows under its own header.

    uv run --with pillow --with numpy papers/build.py CACHE_DIR OUT_DIR FONT_DIR

Needs pdftoppm (poppler). The papers are downloaded from their projects' own sites
into CACHE_DIR; they are not kept in this repository.
"""

import html
import re
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

import numpy as np
from PIL import Image, ImageFont, ImageDraw

W, H = 600, 734  # the screen under KindleWallet's 66 px header
MARGIN = 18
TEXT_W = W - 2 * MARGIN
LINE_H = 27  # a reflowed text line's height on the screen
GAP = 9  # between lines
PARA = 14  # extra space between paragraphs

PAPERS = [
    ("BTC", "https://bitcoin.org/bitcoin.pdf", "pdf"),
    ("ETH", "https://ethereum.org/content/whitepaper/whitepaper-pdf/Ethereum_Whitepaper_-_Buterin_2014.pdf", "pdf"),
    ("XRP", "https://arxiv.org/pdf/1802.07242", "pdf"),
    ("ZEC", "http://zerocash-project.org/media/pdf/zerocash-oakland2014.pdf", "pdf"),
    ("BNB", "https://raw.githubusercontent.com/bnb-chain/whitepaper/master/WHITEPAPER.md", "md"),
    ("SOL", "https://solana.com/solana-whitepaper.pdf", "pdf"),
    ("XMR", "https://www.getmonero.org/resources/research-lab/pubs/cryptonote-whitepaper.pdf", "pdf"),
    ("TRX", "https://tron.network/static/doc/white_paper_v_2_0.pdf", "pdf"),
    ("ARB", "https://raw.githubusercontent.com/OffchainLabs/nitro/master/docs/Nitro-whitepaper.pdf", "pdf"),
    ("QTC", "https://www.quantus.com/whitepaper", "html"),
]


def fetch(url: str, path: Path) -> Path:
    if not path.exists():
        req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
        path.write_bytes(urllib.request.urlopen(req, timeout=60).read())
    return path


class Pages:
    """Fills screens top to bottom with pictures (word images or set text)."""

    def __init__(self):
        self.pages: list[Image.Image] = []
        self.new_page()
        self.line: list[Image.Image] = []  # pictures waiting on the current line
        self.line_w = 0

    def new_page(self):
        self.page = Image.new("L", (W, H), 255)
        self.pages.append(self.page)
        self.y = MARGIN

    def place(self, img: Image.Image, x: int):
        if self.y + img.height > H - MARGIN and self.y > MARGIN:
            self.new_page()
        self.page.paste(img, (x, self.y))
        self.y += img.height

    def word(self, img: Image.Image, space: int):
        if img.width > TEXT_W:
            img = img.resize((TEXT_W, max(1, img.height * TEXT_W // img.width)), Image.LANCZOS)
        if self.line and self.line_w + space + img.width > TEXT_W:
            self.end_line()
        if self.line:
            self.line_w += space
        self.line.append((img, self.line_w))
        self.line_w += img.width

    def end_line(self, extra: int = 0):
        if not self.line:
            self.y += extra
            return
        h = max(img.height for img, _ in self.line)
        if self.y + h > H - MARGIN:
            self.new_page()
        for img, x in self.line:
            self.page.paste(img, (MARGIN + x, self.y))
        self.y += h + GAP + extra
        self.line, self.line_w = [], 0

    def paragraph(self):
        self.end_line(PARA)

    def block(self, img: Image.Image):
        """A figure, table or display formula: whole, at most the text width."""
        self.paragraph()
        if img.width > TEXT_W:
            img = img.resize((TEXT_W, max(1, img.height * TEXT_W // img.width)), Image.LANCZOS)
        if img.height > H - 2 * MARGIN:
            img = img.resize((max(1, img.width * (H - 2 * MARGIN) // img.height), H - 2 * MARGIN), Image.LANCZOS)
        self.place(img, (W - img.width) // 2)
        self.y += PARA


# ---- PDF reflow ----


def runs(mask: np.ndarray, min_gap: int) -> list[tuple[int, int]]:
    """[start, end) of the True runs, merging runs split by gaps shorter than min_gap."""
    out: list[list[int]] = []
    idx = np.flatnonzero(mask)
    for i in idx:
        if out and i - out[-1][1] < min_gap:
            out[-1][1] = i + 1
        elif out and i == out[-1][1]:
            out[-1][1] = i + 1
        else:
            out.append([i, i + 1])
    return [(a, b) for a, b in out]


def columns(ink: np.ndarray) -> list[tuple[int, int, int, int]]:
    """Regions (top, bottom, left, right) in reading order: full-width blocks, and
    two-column blocks as left then right."""
    h, w = ink.shape
    rows = ink.any(axis=1)
    total = max(1, int(rows.sum()))
    # The gutter: the widest band in the middle of the page with (almost) no ink on
    # any row; a title across both columns is allowed.
    lo, hi = int(w * 0.35), int(w * 0.65)
    # Measured below the top quarter, where a first page's title crosses it.
    lower = ink[h // 4:]
    per_x = lower[:, lo:hi].sum(axis=0)
    gutter = None
    gaps = runs(per_x <= max(1, int(lower.any(axis=1).sum())) * 0.02, 1)
    if gaps:
        a, b = max(gaps, key=lambda g: g[1] - g[0])
        left_ink = ink[:, : lo + a].any(axis=0).sum()
        right_ink = ink[:, lo + b:].any(axis=0).sum()
        if b - a >= w * 0.01 and left_ink > w * 0.2 and right_ink > w * 0.2:
            gutter = (lo + a, lo + b)
    if gutter is None:
        return [(0, h, 0, w)]
    ga, gb = gutter
    # Blocks: rows apart by white space; a block is two-column when its gutter is white.
    out = []
    for top, bottom in runs(rows, max(12, h // 150)):
        # Text across both columns covers the gutter; stray ink at its edges does not.
        if ink[top:bottom, ga:gb].any(axis=0).mean() > 0.3:
            out.append((top, bottom, 0, w))
        elif len(out) >= 2 and out[-1][2] == gb and out[-2][3] == ga:
            # Joins the previous two-column block: left after left, right after right.
            out[-2:] = [(out[-2][0], bottom, 0, ga), (out[-1][0], bottom, gb, w)]
        else:
            out += [(top, bottom, 0, ga), (top, bottom, gb, w)]
    return out


def text_lines(region: np.ndarray) -> list[tuple[int, int]]:
    """Rows of text: runs of inked rows, with slivers (the dots of i and j, accents,
    underlines) joined to the nearest line instead of standing alone."""
    lines = [list(r) for r in runs(region.any(axis=1), 1)]
    if not lines:
        return []
    heights = sorted(b - a for a, b in lines)
    med = heights[len(heights) * 2 // 3]
    out: list[list[int]] = []
    for line in lines:
        small = line[1] - line[0] < med * 0.4
        if out and (small or out[-1][1] - out[-1][0] < med * 0.4) and line[0] - out[-1][1] < med * 0.5:
            out[-1][1] = line[1]
        else:
            out.append(line)
    return [(a, b) for a, b in out]


def word_gap(line: np.ndarray, med: int) -> int:
    """The smallest gap that separates words on this line: letters sit closer than
    words, so the threshold goes where the sorted gaps jump the most."""
    cols = line.any(axis=0)
    gaps = [b - a for a, b in runs(~cols, 1)]
    if cols.any():
        first, last = np.flatnonzero(cols)[[0, -1]]
        gaps = [b - a for a, b in runs(~cols[first:last + 1], 1)]
    lo, hi = max(2, int(med * 0.2)), int(med * 0.5)
    gaps = sorted(g for g in gaps if g >= 2)
    # A clear jump (the word gap at least 1.6 times the letter gap) decides; a line
    # too short to show one falls back to a fixed share of the line height.
    best, cut = 0.0, int(med * 0.3)
    for g1, g2 in zip(gaps, gaps[1:]):
        if lo <= g2 <= hi and g2 >= 1.6 * g1 and g2 / max(1, g1) > best:
            best, cut = g2 / max(1, g1), g2
    return max(lo, min(hi, cut))


def reflow_page(gray: np.ndarray, pages: Pages, scale_hint: list):
    ink = gray < 120
    h, w = ink.shape
    # Running headers and page numbers: drop the outer 4% top and bottom.
    ink[: int(h * 0.04)] = False
    ink[int(h * 0.96):] = False
    for top, bottom, left, right in columns(ink):
        region = ink[top:bottom, left:right]
        lines = text_lines(region)
        if not lines:
            continue
        heights = sorted(b - a for a, b in lines)
        med = max(8, heights[len(heights) // 2])
        if not scale_hint:
            scale_hint.append(LINE_H / med)
        scale = scale_hint[0]
        cols = np.flatnonzero(region.any(axis=0))
        x_left, x_right = (int(cols[0]), int(cols[-1])) if len(cols) else (0, right - left)
        full = x_left + (x_right - x_left) * 0.75
        between = sorted(lines[i + 1][0] - lines[i][1] for i in range(len(lines) - 1))
        line_gap = between[len(between) // 2] if between else med
        prev_end = prev_start = None
        for a, b in lines:
            line = region[a:b]
            if b - a > 2.4 * med:
                xs = np.flatnonzero(line.any(axis=0))
                img = Image.fromarray(gray[top + a: top + b, left + xs[0]: left + xs[-1] + 1])
                pages.block(img.resize((max(1, int(img.width * scale)), max(1, int(img.height * scale))), Image.LANCZOS))
                prev_end = prev_start = None
                continue
            words = runs(line.any(axis=0), word_gap(line, med))
            if not words:
                continue
            start, end = words[0][0], words[-1][1]
            # A first-line indent: further in than the line before (a whole indented
            # block, like an abstract, is not one).
            indented = prev_start is not None and start - prev_start > med * 0.9
            if prev_end is not None and (indented or prev_end < full):
                pages.paragraph()
            if a > 0 and prev_end is not None and a - prev_line_bottom > line_gap * 1.6 + 4:
                pages.paragraph()
            for wa, wb in words:
                img = Image.fromarray(gray[top + a: top + b, left + wa: left + wb])
                img = img.resize((max(1, int(img.width * scale)), max(1, int(img.height * scale))), Image.LANCZOS)
                pages.word(img, int(med * 0.33 * scale) + 2)
            prev_end, prev_line_bottom, prev_start = end, b, start
    pages.paragraph()


def pdf(path: Path, pages: Pages):
    with tempfile.TemporaryDirectory() as tmp:
        subprocess.run(["pdftoppm", "-r", "300", "-gray", "-png", str(path), f"{tmp}/p"], check=True)
        files = sorted(Path(tmp).glob("p-*.png"), key=lambda p: int(p.stem.split("-")[1]))
        # One scale for the whole paper, from the usual height of its text lines.
        heights = []
        for png in files:
            ink = np.array(Image.open(png).convert("L")) < 120
            heights += [b - a for a, b in text_lines(ink)]
        heights.sort()
        body = heights[len(heights) // 2] if heights else LINE_H
        hint = [LINE_H / max(8, body)]
        for png in files:
            reflow_page(np.array(Image.open(png).convert("L")), pages, hint)


# ---- set text (Markdown, HTML) ----


class Fonts:
    def __init__(self, font_dir: Path):
        self.dir = font_dir

    def get(self, size: int, weight: str = "Regular", italic: bool = False):
        f = ImageFont.truetype(str(self.dir / ("Literata-Italic.ttf" if italic else "Literata.ttf")), size)
        f.set_variation_by_name(f"{weight} Italic" if italic else weight)
        return f


def set_text(blocks: list[tuple[str, str]], pages: Pages, fonts: Fonts):
    """blocks: (kind, text), kind h1/h2/h3/p/li/pre."""
    styles = {
        "h1": (fonts.get(32, "Bold"), 18),
        "h2": (fonts.get(26, "Bold"), 14),
        "h3": (fonts.get(22, "SemiBold"), 10),
        "p": (fonts.get(22, "Regular"), 8),
        "li": (fonts.get(22, "Regular"), 3),
        "pre": (fonts.get(16, "Regular"), 2),
    }
    for kind, text in blocks:
        font, after = styles[kind]
        if kind in ("h1", "h2", "h3"):
            pages.end_line(PARA)
        asc, desc = font.getmetrics()
        lh = asc + desc
        words = text.split()
        if kind == "li":
            words = ["•"] + words
        space = int(font.getlength(" "))
        for word in words:
            wpx = int(font.getlength(word)) + 1
            img = Image.new("L", (max(1, wpx), lh), 255)
            ImageDraw.Draw(img).text((0, 0), word, font=font, fill=0)
            pages.word(img, space)
        pages.end_line(after)


def markdown(text: str) -> list[tuple[str, str]]:
    blocks, para = [], []

    def flush():
        if para:
            blocks.append(("p", " ".join(para)))
            para.clear()

    skip_toc = False
    for line in text.splitlines():
        s = line.strip()
        s = re.sub(r"!\[[^]]*\]\([^)]*\)", "", s)  # images
        s = re.sub(r"\[([^]]*)\]\([^)]*\)", r"\1", s)  # links
        s = re.sub(r"[*_`]{1,3}", "", s)
        m = re.match(r"(#{1,3})\s+(.*)", s)
        if m:
            flush()
            skip_toc = m.group(2).lower().startswith("table of contents")
            if not skip_toc:
                blocks.append((f"h{len(m.group(1))}", m.group(2)))
            continue
        if skip_toc:
            continue
        if not s:
            flush()
        elif re.match(r"[-*+]\s+|\d+\.\s+", s):
            flush()
            blocks.append(("li", re.sub(r"^([-*+]|\d+\.)\s+", "", s)))
        elif s.startswith("|"):
            flush()
            blocks.append(("pre", " ".join(c.strip() for c in s.strip("|").split("|") if c.strip() and not set(c.strip()) <= set("-:"))))
        else:
            para.append(s)
    flush()
    return [b for b in blocks if b[1].strip()]


def html_article(text: str) -> list[tuple[str, str]]:
    text = re.sub(r"<(script|style|svg|nav|header|footer)\b.*?</\1>", "", text, flags=re.S | re.I)
    m = re.search(r"<article\b.*?</article>", text, flags=re.S | re.I)
    body = m.group(0) if m else text
    blocks = []
    for tag, inner in re.findall(r"<(h1|h2|h3|p|li)\b[^>]*>(.*?)</\1>", body, flags=re.S | re.I):
        t = html.unescape(re.sub(r"<[^>]+>", "", inner))
        t = re.sub(r"\s+", " ", t).strip()
        if t:
            blocks.append((tag.lower(), t))
    return blocks


def main():
    cache, out, font_dir = (Path(a) for a in sys.argv[1:4])
    only = set(sys.argv[4:])
    cache.mkdir(parents=True, exist_ok=True)
    fonts = Fonts(font_dir)
    for ticker, url, kind in PAPERS:
        if only and ticker not in only:
            continue
        src = fetch(url, cache / f"{ticker.lower()}.{kind}")
        pages = Pages()
        if kind == "pdf":
            pdf(src, pages)
        elif kind == "md":
            set_text(markdown(src.read_text()), pages, fonts)
        else:
            set_text(html_article(src.read_text()), pages, fonts)
        pages.end_line()
        if pages.y <= MARGIN and len(pages.pages) > 1:
            pages.pages.pop()
        d = out / ticker
        d.mkdir(parents=True, exist_ok=True)
        for old in d.glob("*.png"):
            old.unlink()
        # E-ink shows light gray as almost nothing: darken the ink, keep paper white.
        curve = [int(255 * (v / 255) ** 2.2) for v in range(256)]
        for n, page in enumerate(pages.pages, 1):
            page.point(curve).save(d / f"{n:03}.png", optimize=True)
        print(f"{ticker}: {len(pages.pages)} pages")


if __name__ == "__main__":
    main()
