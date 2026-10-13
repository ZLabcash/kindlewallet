"""The zkSNARKs collection, packed for KindleWallet's offline gallery.

Every one of the 10,000 portraits is 26 x 26 pixels (served as a 520 px PNG, each
pixel a 20 px square). Each is brought back to its 26 x 26 pixels, turned to gray
(ITU-R BT.601 luma) and packed into one file the Kindle reads directly:

    nft/zksnarks.bin   "ZKS1" || count (u16 LE) || width (u8) || height (u8)
                       || count x width x height gray bytes (0 black, 255 white),
                       token n at index n - 1
    nft/traits.txt     a header line (tab-separated: "#", "Rank", then the trait
                       names), then one line per token: number, rank, values

    uv run --with pillow nft/build.py CACHE_DIR OUT_DIR

Images come from api.zscan.cash, traits from zilkroad's market list; both are
downloaded once into CACHE_DIR (eight requests at a time at most). OUT_DIR is
normally kindle/kindlewallet; the packed files are not kept in this repository.
"""

import io
import json
import struct
import sys
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from PIL import Image

COUNT = 10_000
SIDE = 26
IMAGE = "https://api.zscan.cash/v1/collections/zksnarks/items/{n}/image"
TOKENS = "https://zilkroad.com/api/market/tokens"
WORKERS = 8
# Shown on the Kindle in this order; Background is black for every token.
TRAITS = ["Lineage", "Skin", "Eyes", "Head", "Accessory"]


def fetch(url: str, path: Path) -> bytes:
    if path.exists() and path.stat().st_size > 0:
        return path.read_bytes()
    for attempt in range(7):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "KindleWallet nft/build.py"})
            data = urllib.request.urlopen(req, timeout=60).read()
            path.write_bytes(data)
            return data
        except Exception as e:  # network hiccup or rate limit: back off, try again
            wait = 2 ** attempt
            print(f"{url}: {e}, again in {wait} s", file=sys.stderr)
            time.sleep(wait)
    raise SystemExit(f"could not download {url}")


def portrait(png: bytes) -> bytes:
    """The 26 x 26 gray pixels of a served portrait (any whole multiple of 26)."""
    img = Image.open(io.BytesIO(png)).convert("RGB")
    cell = img.width // SIDE
    if img.width != img.height or cell * SIDE != img.width:
        raise SystemExit(f"unexpected image size {img.size}")
    # Nearest neighbour takes each square's centre pixel.
    small = img.resize((SIDE, SIDE), Image.NEAREST)
    return small.convert("L").tobytes()


def main():
    cache, out = Path(sys.argv[1]), Path(sys.argv[2])
    (cache / "images").mkdir(parents=True, exist_ok=True)
    (out / "nft").mkdir(parents=True, exist_ok=True)

    tokens = json.loads(fetch(TOKENS, cache / "tokens.json"))["tokens"]
    by_number = {t["id"]: t for t in tokens}

    def one(n: int) -> bytes:
        return portrait(fetch(IMAGE.format(n=n), cache / "images" / f"{n}.png"))

    with ThreadPoolExecutor(WORKERS) as pool:
        pixels = []
        for n, px in enumerate(pool.map(one, range(1, COUNT + 1)), 1):
            pixels.append(px)
            if n % 500 == 0:
                print(f"{n}/{COUNT}", file=sys.stderr)

    (out / "nft" / "zksnarks.bin").write_bytes(b"ZKS1" + struct.pack("<HBB", COUNT, SIDE, SIDE) + b"".join(pixels))
    lines = ["\t".join(["#", "Rank"] + TRAITS)]
    for n in range(1, COUNT + 1):
        t = by_number.get(n)
        if t is None:
            continue
        values = [str(t["traits"].get(name, "")).replace("\t", " ") for name in TRAITS]
        lines.append("\t".join([str(n), str(t.get("rank") or "")] + values))
    (out / "nft" / "traits.txt").write_text("\n".join(lines) + "\n")
    print(f"packed {COUNT} portraits into {out / 'nft'}", file=sys.stderr)


if __name__ == "__main__":
    main()
